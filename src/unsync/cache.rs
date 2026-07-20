use super::{CacheBuilder, IndexDeque, Iter, Slab, SlabEntry, DEFAULT_ADMISSION_SCAN_LIMIT};
use crate::Policy;

use hashbrown::HashTable;
use std::{
    borrow::Borrow,
    collections::hash_map::RandomState,
    fmt,
    hash::{BuildHasher, Hash},
};

/// An in-memory cache that is _not_ thread-safe.
///
/// `Cache` uses [`hashbrown::HashTable`][hb-hashtable] for key lookup and a
/// contiguous slab for key-value storage. A configured maximum capacity bounds
/// the number of resident entries.
///
/// [hb-hashtable]: https://docs.rs/hashbrown/latest/hashbrown/struct.HashTable.html
///
/// # Single-threaded by design
///
/// `Cache` requires mutable access for policy-updating operations and contains
/// no synchronization. It is intended to be owned by one thread or task.
/// [`get`](Self::get) marks a SIEVE visited bit, while [`peek`](Self::peek)
/// provides a non-promoting lookup through a shared reference.
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
/// By default, `Cache` uses the same hashing algorithm as
/// `std::collections::HashMap`, selected to provide resistance against HashDoS
/// attacks. The exact algorithm is intentionally unspecified by the standard
/// library.
///
/// Alternative hashing algorithms may outperform the default for small keys such
/// as integers and large keys such as long strings. However, those algorithms do
/// not always protect against attacks such as HashDoS.
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
    admission_scan_limit: u32,
    entry_count: u64,
    table: HashTable<u32>,
    build_hasher: S,
    slab: Slab<K, V>,
    deque: IndexDeque,
}

impl<K, V, S> fmt::Debug for Cache<K, V, S>
where
    K: fmt::Debug,
    V: fmt::Debug,
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
        Self::with_everything(
            Some(max_capacity),
            None,
            DEFAULT_ADMISSION_SCAN_LIMIT,
            build_hasher,
        )
    }

    /// Returns a [`CacheBuilder`][builder-struct], which can build a `Cache` with
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
        Policy::new(self.max_capacity, self.admission_scan_limit)
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

    /// Creates an iterator visiting all key-value pairs in arbitrary order. The
    /// iterator element type is `(&K, &V)`.
    ///
    /// Unlike the `get` method, visiting entries via an iterator does not mark
    /// entries as visited for eviction purposes.
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
    pub fn iter(&self) -> Iter<'_, K, V> {
        Iter::new(&self.slab.entries, self.entry_count as usize)
    }
}

impl<K, V, S> Cache<K, V, S>
where
    K: Hash + Eq,
    S: BuildHasher,
{
    pub(crate) fn with_everything(
        max_capacity: Option<u64>,
        initial_capacity: Option<usize>,
        admission_scan_limit: u32,
        build_hasher: S,
    ) -> Self {
        let init_cap = initial_capacity.unwrap_or_default();

        Self {
            max_capacity,
            admission_scan_limit,
            entry_count: 0,
            table: HashTable::with_capacity(init_cap),
            build_hasher,
            slab: if init_cap > 0 {
                Slab::with_capacity(init_cap)
            } else {
                Slab::new()
            },
            deque: IndexDeque::default(),
        }
    }

    /// Returns `true` if the cache contains a value for the key.
    ///
    /// Unlike the `get` method, this method is not considered a cache read operation,
    /// so it does not mark the entry as visited.
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

        let entry = self.slab.get_mut(idx);
        entry.mark_visited();
        Some(&entry.value)
    }

    /// Returns an immutable reference of the value corresponding to the key,
    /// without marking the entry as visited.
    ///
    /// Unlike [`get`](#method.get), this method does not count as a cache read
    /// for eviction purposes: the entry's visited bit is not set. This is useful
    /// when you want to inspect the cache without influencing which entries get
    /// evicted, or when you only have a shared (`&self`) reference.
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

    /// Returns a reference to the value corresponding to the key, computing and
    /// inserting it if not present.
    ///
    /// If the cache contains the key, the existing value is returned and the
    /// entry is marked as visited. If the key is absent, `f` is called to
    /// compute the value, which is then inserted and returned.
    ///
    /// This performs a single hash computation and avoids the extra lookup
    /// required by a separate `get()` followed by `insert()`.
    ///
    /// A zero-capacity cache retains the value produced by this method because
    /// the returned reference must remain valid. A later miss replaces that value.
    ///
    /// # Example
    ///
    /// ```rust
    /// use micro_moka::unsync::Cache;
    ///
    /// let mut cache = Cache::new(100);
    ///
    /// let value = cache.get_or_insert_with("key", || "computed".to_string());
    /// assert_eq!(value, "computed");
    ///
    /// // Second call returns the cached value without calling the closure.
    /// let value = cache.get_or_insert_with("key", || panic!("should not be called"));
    /// assert_eq!(value, "computed");
    /// ```
    #[inline]
    pub fn get_or_insert_with<F>(&mut self, key: K, f: F) -> &V
    where
        F: FnOnce() -> V,
    {
        let hash = self.hash(&key);

        if let Some(&idx) = self
            .table
            .find(hash, |&idx| self.slab.get(idx).key.borrow() == &key)
        {
            let entry = self.slab.get_mut(idx);
            entry.mark_visited();
            return &entry.value;
        }

        let value = f();

        if !self.has_enough_capacity() {
            self.sieve_evict_one();
        }

        let idx = self.insert_hashed(key, value, hash);

        &self.slab.get(idx).value
    }

    /// Inserts a key-value pair into the cache.
    ///
    /// If the cache has this key present, the value is updated. Otherwise, a
    /// nonzero-capacity cache always admits the new entry. An exact SIEVE sweep
    /// can inspect the whole resident set when every entry was visited; use
    /// [`try_insert`](Self::try_insert) to place a bound on that policy work and
    /// recover a deferred candidate.
    #[inline]
    pub fn insert(&mut self, key: K, value: V) {
        let hash = self.hash(&key);

        if let Some(&idx) = self
            .table
            .find(hash, |&idx| self.slab.get(idx).key.borrow() == &key)
        {
            let entry = self.slab.get_mut(idx);
            entry.value = value;
            entry.mark_visited();
            return;
        }

        if !self.has_enough_capacity() {
            if self.max_capacity == Some(0) {
                return;
            }

            self.sieve_evict_one();
        }

        self.insert_hashed(key, value, hash);
    }

    /// Attempts to insert a key-value pair with bounded eviction-policy work.
    ///
    /// Existing entries are always updated. A new entry is admitted immediately
    /// while the cache has room. When the cache is full, this method examines at
    /// most [`Policy::admission_scan_limit`] resident entries while looking for
    /// an unvisited SIEVE victim. If every examined entry was visited, the
    /// candidate is returned as `Err((key, value))` and the SIEVE hand is saved
    /// so the next admission attempt resumes the sweep.
    ///
    /// This gives latency-sensitive callers a deterministic bound on SIEVE scan
    /// work and prevents a stream of one-hit candidates from immediately
    /// displacing recently accessed residents. Use [`insert`](Self::insert) when
    /// every candidate must be admitted regardless of scan work.
    ///
    /// [`Policy::admission_scan_limit`]: crate::Policy::admission_scan_limit
    ///
    /// # Example
    ///
    /// ```rust
    /// use micro_moka::unsync::Cache;
    ///
    /// let mut cache = Cache::builder()
    ///     .max_capacity(2)
    ///     .admission_scan_limit(1)
    ///     .build();
    /// cache.insert("a", 1);
    /// cache.insert("b", 2);
    /// cache.get(&"a");
    /// cache.get(&"b");
    ///
    /// assert_eq!(cache.try_insert("scan", 3), Err(("scan", 3)));
    /// assert_eq!(cache.entry_count(), 2);
    /// ```
    #[inline]
    pub fn try_insert(&mut self, key: K, value: V) -> Result<(), (K, V)> {
        let hash = self.hash(&key);

        if let Some(&idx) = self
            .table
            .find(hash, |&idx| self.slab.get(idx).key.borrow() == &key)
        {
            let entry = self.slab.get_mut(idx);
            entry.value = value;
            entry.mark_visited();
            return Ok(());
        }

        if !self.has_enough_capacity()
            && (self.max_capacity == Some(0) || !self.try_sieve_evict_one())
        {
            return Err((key, value));
        }

        self.insert_hashed(key, value, hash);
        Ok(())
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
            self.deque.advance_hand_past(&self.slab, idx);
            self.deque.unlink(&mut self.slab, idx);
            let removed = self.slab.deallocate(idx);
            self.entry_count -= 1;
            drop(removed);
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
            self.deque.advance_hand_past(&self.slab, idx);
            self.deque.unlink(&mut self.slab, idx);
            let slab_entry = self.slab.deallocate(idx);
            self.entry_count -= 1;
            Some(slab_entry.value)
        } else {
            None
        }
    }

    /// Discards all cached values.
    #[cold]
    #[inline(never)]
    pub fn invalidate_all(&mut self) {
        let old_capacity = self.table.capacity();
        let old_slab_capacity = self.slab.entries.capacity();
        let old_table = std::mem::replace(&mut self.table, HashTable::new());
        let old_slab = std::mem::replace(&mut self.slab, Slab::new());
        self.deque.clear();
        self.entry_count = 0;

        drop(old_table);
        drop(old_slab);

        self.table.reserve(old_capacity, |&idx| {
            // This closure is for rehashing during reserve. Since the table is
            // empty after the swap, this will never be called, but we must
            // provide it.
            let _ = idx;
            0
        });
        self.slab.entries.reserve(old_slab_capacity);
    }

    /// Discards cached values that satisfy a predicate.
    ///
    /// `invalidate_entries_if` takes a closure that returns `true` or `false`.
    /// `invalidate_entries_if` will apply the closure to each cached value,
    /// and if the closure returns `true`, the value will be invalidated.
    #[cold]
    #[inline(never)]
    pub fn invalidate_entries_if(&mut self, mut predicate: impl FnMut(&K, &V) -> bool) {
        let indices_to_invalidate: Vec<u32> = self
            .slab
            .iter()
            .filter(|(_, entry)| predicate(&entry.key, &entry.value))
            .map(|(idx, _)| idx)
            .collect();

        for idx in indices_to_invalidate {
            let hash = self.slab.get(idx).hash;
            if let Ok(entry) = self.table.find_entry(hash, |&table_idx| table_idx == idx) {
                entry.remove();
                self.deque.advance_hand_past(&self.slab, idx);
                self.deque.unlink(&mut self.slab, idx);
                let removed = self.slab.deallocate(idx);
                self.entry_count -= 1;
                drop(removed);
            }
        }
    }
}

//
// private
//
impl<K, V, S> Cache<K, V, S>
where
    K: Hash + Eq,
    S: BuildHasher,
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
    fn has_enough_capacity(&self) -> bool {
        self.max_capacity
            .map(|limit| self.entry_count < limit)
            .unwrap_or(true)
    }

    #[inline]
    fn insert_hashed(&mut self, key: K, value: V, hash: u64) -> u32 {
        let idx = self.slab.allocate(SlabEntry::new(key, value, hash));
        let slab = &self.slab;
        self.table
            .insert_unique(hash, idx, |&existing_idx| slab.get(existing_idx).hash);
        self.deque.push_back(&mut self.slab, idx);
        self.entry_count += 1;
        idx
    }

    #[cold]
    #[inline(never)]
    fn sieve_evict_one(&mut self) {
        if let Some(victim_idx) = self.deque.sieve_evict(&mut self.slab) {
            self.remove_eviction_victim(victim_idx);
        }
    }

    #[cold]
    #[inline(never)]
    fn try_sieve_evict_one(&mut self) -> bool {
        let victim = self
            .deque
            .sieve_evict_with_scan_limit(&mut self.slab, self.admission_scan_limit);
        if let Some(victim_idx) = victim {
            self.remove_eviction_victim(victim_idx);
            true
        } else {
            false
        }
    }

    fn remove_eviction_victim(&mut self, victim_idx: u32) {
        let victim_hash = self.slab.get(victim_idx).hash;
        if let Ok(entry) = self
            .table
            .find_entry(victim_hash, |&table_idx| table_idx == victim_idx)
        {
            entry.remove();
        }
        let removed = self.slab.deallocate(victim_idx);
        self.entry_count -= 1;
        drop(removed);
    }
}

#[cfg(test)]
mod tests {
    use super::Cache;

    struct DropBomb(bool);

    impl Drop for DropBomb {
        fn drop(&mut self) {
            if self.0 {
                panic!("intentional drop panic");
            }
        }
    }

    #[test]
    fn basic_single_thread() {
        let mut cache = Cache::new(3);

        cache.insert("a", "alice");
        cache.insert("b", "bob");
        assert_eq!(cache.get(&"a"), Some(&"alice"));
        assert!(cache.contains_key(&"a"));
        assert!(cache.contains_key(&"b"));
        assert_eq!(cache.get(&"b"), Some(&"bob"));

        cache.insert("c", "cindy");
        assert_eq!(cache.get(&"c"), Some(&"cindy"));
        assert!(cache.contains_key(&"c"));

        assert!(cache.contains_key(&"a"));
        assert_eq!(cache.get(&"a"), Some(&"alice"));
        assert_eq!(cache.get(&"b"), Some(&"bob"));
        assert!(cache.contains_key(&"b"));

        // All entries are visited. SIEVE will clear visited bits during sweep
        // and evict the first unvisited entry it finds.
        cache.insert("d", "david");
        assert_eq!(cache.entry_count(), 3);
        assert!(cache.contains_key(&"d"));

        cache.invalidate(&"b");
        assert_eq!(cache.get(&"b"), None);
        assert!(!cache.contains_key(&"b"));
    }

    #[test]
    fn sieve_evicts_unvisited_entry() {
        let mut cache = Cache::new(3);

        cache.insert("a", "alice");
        cache.insert("b", "bob");
        cache.insert("c", "cindy");

        // Visit only "b" and "c", leaving "a" unvisited (only inserted, never get'd).
        cache.get(&"b");
        cache.get(&"c");

        // Insert "d": SIEVE sweeps from the oldest entry toward newer entries.
        // "a" is unvisited, so it is evicted before visited "b" and "c".
        cache.insert("d", "david");
        assert_eq!(cache.entry_count(), 3);
        assert_eq!(cache.get(&"a"), None);
        assert!(cache.contains_key(&"b"));
        assert!(cache.contains_key(&"c"));
        assert!(cache.contains_key(&"d"));
    }

    #[test]
    fn sieve_evicts_oldest_when_every_resident_is_unvisited() {
        let mut cache = Cache::new(3);
        cache.insert("a", "alice");
        cache.insert("b", "bob");
        cache.insert("c", "cindy");

        cache.insert("d", "david");

        assert!(!cache.contains_key(&"a"));
        assert!(cache.contains_key(&"b"));
        assert!(cache.contains_key(&"c"));
        assert!(cache.contains_key(&"d"));
    }

    #[test]
    fn sieve_visited_entries_get_second_chance() {
        let mut cache = Cache::new(3);

        cache.insert("a", "alice");
        cache.insert("b", "bob");
        cache.insert("c", "cindy");

        // Visit all entries.
        cache.get(&"a");
        cache.get(&"b");
        cache.get(&"c");

        // Insert "d": SIEVE sweeps and clears all visited bits, then wraps
        // around and evicts the oldest now-unvisited entry.
        cache.insert("d", "david");
        assert_eq!(cache.entry_count(), 3);
        assert!(cache.contains_key(&"d"));
    }

    #[test]
    fn sieve_new_entry_always_admitted() {
        let mut cache = Cache::new(3);

        cache.insert("a", "alice");
        cache.insert("b", "bob");
        cache.insert("c", "cindy");

        // Unlike W-TinyLFU, SIEVE always admits new entries (no frequency check).
        // Even with all existing entries heavily accessed, the new entry is admitted.
        for _ in 0..10 {
            cache.get(&"a");
            cache.get(&"b");
            cache.get(&"c");
        }

        cache.insert("d", "david");
        assert_eq!(cache.entry_count(), 3);
        assert!(cache.contains_key(&"d"));
    }

    #[test]
    fn invalidate_all() {
        let mut cache = Cache::new(100);

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
    fn invalidate_is_consistent_when_value_drop_panics() {
        use std::panic::{catch_unwind, AssertUnwindSafe};

        let mut cache = Cache::new(2);
        cache.insert("bomb", DropBomb(true));

        let result = catch_unwind(AssertUnwindSafe(|| cache.invalidate(&"bomb")));
        assert!(result.is_err());
        assert_eq!(cache.entry_count(), 0);
        assert_eq!(cache.table.len(), 0);
        assert_eq!(cache.slab.iter().count(), 0);

        cache.insert("safe", DropBomb(false));
        assert_eq!(cache.entry_count(), 1);
        assert!(cache.contains_key(&"safe"));
    }

    #[test]
    fn eviction_is_consistent_when_value_drop_panics() {
        use std::panic::{catch_unwind, AssertUnwindSafe};

        let mut cache = Cache::new(1);
        cache.insert("bomb", DropBomb(true));

        let result = catch_unwind(AssertUnwindSafe(|| cache.insert("new", DropBomb(false))));
        assert!(result.is_err());
        assert_eq!(cache.entry_count(), 0);
        assert_eq!(cache.table.len(), 0);
        assert_eq!(cache.slab.iter().count(), 0);

        cache.insert("safe", DropBomb(false));
        assert_eq!(cache.entry_count(), 1);
        assert!(cache.contains_key(&"safe"));
    }

    #[test]
    fn predicate_invalidation_is_consistent_when_value_drop_panics() {
        use std::panic::{catch_unwind, AssertUnwindSafe};

        let mut cache = Cache::new(2);
        cache.insert("bomb", DropBomb(true));
        cache.insert("safe", DropBomb(false));

        let result = catch_unwind(AssertUnwindSafe(|| {
            cache.invalidate_entries_if(|_, value| value.0);
        }));
        assert!(result.is_err());
        assert_eq!(cache.entry_count(), 1);
        assert_eq!(cache.table.len(), 1);
        assert_eq!(cache.slab.iter().count(), 1);
        assert!(cache.contains_key(&"safe"));
    }

    #[test]
    fn invalidate_all_preserves_allocated_capacity() {
        let mut cache = Cache::<u32, u32>::builder()
            .max_capacity(128)
            .initial_capacity(128)
            .build();
        let table_capacity = cache.table.capacity();
        let slab_capacity = cache.slab.entries.capacity();
        cache.insert(1, 1);

        cache.invalidate_all();

        assert!(cache.table.capacity() >= table_capacity);
        assert!(cache.slab.entries.capacity() >= slab_capacity);
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

        cache.insert(1, "a");
        cache.insert(2, "b");
        cache.insert(3, "c");
        assert_eq!(cache.entry_count(), 3);

        cache.insert(4, "d");
        assert!(cache.entry_count() <= 3);
    }

    #[test]
    fn warmup_to_full_transition() {
        let mut cache = Cache::new(4);

        cache.insert(1, "a");
        cache.insert(2, "b");
        assert_eq!(cache.entry_count(), 2);

        cache.insert(3, "c");
        cache.insert(4, "d");
        assert_eq!(cache.entry_count(), 4);

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
    fn peek_does_not_set_visited() {
        let mut cache = Cache::new(3);

        cache.insert("a", "alice");
        cache.insert("b", "bob");
        cache.insert("c", "cindy");

        // peek() should NOT set the visited bit.
        cache.peek(&"a");
        cache.peek(&"b");
        cache.peek(&"c");

        // None of the entries are visited, so SIEVE evicts the oldest entry.
        cache.insert("d", "david");

        // One entry was evicted to make room for "d".
        assert_eq!(cache.entry_count(), 3);
        assert!(cache.contains_key(&"d"));
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
    fn zero_capacity_insert_returns_immediately() {
        let mut cache = Cache::new(0);
        cache.insert("a", "alice");
        assert_eq!(cache.entry_count(), 0);
        assert!(!cache.contains_key(&"a"));
        assert_eq!(cache.get(&"a"), None);
    }

    #[test]
    fn update_existing_key_sets_visited() {
        let mut cache = Cache::new(3);

        cache.insert("a", "alice");
        cache.insert("b", "bob");
        cache.insert("c", "cindy");

        // Update "a" with a new value. This should set visited=true but NOT
        // move it to the back (SIEVE maintains insertion order).
        cache.insert("a", "anna");
        assert_eq!(cache.get(&"a"), Some(&"anna"));
        assert_eq!(cache.entry_count(), 3);

        // "b" and "c" are not visited. Insert "d" to trigger eviction.
        // SIEVE should skip "a" (visited) and evict an unvisited entry.
        cache.insert("d", "david");
        assert_eq!(cache.entry_count(), 3);
        assert!(cache.contains_key(&"a"));
        assert!(cache.contains_key(&"d"));
    }

    #[test]
    fn sieve_hand_advances_across_evictions() {
        let mut cache = Cache::new(3);

        cache.insert("a", "alice");
        cache.insert("b", "bob");
        cache.insert("c", "cindy");

        // No entries visited. First eviction should evict the oldest entry.
        cache.insert("d", "david");
        assert_eq!(cache.entry_count(), 3);
        assert!(!cache.contains_key(&"a"));

        // Insert another to trigger second eviction. Hand should have advanced.
        cache.insert("e", "eve");
        assert_eq!(cache.entry_count(), 3);
        assert!(!cache.contains_key(&"b"));

        // Insert a third to trigger third eviction.
        cache.insert("f", "frank");
        assert_eq!(cache.entry_count(), 3);
        assert!(!cache.contains_key(&"c"));
    }

    #[test]
    fn sieve_multiple_evictions_cycle() {
        let mut cache = Cache::new(2);

        for i in 0u32..20 {
            cache.insert(i, i * 10);
            assert!(cache.entry_count() <= 2);
        }

        // The last two inserted should be present.
        assert_eq!(cache.entry_count(), 2);
        assert!(cache.contains_key(&19));
        assert!(cache.contains_key(&18));
    }

    #[test]
    fn update_below_capacity_no_eviction() {
        let mut cache = Cache::new(5);

        cache.insert("a", "alice");
        cache.insert("b", "bob");
        cache.insert("c", "cindy");
        assert_eq!(cache.entry_count(), 3);

        // Updating below capacity should not evict anything.
        cache.insert("b", "betty");
        assert_eq!(cache.entry_count(), 3);
        assert_eq!(cache.get(&"b"), Some(&"betty"));
        assert!(cache.contains_key(&"a"));
        assert!(cache.contains_key(&"c"));
    }

    #[test]
    fn get_or_insert_with_basic() {
        let mut cache = Cache::new(10);

        let v = cache.get_or_insert_with("a", || "alice");
        assert_eq!(v, &"alice");
        assert_eq!(cache.entry_count(), 1);

        let v = cache.get_or_insert_with("a", || panic!("should not be called"));
        assert_eq!(v, &"alice");
        assert_eq!(cache.entry_count(), 1);
    }

    #[test]
    fn get_or_insert_with_closure_called_only_on_miss() {
        let mut cache = Cache::new(10);
        let mut call_count = 0;

        cache.get_or_insert_with("a", || {
            call_count += 1;
            "alice"
        });
        assert_eq!(call_count, 1);

        cache.get_or_insert_with("a", || {
            call_count += 1;
            "anna"
        });
        assert_eq!(call_count, 1);

        cache.get_or_insert_with("b", || {
            call_count += 1;
            "bob"
        });
        assert_eq!(call_count, 2);
    }

    #[test]
    fn get_or_insert_with_eviction_at_capacity() {
        let mut cache = Cache::new(3);

        cache.get_or_insert_with("a", || "alice");
        cache.get_or_insert_with("b", || "bob");
        cache.get_or_insert_with("c", || "cindy");
        assert_eq!(cache.entry_count(), 3);

        cache.get_or_insert_with("d", || "david");
        assert_eq!(cache.entry_count(), 3);
        assert!(cache.contains_key(&"d"));
    }

    #[test]
    fn get_or_insert_with_existing_key_no_closure() {
        let mut cache = Cache::new(10);
        cache.insert("a", "alice");

        let v = cache.get_or_insert_with("a", || panic!("should not be called"));
        assert_eq!(v, &"alice");
        assert_eq!(cache.entry_count(), 1);
    }

    #[test]
    fn get_or_insert_with_sets_visited_on_hit() {
        let mut cache = Cache::new(3);

        cache.insert("a", "alice");
        cache.insert("b", "bob");
        cache.insert("c", "cindy");

        // Hit via get_or_insert_with sets visited=true on "a".
        cache.get_or_insert_with("a", || panic!("should not be called"));

        // Insert "d" triggers eviction. SIEVE should skip "a" (visited)
        // and evict an unvisited entry.
        cache.insert("d", "david");
        assert_eq!(cache.entry_count(), 3);
        assert!(cache.contains_key(&"a"));
        assert!(cache.contains_key(&"d"));
    }

    #[test]
    fn get_or_insert_with_zero_capacity() {
        let mut cache = Cache::new(0);

        // Zero-capacity cache still stores the value (we must return &V).
        let v = cache.get_or_insert_with("a", || "alice");
        assert_eq!(v, &"alice");

        // Same key: found in cache, closure not called.
        let v = cache.get_or_insert_with("a", || panic!("should not be called"));
        assert_eq!(v, &"alice");

        // Different key: evicts "a", stores "b".
        let v = cache.get_or_insert_with("b", || "bob");
        assert_eq!(v, &"bob");
        assert_eq!(cache.entry_count(), 1);
        assert!(!cache.contains_key(&"a"));
        assert!(cache.contains_key(&"b"));
    }

    #[test]
    fn get_or_insert_with_after_invalidate() {
        let mut cache = Cache::new(10);
        cache.insert("a", "alice");
        cache.invalidate(&"a");
        assert_eq!(cache.entry_count(), 0);

        let v = cache.get_or_insert_with("a", || "anna");
        assert_eq!(v, &"anna");
        assert_eq!(cache.entry_count(), 1);
    }

    #[test]
    fn get_or_insert_with_after_remove() {
        let mut cache = Cache::new(10);
        cache.insert("a", "alice");
        let removed = cache.remove(&"a");
        assert_eq!(removed, Some("alice"));

        let v = cache.get_or_insert_with("a", || "anna");
        assert_eq!(v, &"anna");
        assert_eq!(cache.entry_count(), 1);
    }

    #[test]
    fn iterator_reports_exact_remaining_length() {
        let mut cache = Cache::new(3);
        cache.insert("a", 1);
        cache.insert("b", 2);
        cache.insert("c", 3);
        cache.invalidate(&"b");

        let mut iter = cache.iter();
        assert_eq!(iter.len(), 2);
        assert_eq!(iter.size_hint(), (2, Some(2)));
        iter.next();
        assert_eq!(iter.len(), 1);
        iter.next();
        assert_eq!(iter.len(), 0);
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn custom_hasher_does_not_need_clone() {
        use crate::unsync::CacheBuilder;
        use std::collections::hash_map::DefaultHasher;
        use std::hash::BuildHasher;

        struct NonCloneBuildHasher;

        impl BuildHasher for NonCloneBuildHasher {
            type Hasher = DefaultHasher;

            fn build_hasher(&self) -> Self::Hasher {
                DefaultHasher::new()
            }
        }

        let mut cache = CacheBuilder::<u32, u32, _>::new(2).build_with_hasher(NonCloneBuildHasher);
        cache.insert(1, 10);
        assert_eq!(cache.get(&1), Some(&10));
        assert_eq!(format!("{cache:?}"), "{1: 10}");
    }

    #[test]
    fn try_insert_admits_below_capacity_and_updates_existing() {
        let mut cache = Cache::new(2);

        assert_eq!(cache.try_insert("a", "alice"), Ok(()));
        assert_eq!(cache.try_insert("a", "anna"), Ok(()));
        assert_eq!(cache.entry_count(), 1);
        assert_eq!(cache.get(&"a"), Some(&"anna"));
    }

    #[test]
    fn try_insert_rejects_after_bounded_hot_scan() {
        let mut cache = Cache::builder()
            .max_capacity(4)
            .admission_scan_limit(2)
            .build();
        for key in 0..4 {
            cache.insert(key, key);
            cache.get(&key);
        }

        assert_eq!(cache.try_insert(4, 4), Err((4, 4)));
        assert_eq!(cache.entry_count(), 4);
        assert!((0..4).all(|key| cache.contains_key(&key)));
        assert_eq!(
            cache
                .slab
                .iter()
                .filter(|(_, entry)| entry.is_visited())
                .count(),
            2
        );
    }

    #[test]
    fn try_insert_resumes_sweep_after_rejection() {
        let mut cache = Cache::builder()
            .max_capacity(4)
            .admission_scan_limit(2)
            .build();
        for key in 0..4 {
            cache.insert(key, key);
            cache.get(&key);
        }

        assert_eq!(cache.try_insert(4, 4), Err((4, 4)));
        assert_eq!(cache.try_insert(5, 5), Err((5, 5)));
        assert_eq!(cache.try_insert(6, 6), Ok(()));
        assert_eq!(cache.entry_count(), 4);
        assert!(cache.contains_key(&6));
    }

    #[test]
    fn removing_the_saved_hand_resumes_toward_newer_entries() {
        let mut cache = Cache::builder()
            .max_capacity(4)
            .admission_scan_limit(1)
            .build();
        for key in 0..4 {
            cache.insert(key, key);
            cache.get(&key);
        }

        assert_eq!(cache.try_insert(4, 4), Err((4, 4)));
        assert_eq!(cache.remove(&1), Some(1));
        cache.insert(1, 1);

        assert_eq!(cache.try_insert(5, 5), Err((5, 5)));
        assert_eq!(cache.try_insert(6, 6), Err((6, 6)));
        assert_eq!(cache.try_insert(7, 7), Ok(()));
        assert!(cache.contains_key(&7));
    }

    #[test]
    fn try_insert_admits_when_budget_finds_unvisited_victim() {
        let mut cache = Cache::builder()
            .max_capacity(4)
            .admission_scan_limit(2)
            .build();
        for key in 0..4 {
            cache.insert(key, key);
        }
        cache.get(&3);

        assert_eq!(cache.try_insert(4, 4), Ok(()));
        assert_eq!(cache.entry_count(), 4);
        assert!(cache.contains_key(&4));
    }

    #[test]
    fn try_insert_zero_budget_rejects_at_capacity() {
        let mut cache = Cache::builder()
            .max_capacity(1)
            .admission_scan_limit(0)
            .build();
        cache.insert("resident", 1);

        assert_eq!(cache.try_insert("resident", 3), Ok(()));
        assert_eq!(cache.try_insert("candidate", 2), Err(("candidate", 2)));
        assert_eq!(cache.get(&"resident"), Some(&3));
    }

    #[test]
    fn try_insert_zero_capacity_returns_candidate() {
        let mut cache = Cache::new(0);

        assert_eq!(cache.try_insert("candidate", 2), Err(("candidate", 2)));
        assert_eq!(cache.entry_count(), 0);
    }

    #[test]
    fn regular_insert_always_admits_after_bounded_rejection() {
        let mut cache = Cache::builder()
            .max_capacity(2)
            .admission_scan_limit(1)
            .build();
        cache.insert(0, 0);
        cache.insert(1, 1);
        cache.get(&0);
        cache.get(&1);

        assert_eq!(cache.try_insert(2, 2), Err((2, 2)));
        cache.insert(2, 2);

        assert_eq!(cache.entry_count(), 2);
        assert!(cache.contains_key(&2));
    }

    #[test]
    fn try_insert_eviction_is_consistent_when_value_drop_panics() {
        use std::panic::{catch_unwind, AssertUnwindSafe};

        let mut cache = Cache::builder()
            .max_capacity(1)
            .admission_scan_limit(1)
            .build();
        cache.insert("bomb", DropBomb(true));

        let result = catch_unwind(AssertUnwindSafe(|| {
            let _ = cache.try_insert("new", DropBomb(false));
        }));
        assert!(result.is_err());
        assert_eq!(cache.entry_count(), 0);
        assert_eq!(cache.table.len(), 0);
        assert_eq!(cache.slab.iter().count(), 0);

        cache.insert("safe", DropBomb(false));
        assert!(cache.contains_key(&"safe"));
    }
}
