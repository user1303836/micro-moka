use super::{deques::Deques, CacheBuilder, Iter, KeyHashDate, ValueEntry};
use crate::Policy;

use crate::common::deque::DeqNode;
use hashbrown::hash_map::RawEntryMut;
use hashbrown::HashMap;
use std::{
    borrow::Borrow,
    collections::hash_map::RandomState,
    fmt,
    hash::{BuildHasher, Hash},
    ptr::NonNull,
    rc::Rc,
};

const EVICTION_BATCH_SIZE: usize = 100;

type CacheStore<K, V, S> = HashMap<Rc<K>, ValueEntry<K, V>, S>;

/// SIEVE hand pointer: tracks the current sweep position in the FIFO queue.
type Hand<K> = Option<NonNull<DeqNode<KeyHashDate<K>>>>;

/// An in-memory cache that is _not_ thread-safe.
///
/// `Cache` utilizes a hash table [`hashbrown::HashMap`][hb-hashmap] for the
/// central key-value storage. `Cache` performs a best-effort bounding of the
/// map using the SIEVE eviction algorithm to determine which entries to evict
/// when the capacity is exceeded.
///
/// [hb-hashmap]: https://docs.rs/hashbrown/latest/hashbrown/struct.HashMap.html
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
    cache: CacheStore<K, V, S>,
    build_hasher: S,
    deques: Deques<K>,
    hand: Hand<K>,
}

impl<K, V, S> fmt::Debug for Cache<K, V, S>
where
    K: fmt::Debug + Eq + Hash,
    V: fmt::Debug,
    // TODO: Remove these bounds from S.
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
        let cache = HashMap::with_capacity_and_hasher(
            initial_capacity.unwrap_or_default(),
            build_hasher.clone(),
        );

        Self {
            max_capacity,
            entry_count: 0,
            cache,
            build_hasher,
            deques: Default::default(),
            hand: None,
        }
    }

    /// Returns `true` if the cache contains a value for the key.
    ///
    /// Unlike the `get` method, this method is not considered a cache read operation,
    /// so it does not update the visited bit.
    ///
    /// The key may be any borrowed form of the cache's key type, but `Hash` and `Eq`
    /// on the borrowed form _must_ match those for the key type.
    #[inline]
    pub fn contains_key<Q>(&mut self, key: &Q) -> bool
    where
        Rc<K>: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.cache.contains_key(key)
    }

    /// Returns an immutable reference of the value corresponding to the key.
    ///
    /// The key may be any borrowed form of the cache's key type, but `Hash` and `Eq`
    /// on the borrowed form _must_ match those for the key type.
    #[inline]
    pub fn get<Q>(&mut self, key: &Q) -> Option<&V>
    where
        Rc<K>: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let hash = self.hash(key);
        match self
            .cache
            .raw_entry_mut()
            .from_key_hashed_nocheck(hash, key)
        {
            RawEntryMut::Occupied(o) => {
                let entry = o.into_mut();
                entry.set_visited(true);
                Some(&entry.value)
            }
            RawEntryMut::Vacant(_) => None,
        }
    }

    /// Inserts a key-value pair into the cache.
    ///
    /// If the cache has this key present, the value is updated.
    #[inline]
    pub fn insert(&mut self, key: K, value: V) {
        let key = Rc::new(key);
        let hash = self.hash(&key);
        let entry = ValueEntry::new(value);

        let old_entry = match self
            .cache
            .raw_entry_mut()
            .from_key_hashed_nocheck(hash, &key)
        {
            RawEntryMut::Occupied(mut o) => Some(std::mem::replace(o.get_mut(), entry)),
            RawEntryMut::Vacant(v) => {
                v.insert_hashed_nocheck(hash, Rc::clone(&key), entry);
                None
            }
        };

        if let Some(old_entry) = old_entry {
            self.handle_update(key, hash, old_entry);
        } else {
            // Evict before adding the new entry to the deque so it cannot
            // become a victim of its own insertion.
            if let Some(limit) = self.max_capacity {
                if self.entry_count >= limit {
                    self.sieve_evict(self.entry_count - limit + 1);
                }
            }
            self.handle_insert(key, hash);
        }
    }

    /// Discards any cached value for the key.
    ///
    /// The key may be any borrowed form of the cache's key type, but `Hash` and `Eq`
    /// on the borrowed form _must_ match those for the key type.
    #[inline]
    pub fn invalidate<Q>(&mut self, key: &Q)
    where
        Rc<K>: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        if let Some(mut entry) = self.cache.remove(key) {
            if let Some(node) = entry.access_order_q_node() {
                self.advance_hand_if_at(node);
            }
            self.deques.unlink_ao(&mut entry);
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
        Rc<K>: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        if let Some(mut entry) = self.cache.remove(key) {
            if let Some(node) = entry.access_order_q_node() {
                self.advance_hand_if_at(node);
            }
            self.deques.unlink_ao(&mut entry);
            self.entry_count -= 1;
            Some(entry.value)
        } else {
            None
        }
    }

    /// Discards all cached values.
    #[cold]
    #[inline(never)]
    pub fn invalidate_all(&mut self) {
        let old_capacity = self.cache.capacity();
        let old_cache = std::mem::replace(
            &mut self.cache,
            HashMap::with_hasher(self.build_hasher.clone()),
        );
        self.deques.clear();
        self.hand = None;
        self.entry_count = 0;

        drop(old_cache);

        let _ = self.cache.try_reserve(old_capacity);
    }

    /// Discards cached values that satisfy a predicate.
    ///
    /// `invalidate_entries_if` takes a closure that returns `true` or `false`.
    /// `invalidate_entries_if` will apply the closure to each cached value,
    /// and if the closure returns `true`, the value will be invalidated.
    #[cold]
    #[inline(never)]
    #[allow(clippy::needless_collect)]
    pub fn invalidate_entries_if(&mut self, mut predicate: impl FnMut(&K, &V) -> bool) {
        let Self {
            cache,
            deques,
            hand,
            ..
        } = self;

        let keys_to_invalidate = cache
            .iter()
            .filter(|(key, entry)| (predicate)(key, &entry.value))
            .map(|(key, _)| Rc::clone(key))
            .collect::<Vec<_>>();

        let mut invalidated = 0u64;

        keys_to_invalidate.into_iter().for_each(|k| {
            if let Some(mut entry) = cache.remove(&k) {
                if let Some(node) = entry.access_order_q_node() {
                    if let Some(h) = *hand {
                        if std::ptr::eq(h.as_ptr(), node.as_ptr()) {
                            *hand = unsafe { (*node.as_ptr()).next_raw() };
                        }
                    }
                }
                deques.unlink_ao(&mut entry);
                invalidated += 1;
            }
        });
        self.entry_count -= invalidated;
    }

    /// Creates an iterator visiting all key-value pairs in arbitrary order. The
    /// iterator element type is `(&K, &V)`.
    ///
    /// Unlike the `get` method, visiting entries via an iterator do not update the
    /// visited bit for keys.
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
        Iter::new(self, self.cache.iter())
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
        Rc<K>: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.build_hasher.hash_one(key)
    }

    #[cfg(test)]
    fn weights_to_evict(&self) -> u64 {
        self.max_capacity
            .map(|limit| self.entry_count.saturating_sub(limit))
            .unwrap_or_default()
    }

    #[inline]
    fn handle_insert(&mut self, key: Rc<K>, hash: u64) {
        let entry = match self
            .cache
            .raw_entry_mut()
            .from_key_hashed_nocheck(hash, &key)
        {
            RawEntryMut::Occupied(o) => o.into_mut(),
            RawEntryMut::Vacant(_) => unreachable!(),
        };
        self.deques
            .push_back_ao(KeyHashDate::new(Rc::clone(&key)), entry);
        self.entry_count += 1;
    }

    #[inline]
    fn handle_update(&mut self, key: Rc<K>, hash: u64, old_entry: ValueEntry<K, V>) {
        let entry = match self
            .cache
            .raw_entry_mut()
            .from_key_hashed_nocheck(hash, &key)
        {
            RawEntryMut::Occupied(o) => o.into_mut(),
            RawEntryMut::Vacant(_) => unreachable!(),
        };
        entry.replace_deq_nodes_with(old_entry);
        entry.set_visited(true);
    }

    /// If the hand is currently pointing at `node`, advance it forward (toward
    /// the tail / newer entries) so that a subsequent unlink does not invalidate
    /// the hand.
    #[inline]
    fn advance_hand_if_at(&mut self, node: NonNull<DeqNode<KeyHashDate<K>>>) {
        if let Some(h) = self.hand {
            if std::ptr::eq(h.as_ptr(), node.as_ptr()) {
                self.hand = unsafe { (*node.as_ptr()).next_raw() };
            }
        }
    }

    /// SIEVE eviction: sweep from the hand position forward through the FIFO
    /// queue (head=oldest toward tail=newest). Entries with visited=true get
    /// their bit cleared; the first unvisited entry is evicted.
    #[cold]
    #[inline(never)]
    fn sieve_evict(&mut self, weights_to_evict: u64) {
        debug_assert!(weights_to_evict > 0);
        let mut evicted_count = 0u64;

        for _ in 0..EVICTION_BATCH_SIZE {
            if evicted_count >= weights_to_evict {
                break;
            }

            if self.hand.is_none() {
                self.hand = self.deques.deque.head_ptr();
            }

            let Some(current) = self.hand else {
                break;
            };

            let key = unsafe { Rc::clone(&current.as_ref().element.key) };

            let visited = self.cache.get(&key).map(|e| e.visited()).unwrap_or(false);

            if visited {
                if let Some(entry) = self.cache.get_mut(&key) {
                    entry.set_visited(false);
                }
                let next = unsafe { (*current.as_ptr()).next_raw() };
                self.hand = if next.is_some() {
                    next
                } else {
                    self.deques.deque.head_ptr()
                };
            } else {
                let next = unsafe { (*current.as_ptr()).next_raw() };
                self.hand = next;

                if let Some(mut entry) = self.cache.remove(&key) {
                    Deques::unlink_ao_from_deque(&mut self.deques.deque, &mut entry);
                    evicted_count += 1;
                } else {
                    unsafe { self.deques.deque.unlink_and_drop(current) };
                }
            }
        }

        self.entry_count -= evicted_count;
    }
}

//
// for testing
//
#[cfg(test)]
impl<K, V, S> Cache<K, V, S>
where
    K: Hash + Eq,
    S: BuildHasher + Clone,
{
}

// To see the debug prints, run test as `cargo test -- --nocapture`
#[cfg(test)]
mod tests {
    use super::Cache;

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

        // All three entries present, a and b have been visited via get()
        assert!(cache.contains_key(&"a"));
        assert_eq!(cache.get(&"a"), Some(&"alice"));
        assert_eq!(cache.get(&"b"), Some(&"bob"));
        assert!(cache.contains_key(&"b"));

        // Insert "d" -- cache is full, SIEVE eviction kicks in.
        // The hand sweeps from the tail. "c" was visited (get above),
        // so its bit gets cleared. "b" was visited, so its bit gets cleared.
        // "a" was visited, so its bit gets cleared. On the next sweep (hand
        // wraps), "c" is at the tail and now unvisited, so it gets evicted.
        cache.insert("d", "david");
        assert_eq!(cache.entry_count(), 3);
        assert!(cache.contains_key(&"d"));

        // "d" should be in the cache
        assert_eq!(cache.get(&"d"), Some(&"david"));

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

        // Visit a and b but not c
        cache.get(&"a");
        cache.get(&"b");

        // Insert d -- should evict c (the unvisited entry)
        cache.insert("d", "david");
        assert_eq!(cache.entry_count(), 3);
        assert_eq!(cache.get(&"c"), None);
        assert_eq!(cache.get(&"a"), Some(&"alice"));
        assert_eq!(cache.get(&"b"), Some(&"bob"));
        assert_eq!(cache.get(&"d"), Some(&"david"));
    }

    #[test]
    fn sieve_clears_visited_before_evicting() {
        let mut cache = Cache::new(2);
        cache.insert("a", "alice");
        cache.insert("b", "bob");

        // Visit both entries
        cache.get(&"a");
        cache.get(&"b");

        // Insert c -- both are visited, so SIEVE clears both bits
        // then evicts the first unvisited entry on the next sweep (oldest)
        cache.insert("c", "cindy");
        assert_eq!(cache.entry_count(), 2);
        assert!(cache.contains_key(&"c"));
        // One of a or b was evicted
        let a_present = cache.contains_key(&"a");
        let b_present = cache.contains_key(&"b");
        assert!(
            (a_present && !b_present) || (!a_present && b_present),
            "exactly one of a/b should be evicted"
        );
    }

    #[test]
    fn sieve_fifo_order_for_unvisited() {
        let mut cache = Cache::new(3);
        cache.insert("a", "alice");
        cache.insert("b", "bob");
        cache.insert("c", "cindy");

        // No gets -- all entries are unvisited.
        // SIEVE hand starts at head ("a"), which is unvisited, so "a" is evicted.
        cache.insert("d", "david");
        assert_eq!(cache.entry_count(), 3);
        assert_eq!(cache.get(&"a"), None);
        assert!(cache.contains_key(&"b"));
        assert!(cache.contains_key(&"c"));
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

        assert_eq!(cache.cache.len(), 2);

        cache.invalidate_entries_if(|_k, &v| v == "alice");
        cache.invalidate_entries_if(|_k, &v| v == "bob");

        assert!(cache.get(&1).is_none());
        assert!(cache.get(&3).is_none());

        assert!(!cache.contains_key(&1));
        assert!(!cache.contains_key(&3));

        assert_eq!(cache.cache.len(), 0);
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
        assert_eq!(cache.cache.len(), 0);

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
        assert_eq!(cache.entry_count(), 3);
    }

    #[test]
    fn warmup_to_full_transition() {
        let mut cache = Cache::new(4);

        cache.insert(1, "a");
        cache.insert(2, "b");
        assert_eq!(cache.entry_count(), 2);
        assert_eq!(cache.weights_to_evict(), 0);

        cache.insert(3, "c");
        cache.insert(4, "d");
        assert_eq!(cache.entry_count(), 4);
        assert_eq!(cache.weights_to_evict(), 0);

        cache.insert(5, "e");
        assert_eq!(cache.entry_count(), 4);
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
    fn update_existing_key_marks_visited() {
        let mut cache = Cache::new(3);
        cache.insert("a", "alice");
        cache.insert("b", "bob");
        cache.insert("c", "cindy");

        // Update "a" -- this should mark it as visited
        cache.insert("a", "alice2");

        // Insert "d" -- should evict an unvisited entry (b or c)
        cache.insert("d", "david");
        assert_eq!(cache.entry_count(), 3);
        assert_eq!(cache.get(&"a"), Some(&"alice2"));
        assert!(cache.contains_key(&"d"));
    }

    #[test]
    fn single_entry_cache() {
        let mut cache = Cache::new(1);
        cache.insert("a", "alice");
        assert_eq!(cache.entry_count(), 1);
        assert_eq!(cache.get(&"a"), Some(&"alice"));

        cache.insert("b", "bob");
        assert_eq!(cache.entry_count(), 1);
        // "a" was visited but is the only candidate, so it gets evicted
        assert_eq!(cache.get(&"a"), None);
        assert_eq!(cache.get(&"b"), Some(&"bob"));
    }

    #[test]
    fn empty_cache_eviction() {
        let mut cache: Cache<&str, &str> = Cache::new(0);
        // With max_capacity=0, every insert triggers immediate eviction.
        // The entry gets inserted then the next insert evicts it.
        cache.insert("a", "alice");
        // entry_count may be 1 because we insert before eviction check on next insert
        // but max_capacity=0 means weights_to_evict() = entry_count - 0 = entry_count
        // so on next insert, the previous entry gets evicted first.
        assert!(cache.entry_count() <= 1);
    }

    #[test]
    fn hand_survives_invalidate_of_pointed_entry() {
        let mut cache = Cache::new(4);
        cache.insert("a", "alice");
        cache.insert("b", "bob");
        cache.insert("c", "cindy");
        cache.insert("d", "david");

        // Visit all to build up visited bits
        cache.get(&"a");
        cache.get(&"b");
        cache.get(&"c");
        cache.get(&"d");

        // Force one eviction to position the hand somewhere
        cache.insert("e", "eve");
        assert_eq!(cache.entry_count(), 4);

        // Now invalidate entries -- hand should remain valid
        cache.invalidate(&"b");
        assert_eq!(cache.entry_count(), 3);

        // Should still be able to insert without issues
        cache.insert("f", "fiona");
        assert!(cache.entry_count() <= 4);
    }

    #[test]
    fn all_visited_full_sweep() {
        let mut cache = Cache::new(3);
        cache.insert(1, "a");
        cache.insert(2, "b");
        cache.insert(3, "c");

        // Visit all entries
        cache.get(&1);
        cache.get(&2);
        cache.get(&3);

        // Insert a new entry -- all are visited, so SIEVE must do a full sweep
        // clearing all bits, then evict the first unvisited entry
        cache.insert(4, "d");
        assert_eq!(cache.entry_count(), 3);
        assert!(cache.contains_key(&4));
    }
}
