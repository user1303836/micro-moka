//! Provides a *not* thread-safe cache implementation built upon
//! [`hashbrown::HashTable`][hb-hashtable].
//!
//! [hb-hashtable]: https://docs.rs/hashbrown/latest/hashbrown/struct.HashTable.html

mod builder;
mod cache;
mod iter;

use std::num::NonZeroU32;

pub use builder::CacheBuilder;
pub use cache::Cache;
pub use iter::Iter;

const SENTINEL: u32 = u32::MAX;
const PREV_NONE: u32 = 1;
const VISITED_MASK: u32 = 1 << 31;
const PREV_MASK: u32 = !VISITED_MASK;
const MAX_SLAB_LEN: usize = PREV_MASK as usize - 1;
pub(crate) const DEFAULT_ADMISSION_SCAN_LIMIT: u32 = 16;

pub(crate) struct SlabEntry<K, V> {
    pub(crate) key: K,
    pub(crate) value: V,
    pub(crate) hash: u64,
    // The high bit is SIEVE's visited flag. The remaining nonzero value
    // encodes prev + 2, reserving 1 for no predecessor and preserving an
    // Option niche so vacant slab slots add no discriminant bytes.
    prev_visited: NonZeroU32,
    next: u32,
}

impl<K, V> SlabEntry<K, V> {
    fn new(key: K, value: V, hash: u64) -> Self {
        Self {
            key,
            value,
            hash,
            prev_visited: NonZeroU32::new(PREV_NONE).unwrap(),
            next: SENTINEL,
        }
    }

    #[inline]
    fn prev(&self) -> u32 {
        let link = self.prev_visited.get() & PREV_MASK;
        if link == PREV_NONE {
            SENTINEL
        } else {
            link - 2
        }
    }

    #[inline]
    fn set_prev(&mut self, index: u32) {
        let link = if index == SENTINEL {
            PREV_NONE
        } else {
            debug_assert!(index <= PREV_MASK - 2);
            index + 2
        };
        let visited = self.prev_visited.get() & VISITED_MASK;
        self.prev_visited = NonZeroU32::new(visited | link).unwrap();
    }

    #[inline]
    fn is_visited(&self) -> bool {
        self.prev_visited.get() & VISITED_MASK != 0
    }

    #[inline]
    fn mark_visited(&mut self) {
        self.prev_visited = NonZeroU32::new(self.prev_visited.get() | VISITED_MASK).unwrap();
    }

    #[inline]
    fn clear_visited(&mut self) {
        self.prev_visited = NonZeroU32::new(self.prev_visited.get() & !VISITED_MASK).unwrap();
    }
}

pub(crate) struct Slab<K, V> {
    pub(crate) entries: Vec<Option<SlabEntry<K, V>>>,
    free_list: Vec<u32>,
}

impl<K, V> Slab<K, V> {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            free_list: Vec::new(),
        }
    }

    fn with_capacity(cap: usize) -> Self {
        Self {
            entries: Vec::with_capacity(cap),
            free_list: Vec::new(),
        }
    }

    fn allocate(&mut self, entry: SlabEntry<K, V>) -> u32 {
        if let Some(idx) = self.free_list.pop() {
            self.entries[idx as usize] = Some(entry);
            idx
        } else {
            assert!(
                self.entries.len() < MAX_SLAB_LEN,
                "cache cannot contain more than {MAX_SLAB_LEN} entry slots"
            );
            let idx = self.entries.len() as u32;
            self.entries.push(Some(entry));
            idx
        }
    }

    fn deallocate(&mut self, index: u32) -> SlabEntry<K, V> {
        let entry = self.entries[index as usize]
            .take()
            .expect("deallocate called on empty slot");
        self.free_list.push(index);
        entry
    }

    #[inline]
    fn get(&self, index: u32) -> &SlabEntry<K, V> {
        self.entries[index as usize]
            .as_ref()
            .expect("get called on empty slot")
    }

    #[inline]
    fn get_mut(&mut self, index: u32) -> &mut SlabEntry<K, V> {
        self.entries[index as usize]
            .as_mut()
            .expect("get_mut called on empty slot")
    }

    #[inline]
    fn iter(&self) -> impl Iterator<Item = (u32, &SlabEntry<K, V>)> {
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(i, slot)| slot.as_ref().map(|e| (i as u32, e)))
    }
}

pub(crate) struct IndexDeque {
    head: u32,
    tail: u32,
    hand: u32,
}

impl Default for IndexDeque {
    fn default() -> Self {
        Self {
            head: SENTINEL,
            tail: SENTINEL,
            hand: SENTINEL,
        }
    }
}

impl IndexDeque {
    fn push_back<K, V>(&mut self, slab: &mut Slab<K, V>, index: u32) {
        let entry = slab.get_mut(index);
        entry.set_prev(self.tail);
        entry.next = SENTINEL;

        if self.tail != SENTINEL {
            slab.get_mut(self.tail).next = index;
        } else {
            self.head = index;
        }
        self.tail = index;
    }

    fn unlink<K, V>(&mut self, slab: &mut Slab<K, V>, index: u32) {
        let entry = slab.get(index);
        let prev = entry.prev();
        let next = entry.next;

        if prev != SENTINEL {
            slab.get_mut(prev).next = next;
        } else {
            self.head = next;
        }

        if next != SENTINEL {
            slab.get_mut(next).set_prev(prev);
        } else {
            self.tail = prev;
        }

        let entry = slab.get_mut(index);
        entry.set_prev(SENTINEL);
        entry.next = SENTINEL;
    }

    fn advance_hand_past<K, V>(&mut self, slab: &Slab<K, V>, index: u32) {
        if self.hand == index {
            self.hand = slab.get(index).next;
        }
    }

    fn sieve_evict<K, V>(&mut self, slab: &mut Slab<K, V>) -> Option<u32> {
        self.sieve_evict_with_scan_limit(slab, u32::MAX)
    }

    fn sieve_evict_with_scan_limit<K, V>(
        &mut self,
        slab: &mut Slab<K, V>,
        scan_limit: u32,
    ) -> Option<u32> {
        if self.head == SENTINEL {
            return None;
        }

        let mut current = if self.hand != SENTINEL {
            self.hand
        } else {
            self.head
        };

        for _ in 0..scan_limit {
            if current == SENTINEL {
                current = self.head;
                if current == SENTINEL {
                    return None;
                }
            }

            let entry = slab.get_mut(current);
            if entry.is_visited() {
                entry.clear_visited();
                let next = entry.next;
                current = if next != SENTINEL { next } else { self.head };
            } else {
                self.hand = slab.get(current).next;
                let victim = current;
                self.unlink(slab, victim);
                return Some(victim);
            }
        }

        self.hand = current;
        None
    }

    fn clear(&mut self) {
        self.head = SENTINEL;
        self.tail = SENTINEL;
        self.hand = SENTINEL;
    }
}

#[cfg(test)]
mod tests {
    use super::SlabEntry;
    use std::mem::size_of;

    #[test]
    fn packed_slab_entry_uses_value_niche() {
        assert_eq!(size_of::<SlabEntry<u64, u64>>(), 32);
        assert_eq!(size_of::<Option<SlabEntry<u64, u64>>>(), 32);
    }
}
