//! Provides a *not* thread-safe cache implementation built upon
//! [`hashbrown::HashTable`][hb-hashtable].
//!
//! [hb-hashtable]: https://docs.rs/hashbrown/latest/hashbrown/struct.HashTable.html

mod builder;
mod cache;
mod iter;

pub use builder::CacheBuilder;
pub use cache::Cache;
pub use iter::Iter;

const SENTINEL: u32 = u32::MAX;

pub(crate) struct SlabEntry<K, V> {
    pub(crate) key: K,
    pub(crate) value: V,
    pub(crate) hash: u64,
    pub(crate) visited: bool,
    prev: u32,
    next: u32,
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
        entry.prev = self.tail;
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
        let prev = entry.prev;
        let next = entry.next;

        if prev != SENTINEL {
            slab.get_mut(prev).next = next;
        } else {
            self.head = next;
        }

        if next != SENTINEL {
            slab.get_mut(next).prev = prev;
        } else {
            self.tail = prev;
        }

        let entry = slab.get_mut(index);
        entry.prev = SENTINEL;
        entry.next = SENTINEL;
    }

    fn advance_hand_past<K, V>(&mut self, slab: &Slab<K, V>, index: u32) {
        if self.hand == index {
            let prev = slab.get(index).prev;
            self.hand = prev;
        }
    }

    fn sieve_evict<K, V>(&mut self, slab: &mut Slab<K, V>) -> Option<u32> {
        if self.head == SENTINEL {
            return None;
        }

        let mut current = if self.hand != SENTINEL {
            self.hand
        } else {
            self.tail
        };

        loop {
            if current == SENTINEL {
                current = self.tail;
                if current == SENTINEL {
                    return None;
                }
            }

            let entry = slab.get_mut(current);
            if entry.visited {
                entry.visited = false;
                let prev = entry.prev;
                current = if prev != SENTINEL { prev } else { self.tail };
            } else {
                let prev = slab.get(current).prev;
                self.hand = prev;
                let victim = current;
                self.unlink(slab, victim);
                return Some(victim);
            }
        }
    }

    fn clear(&mut self) {
        self.head = SENTINEL;
        self.tail = SENTINEL;
        self.hand = SENTINEL;
    }
}
