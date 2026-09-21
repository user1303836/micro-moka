use super::Cache;
use crate::unsync::SENTINEL;
use std::hash::{BuildHasherDefault, Hasher};

#[derive(Default)]
struct CollisionHasher;
impl Hasher for CollisionHasher {
    fn finish(&self) -> u64 {
        0
    }
    fn write(&mut self, _: &[u8]) {}
}
type TestCache = Cache<u64, u64, BuildHasherDefault<CollisionHasher>>;

struct Entry {
    key: u64,
    value: u64,
    visited: bool,
}
struct Model {
    entries: Vec<Entry>,
    hand: Option<u64>,
    capacity: usize,
    budget: usize,
}
impl Model {
    fn pos(&self, key: u64) -> Option<usize> {
        self.entries.iter().position(|e| e.key == key)
    }
    fn peek(&self, key: u64) -> Option<u64> {
        self.pos(key).map(|i| self.entries[i].value)
    }
    fn get(&mut self, key: u64) -> Option<u64> {
        self.pos(key).map(|i| {
            self.entries[i].visited = true;
            self.entries[i].value
        })
    }
    fn remove(&mut self, key: u64) -> Option<u64> {
        let i = self.pos(key)?;
        if self.hand == Some(key) {
            self.hand = self.entries.get(i + 1).map(|e| e.key);
        }
        Some(self.entries.remove(i).value)
    }
    fn evict(&mut self, budget: usize) -> bool {
        if self.entries.is_empty() {
            return false;
        }
        let mut i = self.hand.and_then(|key| self.pos(key)).unwrap_or(0);
        for _ in 0..budget {
            if self.entries[i].visited {
                self.entries[i].visited = false;
                i = (i + 1) % self.entries.len();
            } else {
                self.hand = self.entries.get(i + 1).map(|e| e.key);
                self.entries.remove(i);
                return true;
            }
        }
        self.hand = Some(self.entries[i].key);
        false
    }
    fn insert(&mut self, key: u64, value: u64, bounded: bool) -> bool {
        if let Some(i) = self.pos(key) {
            self.entries[i].value = value;
            self.entries[i].visited = true;
            return true;
        }
        if self.entries.len() >= self.capacity
            && (self.capacity == 0 || !self.evict(if bounded { self.budget } else { usize::MAX }))
        {
            return false;
        }
        self.entries.push(Entry {
            key,
            value,
            visited: false,
        });
        true
    }
    fn load(&mut self, key: u64, value: u64) -> u64 {
        if let Some(value) = self.get(key) {
            return value;
        }
        if self.entries.len() >= self.capacity {
            self.evict(usize::MAX);
        }
        self.entries.push(Entry {
            key,
            value,
            visited: false,
        });
        value
    }
    fn clear(&mut self) {
        self.entries.clear();
        self.hand = None;
    }
}

fn assert_state(cache: &TestCache, model: &Model) {
    assert_eq!(cache.entry_count as usize, model.entries.len());
    assert_eq!(cache.table.len(), model.entries.len());
    let occupied: Vec<_> = cache.slab.iter().map(|(i, _)| i).collect();
    let mut indexed: Vec<_> = cache.table.iter().copied().collect();
    indexed.sort_unstable();
    assert_eq!(indexed, occupied);
    let vacant: Vec<_> = cache
        .slab
        .entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| e.is_none().then_some(i as u32))
        .collect();
    let mut free = cache.slab.free_list.clone();
    free.sort_unstable();
    assert_eq!(free, vacant);

    let mut current = cache.deque.head;
    let mut previous = SENTINEL;
    for expected in &model.entries {
        assert_ne!(current, SENTINEL);
        let entry = cache.slab.get(current);
        assert_eq!(
            (entry.key, entry.value, entry.is_visited()),
            (expected.key, expected.value, expected.visited)
        );
        assert_eq!(entry.prev(), previous);
        assert_eq!(entry.hash, 0);
        previous = current;
        current = entry.next;
    }
    assert_eq!(current, SENTINEL);
    assert_eq!(cache.deque.tail, previous);
    let hand = (cache.deque.hand != SENTINEL).then(|| cache.slab.get(cache.deque.hand).key);
    assert_eq!(hand, model.hand);

    let mut expected: Vec<_> = model.entries.iter().map(|e| (e.key, e.value)).collect();
    expected.sort_unstable();
    let mut actual: Vec<_> = cache.iter().map(|(&k, &v)| (k, v)).collect();
    actual.sort_unstable();
    assert_eq!(actual, expected);
    let mut folded = cache.iter().fold(Vec::new(), |mut acc, (&k, &v)| {
        acc.push((k, v));
        acc
    });
    folded.sort_unstable();
    assert_eq!(folded, expected);
    let mut iter = cache.iter();
    for remaining in (1..=expected.len()).rev() {
        assert_eq!(iter.len(), remaining);
        assert_eq!(iter.size_hint(), (remaining, Some(remaining)));
        assert!(iter.next().is_some());
    }
    assert_eq!(iter.next(), None);
    assert_eq!(iter.next(), None);
    assert_eq!(iter.count(), 0);
}

fn random(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

#[test]
fn differential_sieve_with_collisions_and_invariants() {
    for capacity in [0, 1, 2, 3, 8, 31, 128] {
        for budget in [0, 1, 2, 16, 32] {
            for seed in [0, 42, 98765] {
                let mut cache: TestCache = Cache::builder()
                    .max_capacity(capacity as u64)
                    .admission_scan_limit(budget as u32)
                    .build_with_hasher(BuildHasherDefault::default());
                let mut model = Model {
                    entries: vec![],
                    hand: None,
                    capacity,
                    budget,
                };
                let mut rng = seed;
                for step in 0..if cfg!(miri) { 16 } else { 10000 } {
                    let r = random(&mut rng);
                    let key = (r >> 8) % 257;
                    let value = r >> 32;
                    match r % 16 {
                        0..=4 => assert_eq!(cache.get(&key).copied(), model.get(key)),
                        5 | 15 => {
                            cache.insert(key, value);
                            model.insert(key, value, false);
                        }
                        6 => assert_eq!(
                            cache.try_insert(key, value),
                            if model.insert(key, value, true) {
                                Ok(())
                            } else {
                                Err((key, value))
                            }
                        ),
                        7 => assert_eq!(cache.remove(&key), model.remove(key)),
                        8 => assert_eq!(cache.peek(&key).copied(), model.peek(key)),
                        9 => assert_eq!(
                            *cache.get_or_insert_with_ref(&key, || value),
                            model.load(key, value)
                        ),
                        10 => assert_eq!(cache.contains_key(&key), model.peek(key).is_some()),
                        11 => assert_eq!(
                            *cache.get_or_insert_with(key, || value),
                            model.load(key, value)
                        ),
                        12 => {
                            cache.invalidate_entries_if(|k, _| k % 13 == key % 13);
                            let keys: Vec<_> = model
                                .entries
                                .iter()
                                .filter(|e| e.key % 13 == key % 13)
                                .map(|e| e.key)
                                .collect();
                            for k in keys {
                                model.remove(k);
                            }
                        }
                        13 => {
                            cache.invalidate(&key);
                            model.remove(key);
                        }
                        _ if r % 127 == 0 => {
                            cache.invalidate_all();
                            model.clear();
                        }
                        _ => {}
                    }
                    assert_eq!(
                        cache.entry_count() as usize,
                        model.entries.len(),
                        "capacity={capacity} budget={budget} seed={seed} step={step}"
                    );
                    if step % 19 == 0 {
                        assert_state(&cache, &model);
                    }
                }
                assert_state(&cache, &model);
            }
        }
    }
}

#[test]
fn iteration_across_sparse_dense_clear_and_refill_transitions() {
    let mut cache: TestCache = Cache::builder()
        .max_capacity(128)
        .build_with_hasher(BuildHasherDefault::default());
    let mut model = Model {
        entries: vec![],
        hand: None,
        capacity: 128,
        budget: 16,
    };
    for round in 0..3 {
        for key in 0..128 {
            cache.insert(key, round);
            model.insert(key, round, false);
        }
        assert_state(&cache, &model);
        for key in 0..127 {
            cache.remove(&key);
            model.remove(key);
        }
        assert_state(&cache, &model);
        for key in 0..128 {
            cache.insert(key, round);
            model.insert(key, round, false);
        }
        cache.invalidate_all();
        model.clear();
        assert_state(&cache, &model);
    }
}
