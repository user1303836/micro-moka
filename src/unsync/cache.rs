use super::{CacheBuilder, IndexDeque, Iter, Slab, SlabEntry, SENTINEL};
use crate::{
    common::{self, frequency_sketch::FrequencySketch},
    Policy,
};

use hashbrown::HashTable;
use std::{
    borrow::Borrow,
    collections::hash_map::RandomState,
    fmt,
    hash::{BuildHasher, Hash},
};

const EVICTION_BATCH_SIZE: usize = 100;

/// An in-memory cache that is _not_ thread-safe.
///
/// `Cache` utilizes a hash table [`hashbrown::HashTable`][hb-hashtable] for the
/// central key-value storage. `Cache` performs a best-effort bounding of the
/// map using an entry replacement algorithm to determine which entries to evict
/// when the capacity is exceeded.
///
/// [hb-hashtable]: https://docs.rs/hashbrown/latest/hashbrown/struct.HashTable.html
///
/// # Characteristic difference between `unsync` and `sync`/`future` caches
///
/// If you use a cache from a single thread application, `unsync::Cache` may
/// outperform other caches for updates and retrievals because other caches have some
/// overhead on syncing internal data structures between threads.
///
/// # Examples
///
/// Cache entries are manually added using the insert method, and are stored in the
/// cache until either evicted or manually invalidated.
///
/// Here's an example of reading and updating a cache by using the main thread:
///
///```rust
/// use micro_moka::unsync::Cache;
///
/// const NUM_KEYS: usize = 64;
///
/// fn value(n: usize) -> String {
///     format!("value {}", n)
/// }
///
/// // Create a cache that can store up to 10,000 entries.
/// let mut cache = Cache::new(10_000);
///
/// // Insert 64 entries.
/// for key in 0..NUM_KEYS {
///     cache.insert(key, value(key));
/// }
///
/// // Invalidate every 4 element of the inserted entries.
/// for key in (0..NUM_KEYS).step_by(4) {
///     cache.invalidate(&key);
/// }
///
/// // Verify the result.
/// for key in 0..NUM_KEYS {
///     if key % 4 == 0 {
///         assert_eq!(cache.get(&key), None);
///     } else {
///         assert_eq!(cache.get(&key), Some(&value(key)));
///     }
/// }
/// ```
///
/// # Hashing Algorithm
///
/// By default, `Cache` uses a hashing algorithm selected to provide resistance
/// against HashDoS attacks. It will the same one used by
/// `std::collections::HashMap`, which is currently SipHash 1-3.
///
/// While SipHash's performance is very competitive for medium sized keys, other
/// hashing algorithms will outperform it for small keys such as integers as well as
/// large keys such as long strings. However those algorithms will typically not
/// protect against attacks such as HashDoS.
///
/// The hashing algorithm can be replaced on a per-`Cache` basis using the
/// [`build_with_hasher`][build-with-hasher-method] method of the
/// `CacheBuilder`. Many alternative algorithms are available on crates.io, such
/// as the [aHash][ahash-crate] crate.
///
/// [build-with-hasher-method]: ./struct.CacheBuilder.html#method.build_with_hasher
/// [ahash-crate]: https://crates.io/crates/ahash
///
pub struct Cache<K, V, S = RandomState> {
    max_capacity: Option<u64>,
    entry_count: u64,
    table: HashTable<u32>,
    build_hasher: S,
    slab: Slab<K, V>,
    deque: IndexDeque,
    frequency_sketch: FrequencySketch,
    frequency_sketch_enabled: bool,
    read_buffer: [u64; 64],
    read_buffer_len: u8,
}

impl<K, V, S> fmt::Debug for Cache<K, V, S>
where
    K: fmt::Debug + Eq + Hash,
    V: fmt::Debug,
    S: BuildHasher + Clone,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d_map = f.debug_map();

        for (k, v) in self.iter() {
            d_map.entry(&k, &v);
        }

        d_map.finish()
    }
}

impl<K, V> Cache<K, V, RandomState>
where
    K: Hash + Eq,
{
    /// Constructs a new `Cache<K, V>` that will store up to the `max_capacity` entries.
    ///
    /// To adjust various configuration knobs such as `initial_capacity`, use the
    /// [`CacheBuilder`][builder-struct].
    ///
    /// [builder-struct]: ./struct.CacheBuilder.html
    pub fn new(max_capacity: u64) -> Self {
        let build_hasher = RandomState::default();
        Self::with_everything(Some(max_capacity), None, build_hasher)
    }

    /// Returns a [`CacheBuilder`][builder-struct], which can builds a `Cache` with
    /// various configuration knobs.
    ///
    /// [builder-struct]: ./struct.CacheBuilder.html
    pub fn builder() -> CacheBuilder<K, V, Cache<K, V, RandomState>> {
        CacheBuilder::default()
    }
}

//
// public
//
impl<K, V, S> Cache<K, V, S> {
    /// Returns a read-only cache policy of this cache.
    ///
    /// At this time, cache policy cannot be modified after cache creation.
    /// A future version may support to modify it.
    pub fn policy(&self) -> Policy {
        Policy::new(self.max_capacity)
    }

    /// Returns the number of entries in this cache.
    ///
    /// # Example
    ///
    /// ```rust
    /// use micro_moka::unsync::Cache;
    ///
    /// let mut cache = Cache::new(10);
    /// cache.insert('n', "Netherland Dwarf");
    /// cache.insert('l', "Lop Eared");
    /// cache.insert('d', "Dutch");
    ///
    /// // Ensure an entry exists.
    /// assert!(cache.contains_key(&'n'));
    ///
    /// // Followings will print the actual numbers.
    /// println!("{}", cache.entry_count());   // -> 3
    /// ```
    ///
    pub fn entry_count(&self) -> u64 {
        self.entry_count
    }

    /// Returns the total weighted size of entries in this cache.
    ///
    /// This is equivalent to `entry_count` as weight support has been removed.
    pub fn weighted_size(&self) -> u64 {
        self.entry_count
    }
}

impl<K, V, S> Cache<K, V, S>
where
    K: Hash + Eq,
    S: BuildHasher + Clone,
{
    pub(crate) fn with_everything(
        max_capacity: Option<u64>,
        initial_capacity: Option<usize>,
        build_hasher: S,
    ) -> Self {
        let init_cap = initial_capacity.unwrap_or_default();

        Self {
            max_capacity,
            entry_count: 0,
            table: HashTable::with_capacity(init_cap),
            build_hasher,
            slab: if init_cap > 0 {
                Slab::with_capacity(init_cap)
            } else {
                Slab::new()
            },
            deque: IndexDeque::default(),
            frequency_sketch: FrequencySketch::default(),
            frequency_sketch_enabled: false,
            read_buffer: [0; 64],
            read_buffer_len: 0,
        }
    }

    /// Returns `true` if the cache contains a value for the key.
    ///
    /// Unlike the `get` method, this method is not considered a cache read operation,
    /// so it does not update the historic popularity estimator.
    ///
    /// The key may be any borrowed form of the cache's key type, but `Hash` and `Eq`
    /// on the borrowed form _must_ match those for the key type.
    #[inline]
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let hash = self.hash(key);
        self.table
            .find(hash, |&idx| self.slab.get(idx).key.borrow() == key)
            .is_some()
    }

    /// Returns an immutable reference of the value corresponding to the key.
    ///
    /// The key may be any borrowed form of the cache's key type, but `Hash` and `Eq`
    /// on the borrowed form _must_ match those for the key type.
    #[inline]
    pub fn get<Q>(&mut self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let hash = self.hash(key);

        let idx = match self
            .table
            .find(hash, |&idx| self.slab.get(idx).key.borrow() == key)
        {
            Some(&idx) => idx,
            None => return None,
        };

        if self.read_buffer_len == 64 {
            self.drain_read_buffer();
        }
        self.read_buffer[self.read_buffer_len as usize] = hash;
        self.read_buffer_len += 1;

        self.deque.move_to_back(&mut self.slab, idx);
        Some(&self.slab.get(idx).value)
    }

    /// Returns an immutable reference of the value corresponding to the key,
    /// without updating the frequency sketch or deque position.
    ///
    /// Unlike [`get`](#method.get), this method does not count as a cache read
    /// for eviction purposes: the entry's popularity estimate is unchanged and
    /// its position in the LRU queue is not promoted. This is useful when you
    /// want to inspect the cache without influencing which entries get evicted,
    /// or when you only have a shared (`&self`) reference.
    ///
    /// The key may be any borrowed form of the cache's key type, but `Hash` and `Eq`
    /// on the borrowed form _must_ match those for the key type.
    ///
    /// # Example
    ///
    /// ```rust
    /// use micro_moka::unsync::Cache;
    ///
    /// let mut cache = Cache::new(100);
    /// cache.insert("a", "alice");
    ///
    /// // peek() returns the value without affecting eviction order.
    /// assert_eq!(cache.peek(&"a"), Some(&"alice"));
    /// assert_eq!(cache.peek(&"missing"), None);
    /// ```
    #[inline]
    pub fn peek<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let hash = self.hash(key);
        let idx = self
            .table
            .find(hash, |&idx| self.slab.get(idx).key.borrow() == key)?;
        Some(&self.slab.get(*idx).value)
    }

    /// Inserts a key-value pair into the cache.
    ///
    /// If the cache has this key present, the value is updated.
    #[inline]
    pub fn insert(&mut self, key: K, value: V) {
        let weights_to_evict = self.weights_to_evict();
        if weights_to_evict > 0 {
            self.evict_lru_entries(weights_to_evict);
        }

        let hash = self.hash(&key);

        if let Some(&idx) = self
            .table
            .find(hash, |&idx| self.slab.get(idx).key.borrow() == &key)
        {
            self.slab.get_mut(idx).value = value;
            self.deque.move_to_back(&mut self.slab, idx);
            return;
        }

        let slab_entry = SlabEntry {
            key,
            value,
            hash,
            prev: SENTINEL,
            next: SENTINEL,
        };
        let idx = self.slab.allocate(slab_entry);

        let slab = &self.slab;
        self.table
            .insert_unique(hash, idx, |&existing_idx| slab.get(existing_idx).hash);

        self.handle_insert(idx, hash);
    }

    /// Discards any cached value for the key.
    ///
    /// The key may be any borrowed form of the cache's key type, but `Hash` and `Eq`
    /// on the borrowed form _must_ match those for the key type.
    #[inline]
    pub fn invalidate<Q>(&mut self, key: &Q)
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let hash = self.hash(key);
        let slab = &self.slab;
        if let Ok(entry) = self
            .table
            .find_entry(hash, |&idx| slab.get(idx).key.borrow() == key)
        {
            let (idx, _) = entry.remove();
            self.deque.unlink(&mut self.slab, idx);
            self.slab.deallocate(idx);
            self.entry_count -= 1;
        }
    }

    /// Discards any cached value for the key, returning the cached value.
    ///
    /// The key may be any borrowed form of the cache's key type, but `Hash` and `Eq`
    /// on the borrowed form _must_ match those for the key type.
    #[inline]
    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let hash = self.hash(key);
        let slab = &self.slab;
        if let Ok(entry) = self
            .table
            .find_entry(hash, |&idx| slab.get(idx).key.borrow() == key)
        {
            let (idx, _) = entry.remove();
            self.deque.unlink(&mut self.slab, idx);
            let slab_entry = self.slab.deallocate(idx);
            self.entry_count -= 1;
            Some(slab_entry.value)
        } else {
            None
        }
    }

    /// Discards all cached values.
    ///
    /// Like the `invalidate` method, this method does not clear the historic
    /// popularity estimator of keys so that it retains the client activities of
    /// trying to retrieve an item.
    #[cold]
    #[inline(never)]
    pub fn invalidate_all(&mut self) {
        let old_capacity = self.table.capacity();
        let old_table = std::mem::replace(&mut self.table, HashTable::new());
        let old_slab = std::mem::replace(&mut self.slab, Slab::new());
        self.deque.clear();
        self.entry_count = 0;
        self.read_buffer_len = 0;

        drop(old_table);
        drop(old_slab);

        self.table.reserve(old_capacity, |&idx| {
            // This closure is for rehashing during reserve. Since the table is
            // empty after the swap, this will never be called, but we must
            // provide it.
            let _ = idx;
            0
        });
    }

    /// Discards cached values that satisfy a predicate.
    ///
    /// `invalidate_entries_if` takes a closure that returns `true` or `false`.
    /// `invalidate_entries_if` will apply the closure to each cached value,
    /// and if the closure returns `true`, the value will be invalidated.
    ///
    /// Like the `invalidate` method, this method does not clear the historic
    /// popularity estimator of keys so that it retains the client activities of
    /// trying to retrieve an item.
    #[cold]
    #[inline(never)]
    pub fn invalidate_entries_if(&mut self, mut predicate: impl FnMut(&K, &V) -> bool) {
        let indices_to_invalidate: Vec<u32> = self
            .slab
            .iter()
            .filter(|(_, entry)| predicate(&entry.key, &entry.value))
            .map(|(idx, _)| idx)
            .collect();

        let mut invalidated = 0u64;
        for idx in indices_to_invalidate {
            let hash = self.slab.get(idx).hash;
            if let Ok(entry) = self.table.find_entry(hash, |&table_idx| table_idx == idx) {
                entry.remove();
                self.deque.unlink(&mut self.slab, idx);
                self.slab.deallocate(idx);
                invalidated += 1;
            }
        }
        self.entry_count -= invalidated;
    }

    /// Creates an iterator visiting all key-value pairs in arbitrary order. The
    /// iterator element type is `(&K, &V)`.
    ///
    /// Unlike the `get` method, visiting entries via an iterator do not update the
    /// historic popularity estimator or reset idle timers for keys.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use micro_moka::unsync::Cache;
    ///
    /// let mut cache = Cache::new(100);
    /// cache.insert("Julia", 14);
    ///
    /// let mut iter = cache.iter();
    /// let (k, v) = iter.next().unwrap(); // (&K, &V)
    /// assert_eq!(k, &"Julia");
    /// assert_eq!(v, &14);
    ///
    /// assert!(iter.next().is_none());
    /// ```
    ///
    pub fn iter(&self) -> Iter<'_, K, V> {
        Iter::new(&self.slab.entries)
    }
}

//
// private
//
impl<K, V, S> Cache<K, V, S>
where
    K: Hash + Eq,
    S: BuildHasher + Clone,
{
    #[inline]
    fn hash<Q>(&self, key: &Q) -> u64
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.build_hasher.hash_one(key)
    }

    #[inline]
    fn has_enough_capacity(&self, candidate_weight: u32, ws: u64) -> bool {
        self.max_capacity
            .map(|limit| ws + candidate_weight as u64 <= limit)
            .unwrap_or(true)
    }

    #[inline]
    fn weights_to_evict(&self) -> u64 {
        self.max_capacity
            .map(|limit| self.entry_count.saturating_sub(limit))
            .unwrap_or_default()
    }

    #[inline]
    fn should_enable_frequency_sketch(&self) -> bool {
        if self.frequency_sketch_enabled {
            false
        } else if let Some(max_cap) = self.max_capacity {
            self.entry_count >= max_cap / 2
        } else {
            false
        }
    }

    #[inline]
    fn enable_frequency_sketch(&mut self) {
        if let Some(max_cap) = self.max_capacity {
            self.do_enable_frequency_sketch(max_cap);
        }
    }

    #[cfg(test)]
    fn enable_frequency_sketch_for_testing(&mut self) {
        if let Some(max_cap) = self.max_capacity {
            self.do_enable_frequency_sketch(max_cap);
        }
    }

    #[inline]
    fn do_enable_frequency_sketch(&mut self, cache_capacity: u64) {
        let skt_capacity = common::sketch_capacity(cache_capacity);
        self.frequency_sketch.ensure_capacity(skt_capacity);
        self.frequency_sketch_enabled = true;
    }

    #[inline]
    fn drain_read_buffer(&mut self) {
        for i in 0..self.read_buffer_len as usize {
            self.frequency_sketch.increment(self.read_buffer[i]);
        }
        self.read_buffer_len = 0;
    }

    #[inline]
    fn handle_insert(&mut self, idx: u32, hash: u64) {
        self.drain_read_buffer();
        let has_free_space = self.has_enough_capacity(1, self.entry_count);

        if has_free_space {
            self.deque.push_back(&mut self.slab, idx);
            self.entry_count += 1;

            if self.should_enable_frequency_sketch() {
                self.enable_frequency_sketch();
            }
            return;
        }

        if let Some(max) = self.max_capacity {
            if max == 0 {
                self.remove_by_index(idx);
                return;
            }
        }

        let candidate_freq = self.frequency_sketch.frequency(hash);

        match self.admit(candidate_freq) {
            AdmissionResult::Admitted { victim_index } => {
                self.remove_by_index(victim_index);

                self.deque.push_back(&mut self.slab, idx);
                self.entry_count += 1;

                if self.should_enable_frequency_sketch() {
                    self.enable_frequency_sketch();
                }
            }
            AdmissionResult::Rejected => {
                self.remove_by_index(idx);
            }
        }
    }

    #[inline]
    fn admit(&self, candidate_freq: u8) -> AdmissionResult {
        let Some(victim_index) = self.deque.peek_front() else {
            return AdmissionResult::Rejected;
        };
        let victim_hash = self.slab.get(victim_index).hash;
        let victim_freq = self.frequency_sketch.frequency(victim_hash);

        if candidate_freq > victim_freq {
            AdmissionResult::Admitted { victim_index }
        } else {
            AdmissionResult::Rejected
        }
    }

    fn remove_by_index(&mut self, idx: u32) {
        let hash = self.slab.get(idx).hash;
        if let Ok(entry) = self.table.find_entry(hash, |&table_idx| table_idx == idx) {
            entry.remove();
        }
        let entry = self.slab.get(idx);
        if entry.prev != SENTINEL || entry.next != SENTINEL || self.deque.head == idx {
            self.deque.unlink(&mut self.slab, idx);
            self.entry_count -= 1;
        }
        self.slab.deallocate(idx);
    }

    #[cold]
    #[inline(never)]
    fn evict_lru_entries(&mut self, weights_to_evict: u64) {
        debug_assert!(weights_to_evict > 0);
        let mut evicted = 0u64;

        for _ in 0..EVICTION_BATCH_SIZE {
            if evicted >= weights_to_evict {
                break;
            }

            let Some(victim_idx) = self.deque.peek_front() else {
                break;
            };

            let victim_hash = self.slab.get(victim_idx).hash;
            if let Ok(entry) = self
                .table
                .find_entry(victim_hash, |&table_idx| table_idx == victim_idx)
            {
                entry.remove();
            }

            self.deque.unlink(&mut self.slab, victim_idx);
            self.slab.deallocate(victim_idx);
            evicted += 1;
        }

        self.entry_count -= evicted;
    }
}

#[cfg(test)]
impl<K, V, S> Cache<K, V, S>
where
    K: Hash + Eq,
    S: BuildHasher + Clone,
{
    fn read_buffer_len(&self) -> u8 {
        self.read_buffer_len
    }
}

enum AdmissionResult {
    Admitted { victim_index: u32 },
    Rejected,
}

#[cfg(test)]
mod tests {
    use super::Cache;

    #[test]
    fn basic_single_thread() {
        let mut cache = Cache::new(3);
        cache.enable_frequency_sketch_for_testing();

        cache.insert("a", "alice");
        cache.insert("b", "bob");
        assert_eq!(cache.get(&"a"), Some(&"alice"));
        assert!(cache.contains_key(&"a"));
        assert!(cache.contains_key(&"b"));
        assert_eq!(cache.get(&"b"), Some(&"bob"));
        // buffered: [hash_a, hash_b] (not yet flushed)

        cache.insert("c", "cindy");
        // drain flushes buffer -> counts: a -> 1, b -> 1
        assert_eq!(cache.get(&"c"), Some(&"cindy"));
        assert!(cache.contains_key(&"c"));
        // buffered: [hash_c]

        assert!(cache.contains_key(&"a"));
        assert_eq!(cache.get(&"a"), Some(&"alice"));
        assert_eq!(cache.get(&"b"), Some(&"bob"));
        assert!(cache.contains_key(&"b"));
        // buffered: [hash_c, hash_a, hash_b]

        // "d" should not be admitted because its frequency is too low.
        // insert drains buffer -> counts: a -> 2, b -> 2, c -> 1
        cache.insert("d", "david");
        assert_eq!(cache.get(&"d"), None);
        assert!(!cache.contains_key(&"d"));

        cache.insert("d", "david");
        assert!(!cache.contains_key(&"d"));
        assert_eq!(cache.get(&"d"), None);

        // Build up d's frequency directly (misses do not buffer).
        let hash_d = cache.hash(&"d");
        cache.frequency_sketch.increment(hash_d);
        cache.frequency_sketch.increment(hash_d);

        // "d" should be admitted and "c" should be evicted
        // because d's frequency is higher than c's.
        cache.insert("d", "dennis");
        assert_eq!(cache.get(&"a"), Some(&"alice"));
        assert_eq!(cache.get(&"b"), Some(&"bob"));
        assert_eq!(cache.get(&"c"), None);
        assert_eq!(cache.get(&"d"), Some(&"dennis"));
        assert!(cache.contains_key(&"a"));
        assert!(cache.contains_key(&"b"));
        assert!(!cache.contains_key(&"c"));
        assert!(cache.contains_key(&"d"));

        cache.invalidate(&"b");
        assert_eq!(cache.get(&"b"), None);
        assert!(!cache.contains_key(&"b"));
    }

    #[test]
    fn invalidate_all() {
        let mut cache = Cache::new(100);
        cache.enable_frequency_sketch_for_testing();

        cache.insert("a", "alice");
        cache.insert("b", "bob");
        cache.insert("c", "cindy");
        assert_eq!(cache.get(&"a"), Some(&"alice"));
        assert_eq!(cache.get(&"b"), Some(&"bob"));
        assert_eq!(cache.get(&"c"), Some(&"cindy"));
        assert!(cache.contains_key(&"a"));
        assert!(cache.contains_key(&"b"));
        assert!(cache.contains_key(&"c"));

        cache.invalidate_all();

        cache.insert("d", "david");

        assert!(cache.get(&"a").is_none());
        assert!(cache.get(&"b").is_none());
        assert!(cache.get(&"c").is_none());
        assert_eq!(cache.get(&"d"), Some(&"david"));
        assert!(!cache.contains_key(&"a"));
        assert!(!cache.contains_key(&"b"));
        assert!(!cache.contains_key(&"c"));
        assert!(cache.contains_key(&"d"));
    }

    #[test]
    fn invalidate_entries_if() {
        use std::collections::HashSet;

        let mut cache = Cache::new(100);
        cache.enable_frequency_sketch_for_testing();

        cache.insert(0, "alice");
        cache.insert(1, "bob");
        cache.insert(2, "alex");

        assert_eq!(cache.get(&0), Some(&"alice"));
        assert_eq!(cache.get(&1), Some(&"bob"));
        assert_eq!(cache.get(&2), Some(&"alex"));
        assert!(cache.contains_key(&0));
        assert!(cache.contains_key(&1));
        assert!(cache.contains_key(&2));

        let names = ["alice", "alex"].iter().cloned().collect::<HashSet<_>>();
        cache.invalidate_entries_if(move |_k, &v| names.contains(v));

        cache.insert(3, "alice");

        assert!(cache.get(&0).is_none());
        assert!(cache.get(&2).is_none());
        assert_eq!(cache.get(&1), Some(&"bob"));
        // This should survive as it was inserted after calling invalidate_entries_if.
        assert_eq!(cache.get(&3), Some(&"alice"));

        assert!(!cache.contains_key(&0));
        assert!(cache.contains_key(&1));
        assert!(!cache.contains_key(&2));
        assert!(cache.contains_key(&3));

        assert_eq!(cache.table.len(), 2);

        cache.invalidate_entries_if(|_k, &v| v == "alice");
        cache.invalidate_entries_if(|_k, &v| v == "bob");

        assert!(cache.get(&1).is_none());
        assert!(cache.get(&3).is_none());

        assert!(!cache.contains_key(&1));
        assert!(!cache.contains_key(&3));

        assert_eq!(cache.table.len(), 0);
    }

    #[cfg_attr(target_pointer_width = "16", ignore)]
    #[test]
    fn test_skt_capacity_will_not_overflow() {
        let pot = |exp| 2u64.pow(exp);

        let ensure_sketch_len = |max_capacity, len, name| {
            let mut cache = Cache::<u8, u8>::new(max_capacity);
            cache.enable_frequency_sketch_for_testing();
            assert_eq!(cache.frequency_sketch.table_len(), len as usize, "{}", name);
        };

        if cfg!(target_pointer_width = "32") {
            let pot24 = pot(24);
            let pot16 = pot(16);
            ensure_sketch_len(0, 128, "0");
            ensure_sketch_len(128, 128, "128");
            ensure_sketch_len(pot16, pot16, "pot16");
            ensure_sketch_len(pot16 + 1, pot(17), "pot16 + 1");
            ensure_sketch_len(pot24 - 1, pot24, "pot24 - 1");
            ensure_sketch_len(pot24, pot24, "pot24");
            ensure_sketch_len(pot(27), pot24, "pot(27)");
            ensure_sketch_len(u32::MAX as u64, pot24, "u32::MAX");
        } else {
            let pot30 = pot(30);
            let pot16 = pot(16);
            ensure_sketch_len(0, 128, "0");
            ensure_sketch_len(128, 128, "128");
            ensure_sketch_len(pot16, pot16, "pot16");
            ensure_sketch_len(pot16 + 1, pot(17), "pot16 + 1");

            if !cfg!(circleci) {
                ensure_sketch_len(pot30 - 1, pot30, "pot30- 1");
                ensure_sketch_len(pot30, pot30, "pot30");
                ensure_sketch_len(u64::MAX, pot30, "u64::MAX");
            }
        };
    }

    #[test]
    fn remove_decrements_entry_count() {
        let mut cache = Cache::new(3);
        cache.insert("a", "alice");
        cache.insert("b", "bob");
        assert_eq!(cache.entry_count(), 2);

        let removed = cache.remove(&"a");
        assert_eq!(removed, Some("alice"));
        assert_eq!(cache.entry_count(), 1);

        cache.remove(&"nonexistent");
        assert_eq!(cache.entry_count(), 1);

        cache.remove(&"b");
        assert_eq!(cache.entry_count(), 0);
    }

    #[test]
    fn invalidate_decrements_entry_count() {
        let mut cache = Cache::new(3);
        cache.insert("a", "alice");
        cache.insert("b", "bob");
        assert_eq!(cache.entry_count(), 2);

        cache.invalidate(&"a");
        assert_eq!(cache.entry_count(), 1);

        cache.invalidate(&"nonexistent");
        assert_eq!(cache.entry_count(), 1);

        cache.invalidate(&"b");
        assert_eq!(cache.entry_count(), 0);
    }

    #[test]
    fn insert_after_remove_on_full_cache() {
        let mut cache = Cache::new(2);
        cache.insert("a", "alice");
        cache.insert("b", "bob");
        assert_eq!(cache.entry_count(), 2);

        cache.remove(&"a");
        assert_eq!(cache.entry_count(), 1);

        cache.insert("c", "cindy");
        assert_eq!(cache.entry_count(), 2);
        assert_eq!(cache.get(&"c"), Some(&"cindy"));
        assert_eq!(cache.get(&"b"), Some(&"bob"));
        assert_eq!(cache.get(&"a"), None);
    }

    #[test]
    fn insert_after_invalidate_on_full_cache() {
        let mut cache = Cache::new(2);
        cache.insert("a", "alice");
        cache.insert("b", "bob");
        assert_eq!(cache.entry_count(), 2);

        cache.invalidate(&"a");
        assert_eq!(cache.entry_count(), 1);

        cache.insert("c", "cindy");
        assert_eq!(cache.entry_count(), 2);
        assert_eq!(cache.get(&"c"), Some(&"cindy"));
        assert_eq!(cache.get(&"b"), Some(&"bob"));
        assert_eq!(cache.get(&"a"), None);
    }

    #[test]
    fn invalidate_all_panic_safety() {
        use std::panic::catch_unwind;
        use std::panic::AssertUnwindSafe;
        use std::sync::atomic::{AtomicU32, Ordering};

        static DROP_COUNT: AtomicU32 = AtomicU32::new(0);

        struct PanicOnDrop {
            id: u32,
            should_panic: bool,
        }

        impl Drop for PanicOnDrop {
            fn drop(&mut self) {
                DROP_COUNT.fetch_add(1, Ordering::Relaxed);
                if self.should_panic {
                    panic!("intentional panic in drop for id={}", self.id);
                }
            }
        }

        DROP_COUNT.store(0, Ordering::Relaxed);
        let mut cache = Cache::new(10);
        cache.insert(
            1,
            PanicOnDrop {
                id: 1,
                should_panic: false,
            },
        );
        cache.insert(
            2,
            PanicOnDrop {
                id: 2,
                should_panic: true,
            },
        );
        cache.insert(
            3,
            PanicOnDrop {
                id: 3,
                should_panic: false,
            },
        );
        assert_eq!(cache.entry_count(), 3);

        let result = catch_unwind(AssertUnwindSafe(|| {
            cache.invalidate_all();
        }));
        assert!(result.is_err());

        assert_eq!(cache.entry_count(), 0);
        assert_eq!(cache.table.len(), 0);

        cache.insert(
            4,
            PanicOnDrop {
                id: 4,
                should_panic: false,
            },
        );
        assert_eq!(cache.entry_count(), 1);
        assert!(cache.contains_key(&4));
    }

    #[test]
    fn test_debug_format() {
        let mut cache = Cache::new(10);
        cache.insert('a', "alice");
        cache.insert('b', "bob");
        cache.insert('c', "cindy");

        let debug_str = format!("{:?}", cache);
        assert!(debug_str.starts_with('{'));
        assert!(debug_str.contains(r#"'a': "alice""#));
        assert!(debug_str.contains(r#"'b': "bob""#));
        assert!(debug_str.contains(r#"'c': "cindy""#));
        assert!(debug_str.ends_with('}'));
    }

    #[test]
    fn sub_capacity_inserts_skip_eviction() {
        let mut cache = Cache::new(10);
        for i in 0u32..5 {
            cache.insert(i, i * 10);
        }
        assert_eq!(cache.entry_count(), 5);
        for i in 0u32..5 {
            assert_eq!(cache.get(&i), Some(&(i * 10)));
        }
    }

    #[test]
    fn eviction_triggers_when_over_capacity() {
        let mut cache = Cache::new(3);
        cache.enable_frequency_sketch_for_testing();

        cache.insert(1, "a");
        cache.insert(2, "b");
        cache.insert(3, "c");
        assert_eq!(cache.entry_count(), 3);

        for _ in 0..5 {
            cache.get(&1);
            cache.get(&2);
            cache.get(&3);
        }

        cache.insert(4, "d");
        assert!(cache.entry_count() <= 3);
    }

    #[test]
    fn warmup_to_full_transition() {
        let mut cache = Cache::new(4);
        cache.enable_frequency_sketch_for_testing();

        cache.insert(1, "a");
        cache.insert(2, "b");
        assert_eq!(cache.entry_count(), 2);
        assert_eq!(cache.weights_to_evict(), 0);

        cache.insert(3, "c");
        cache.insert(4, "d");
        assert_eq!(cache.entry_count(), 4);
        assert_eq!(cache.weights_to_evict(), 0);

        for _ in 0..5 {
            cache.get(&1);
            cache.get(&2);
            cache.get(&3);
            cache.get(&4);
        }

        cache.insert(5, "e");
        assert!(cache.entry_count() <= 4);
    }

    #[test]
    fn invalidate_and_remove_skip_eviction_below_capacity() {
        let mut cache = Cache::new(10);
        cache.insert(1, "a");
        cache.insert(2, "b");
        cache.insert(3, "c");
        assert_eq!(cache.entry_count(), 3);
        assert_eq!(cache.weights_to_evict(), 0);

        cache.invalidate(&1);
        assert_eq!(cache.entry_count(), 2);

        let val = cache.remove(&2);
        assert_eq!(val, Some("b"));
        assert_eq!(cache.entry_count(), 1);

        assert_eq!(cache.get(&3), Some(&"c"));
    }

    #[test]
    fn peek_returns_value() {
        let mut cache = Cache::new(10);
        cache.insert("a", "alice");
        cache.insert("b", "bob");

        assert_eq!(cache.peek(&"a"), Some(&"alice"));
        assert_eq!(cache.peek(&"b"), Some(&"bob"));
    }

    #[test]
    fn peek_returns_none_for_missing_key() {
        let cache = Cache::<&str, &str>::new(10);
        assert_eq!(cache.peek(&"missing"), None);
    }

    #[test]
    fn peek_does_not_promote() {
        let mut cache = Cache::new(3);
        cache.enable_frequency_sketch_for_testing();

        // Insert three entries to fill the cache. Insertion order in deque: a, b, c.
        cache.insert("a", "alice");
        cache.insert("b", "bob");
        cache.insert("c", "cindy");

        // Use get() on b and c to boost their frequency and promote them in the
        // deque. "a" remains at the LRU front with a low frequency count.
        for _ in 0..5 {
            cache.get(&"b");
            cache.get(&"c");
        }

        // peek() should return the value but NOT promote "a" or boost its frequency.
        assert_eq!(cache.peek(&"a"), Some(&"alice"));

        // Insert a new entry "d" with enough frequency to be admitted.
        // Build up frequency for "d" before inserting.
        let hash_d = cache.hash(&"d");
        for _ in 0..5 {
            cache.frequency_sketch.increment(hash_d);
        }

        // "a" should be the LRU victim since peek() did not promote it.
        cache.insert("d", "david");

        assert_eq!(cache.peek(&"a"), None, "a should have been evicted");
        assert_eq!(cache.peek(&"b"), Some(&"bob"));
        assert_eq!(cache.peek(&"c"), Some(&"cindy"));
        assert_eq!(cache.peek(&"d"), Some(&"david"));
    }

    #[test]
    fn contains_key_with_shared_reference() {
        let mut cache = Cache::new(10);
        cache.insert("a", 1);
        cache.insert("b", 2);
        cache.insert("c", 3);

        let cache_ref: &Cache<&str, i32> = &cache;
        assert!(cache_ref.contains_key(&"a"));
        assert!(cache_ref.contains_key(&"b"));
        assert!(cache_ref.contains_key(&"c"));
        assert!(!cache_ref.contains_key(&"d"));
    }

    #[test]
    fn read_buffer_accumulates_on_hits() {
        let mut cache = Cache::new(10);
        cache.enable_frequency_sketch_for_testing();

        cache.insert("a", 1);
        cache.insert("b", 2);
        assert_eq!(cache.read_buffer_len(), 0);

        cache.get(&"a");
        assert_eq!(cache.read_buffer_len(), 1);

        cache.get(&"b");
        assert_eq!(cache.read_buffer_len(), 2);
    }

    #[test]
    fn read_buffer_skips_misses() {
        let mut cache = Cache::new(10);
        cache.enable_frequency_sketch_for_testing();

        cache.insert("a", 1);
        assert_eq!(cache.read_buffer_len(), 0);

        assert_eq!(cache.get(&"missing"), None);
        assert_eq!(cache.read_buffer_len(), 0);

        cache.get(&"a");
        assert_eq!(cache.read_buffer_len(), 1);

        assert_eq!(cache.get(&"also_missing"), None);
        assert_eq!(cache.read_buffer_len(), 1);
    }

    #[test]
    fn read_buffer_drains_on_insert() {
        let mut cache = Cache::new(10);
        cache.enable_frequency_sketch_for_testing();

        cache.insert("a", 1);
        cache.get(&"a");
        cache.get(&"a");
        cache.get(&"a");
        assert_eq!(cache.read_buffer_len(), 3);

        cache.insert("b", 2);
        assert_eq!(cache.read_buffer_len(), 0);

        let hash_a = cache.hash(&"a");
        assert!(cache.frequency_sketch.frequency(hash_a) >= 3);
    }

    #[test]
    fn read_buffer_drains_at_capacity() {
        let mut cache = Cache::new(100);
        cache.enable_frequency_sketch_for_testing();

        for i in 0u32..65 {
            cache.insert(i, i);
        }

        for i in 0u32..64 {
            cache.get(&i);
        }
        assert_eq!(cache.read_buffer_len(), 64);

        // The 65th hit should trigger a drain before buffering.
        cache.get(&64);
        assert_eq!(cache.read_buffer_len(), 1);
    }

    #[test]
    fn invalidate_all_resets_read_buffer() {
        let mut cache = Cache::new(10);
        cache.enable_frequency_sketch_for_testing();

        cache.insert("a", 1);
        cache.insert("b", 2);
        cache.get(&"a");
        cache.get(&"b");
        assert_eq!(cache.read_buffer_len(), 2);

        cache.invalidate_all();
        assert_eq!(cache.read_buffer_len(), 0);
    }
}
