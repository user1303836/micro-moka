use super::{deques::Deques, CacheBuilder, Iter, KeyHashDate, ValueEntry};
use crate::{
    common::{self, deque::DeqNode, frequency_sketch::FrequencySketch, CacheRegion},
    Policy,
};

use smallvec::SmallVec;
use std::{
    borrow::Borrow,
    collections::{hash_map::RandomState, HashMap},
    fmt,
    hash::{BuildHasher, Hash},
    ptr::NonNull,
    rc::Rc,
};

const EVICTION_BATCH_SIZE: usize = 100;

type CacheStore<K, V, S> = std::collections::HashMap<Rc<K>, ValueEntry<K, V>, S>;

/// An in-memory cache that is _not_ thread-safe.
///
/// `Cache` utilizes a hash table [`std::collections::HashMap`][std-hashmap] from the
/// standard library for the central key-value storage. `Cache` performs a
/// best-effort bounding of the map using an entry replacement algorithm to determine
/// which entries to evict when the capacity is exceeded.
///
/// [std-hashmap]: https://doc.rust-lang.org/std/collections/struct.HashMap.html
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
    frequency_sketch: FrequencySketch,
    frequency_sketch_enabled: bool,
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
            frequency_sketch: Default::default(),
            frequency_sketch_enabled: false,
        }
    }

    /// Returns `true` if the cache contains a value for the key.
    ///
    /// Unlike the `get` method, this method is not considered a cache read operation,
    /// so it does not update the historic popularity estimator.
    ///
    /// The key may be any borrowed form of the cache's key type, but `Hash` and `Eq`
    /// on the borrowed form _must_ match those for the key type.
    pub fn contains_key<Q>(&mut self, key: &Q) -> bool
    where
        Rc<K>: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.evict_lru_entries();
        self.cache.contains_key(key)
    }

    /// Returns an immutable reference of the value corresponding to the key.
    ///
    /// The key may be any borrowed form of the cache's key type, but `Hash` and `Eq`
    /// on the borrowed form _must_ match those for the key type.
    pub fn get<Q>(&mut self, key: &Q) -> Option<&V>
    where
        Rc<K>: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.evict_lru_entries();
        self.frequency_sketch.increment(self.hash(key));

        if let Some(entry) = self.cache.get_mut(key) {
            Self::record_hit(&mut self.deques, entry);
            Some(&entry.value)
        } else {
            None
        }
    }

    /// Inserts a key-value pair into the cache.
    ///
    /// If the cache has this key present, the value is updated.
    pub fn insert(&mut self, key: K, value: V) {
        self.evict_lru_entries();
        let policy_weight = 1;
        let key = Rc::new(key);
        let entry = ValueEntry::new(value);

        if let Some(old_entry) = self.cache.insert(Rc::clone(&key), entry) {
            self.handle_update(key, policy_weight, old_entry);
        } else {
            let hash = self.hash(&key);
            self.handle_insert(key, hash, policy_weight);
        }
    }

    /// Discards any cached value for the key.
    ///
    /// The key may be any borrowed form of the cache's key type, but `Hash` and `Eq`
    /// on the borrowed form _must_ match those for the key type.
    pub fn invalidate<Q>(&mut self, key: &Q)
    where
        Rc<K>: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.evict_lru_entries();

        if let Some(mut entry) = self.cache.remove(key) {
            self.deques.unlink_ao(&mut entry);
            self.entry_count -= 1;
        }
    }

    /// Discards any cached value for the key, returning the cached value.
    ///
    /// The key may be any borrowed form of the cache's key type, but `Hash` and `Eq`
    /// on the borrowed form _must_ match those for the key type.
    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        Rc<K>: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.evict_lru_entries();

        if let Some(mut entry) = self.cache.remove(key) {
            self.deques.unlink_ao(&mut entry);
            self.entry_count -= 1;
            Some(entry.value)
        } else {
            None
        }
    }

    /// Discards all cached values.
    ///
    /// Like the `invalidate` method, this method does not clear the historic
    /// popularity estimator of keys so that it retains the client activities of
    /// trying to retrieve an item.
    pub fn invalidate_all(&mut self) {
        // Phase 1: swap out the cache before resetting internal state so that
        // a panic in V::drop leaves `self` in a consistent (empty) state.
        let old_capacity = self.cache.capacity();
        let old_cache = std::mem::replace(
            &mut self.cache,
            HashMap::with_hasher(self.build_hasher.clone()),
        );
        self.deques.clear();
        self.entry_count = 0;

        // If V::drop panics, `self` is already in a valid empty state.
        drop(old_cache);

        // Phase 2: best effort capacity restoration for future inserts.
        let _ = self.cache.try_reserve(old_capacity);
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
    // -----------------------------------------------------------------------
    // (The followings are not doc comments)
    // We need this #[allow(...)] to avoid a false Clippy warning about needless
    // collect to create keys_to_invalidate.
    // clippy 0.1.52 (9a1dfd2dc5c 2021-04-30) in Rust 1.52.0-beta.7
    #[allow(clippy::needless_collect)]
    pub fn invalidate_entries_if(&mut self, mut predicate: impl FnMut(&K, &V) -> bool) {
        let Self { cache, deques, .. } = self;

        // Since we can't do cache.iter() and cache.remove() at the same time,
        // invalidation needs to run in two steps:
        // 1. Examine all entries in this cache and collect keys to invalidate.
        // 2. Remove entries for the keys.

        let keys_to_invalidate = cache
            .iter()
            .filter(|(key, entry)| (predicate)(key, &entry.value))
            .map(|(key, _)| Rc::clone(key))
            .collect::<Vec<_>>();

        let mut invalidated = 0u64;

        keys_to_invalidate.into_iter().for_each(|k| {
            if let Some(mut entry) = cache.remove(&k) {
                let _weight = entry.policy_weight();
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

    fn record_hit(deques: &mut Deques<K>, entry: &mut ValueEntry<K, V>) {
        deques.move_to_back_ao(entry)
    }

    fn has_enough_capacity(&self, candidate_weight: u32, ws: u64) -> bool {
        self.max_capacity
            .map(|limit| ws + candidate_weight as u64 <= limit)
            .unwrap_or(true)
    }

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
    fn handle_insert(&mut self, key: Rc<K>, hash: u64, policy_weight: u32) {
        let has_free_space = self.has_enough_capacity(policy_weight, self.entry_count);
        let (cache, deqs, freq) = (&mut self.cache, &mut self.deques, &self.frequency_sketch);

        if has_free_space {
            // Add the candidate to the deque.
            let key = Rc::clone(&key);
            let entry = cache.get_mut(&key).unwrap();
            deqs.push_back_ao(
                CacheRegion::MainProbation,
                KeyHashDate::new(Rc::clone(&key), hash),
                entry,
            );
            self.entry_count += 1;
            // self.saturating_add_to_total_weight(policy_weight as u64);

            if self.should_enable_frequency_sketch() {
                self.enable_frequency_sketch();
            }

            return;
        }

        if let Some(max) = self.max_capacity {
            if policy_weight as u64 > max {
                // The candidate is too big to fit in the cache. Reject it.
                cache.remove(&Rc::clone(&key));
                return;
            }
        }

        let mut candidate = EntrySizeAndFrequency::new(policy_weight as u64);
        candidate.add_frequency(freq, hash);

        match Self::admit(&candidate, cache, deqs, freq) {
            AdmissionResult::Admitted { victim_nodes } => {
                // Remove the victims from the cache (hash map) and deque.
                for victim in victim_nodes {
                    // Remove the victim from the hash map.
                    let mut vic_entry = cache
                        .remove(unsafe { &victim.as_ref().element.key })
                        .expect("Cannot remove a victim from the hash map");
                    // And then remove the victim from the deques.
                    deqs.unlink_ao(&mut vic_entry);
                    // Deques::unlink_wo(&mut deqs.write_order, &mut vic_entry);
                    self.entry_count -= 1;
                }

                // Add the candidate to the deque.
                let entry = cache.get_mut(&key).unwrap();
                let key = Rc::clone(&key);
                deqs.push_back_ao(
                    CacheRegion::MainProbation,
                    KeyHashDate::new(Rc::clone(&key), hash),
                    entry,
                );

                self.entry_count += 1;
                // Self::saturating_sub_from_total_weight(self, victims_weight);
                // Self::saturating_add_to_total_weight(self, policy_weight as u64);

                if self.should_enable_frequency_sketch() {
                    self.enable_frequency_sketch();
                }
            }
            AdmissionResult::Rejected => {
                // Remove the candidate from the cache.
                cache.remove(&key);
            }
        }
    }

    /// Performs size-aware admission explained in the paper:
    /// [Lightweight Robust Size Aware Cache Management][size-aware-cache-paper]
    /// by Gil Einziger, Ohad Eytan, Roy Friedman, Ben Manes.
    ///
    /// [size-aware-cache-paper]: https://arxiv.org/abs/2105.08770
    ///
    /// There are some modifications in this implementation:
    /// - To admit to the main space, candidate's frequency must be higher than
    ///   the aggregated frequencies of the potential victims. (In the paper,
    ///   `>=` operator is used rather than `>`)  The `>` operator will do a better
    ///   job to prevent the main space from polluting.
    /// - When a candidate is rejected, the potential victims will stay at the LRU
    ///   position of the probation access-order queue. (In the paper, they will be
    ///   promoted (to the MRU position?) to force the eviction policy to select a
    ///   different set of victims for the next candidate). We may implement the
    ///   paper's behavior later?
    ///
    #[inline]
    fn admit(
        candidate: &EntrySizeAndFrequency,
        _cache: &CacheStore<K, V, S>,
        deqs: &Deques<K>,
        freq: &FrequencySketch,
    ) -> AdmissionResult<K> {
        let mut victims = EntrySizeAndFrequency::default();
        let mut victim_nodes = SmallVec::default();

        // Get first potential victim at the LRU position.
        let mut next_victim = deqs.probation.peek_front_ptr();

        // Aggregate potential victims.
        while victims.weight < candidate.weight {
            if candidate.freq < victims.freq {
                break;
            }
            if let Some(victim) = next_victim.take() {
                next_victim = DeqNode::next_node_ptr(victim);
                let vic_elem = &unsafe { victim.as_ref() }.element;

                // let vic_entry = cache
                //     .get(&vic_elem.key)
                //     .expect("Cannot get an victim entry");
                victims.add_policy_weight();
                victims.add_frequency(freq, vic_elem.hash);
                victim_nodes.push(victim);
            } else {
                // No more potential victims.
                break;
            }
        }

        // Admit or reject the candidate.

        // TODO: Implement some randomness to mitigate hash DoS attack.
        // See Caffeine's implementation.

        if victims.weight >= candidate.weight && candidate.freq > victims.freq {
            AdmissionResult::Admitted { victim_nodes }
        } else {
            AdmissionResult::Rejected
        }
    }

    fn handle_update(&mut self, key: Rc<K>, policy_weight: u32, old_entry: ValueEntry<K, V>) {
        let entry = self.cache.get_mut(&key).unwrap();
        entry.replace_deq_nodes_with(old_entry);
        entry.set_policy_weight(policy_weight);

        let deqs = &mut self.deques;
        deqs.move_to_back_ao(entry);

        // self.saturating_sub_from_total_weight(old_policy_weight as u64);
        // self.saturating_add_to_total_weight(policy_weight as u64);
    }

    #[inline]
    fn evict_lru_entries(&mut self) {
        const DEQ_NAME: &str = "probation";

        let weights_to_evict = self.weights_to_evict();
        let mut evicted_count = 0u64;
        let mut evicted_policy_weight = 0u64;

        {
            let deqs = &mut self.deques;
            let (probation, cache) = (&mut deqs.probation, &mut self.cache);

            for _ in 0..EVICTION_BATCH_SIZE {
                if evicted_policy_weight >= weights_to_evict {
                    break;
                }

                // clippy::map_clone will give us a false positive warning here.
                // Version: clippy 0.1.77 (f2048098a1c 2024-02-09) in Rust 1.77.0-beta.2
                #[allow(clippy::map_clone)]
                let key = probation
                    .peek_front()
                    .map(|node| Rc::clone(&node.element.key));

                if key.is_none() {
                    break;
                }
                let key = key.unwrap();

                if let Some(mut entry) = cache.remove(&key) {
                    let weight = entry.policy_weight();
                    Deques::unlink_ao_from_deque(DEQ_NAME, probation, &mut entry);
                    evicted_count += 1;
                    evicted_policy_weight = evicted_policy_weight.saturating_add(weight as u64);
                } else {
                    probation.pop_front();
                }
            }
        }

        self.entry_count -= evicted_count;
        // self.saturating_sub_from_total_weight(evicted_policy_weight);
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

#[derive(Default)]
struct EntrySizeAndFrequency {
    weight: u64,
    freq: u32,
}

impl EntrySizeAndFrequency {
    fn new(policy_weight: u64) -> Self {
        Self {
            weight: policy_weight,
            ..Default::default()
        }
    }

    fn add_policy_weight(&mut self) {
        self.weight += 1;
    }

    fn add_frequency(&mut self, freq: &FrequencySketch, hash: u64) {
        self.freq += freq.frequency(hash) as u32;
    }
}

// Access-Order Queue Node
type AoqNode<K> = NonNull<DeqNode<KeyHashDate<K>>>;

enum AdmissionResult<K> {
    Admitted {
        victim_nodes: SmallVec<[AoqNode<K>; 8]>,
    },
    Rejected,
}

//
// private free-standing functions
//

// To see the debug prints, run test as `cargo test -- --nocapture`
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
        // counts: a -> 1, b -> 1

        cache.insert("c", "cindy");
        assert_eq!(cache.get(&"c"), Some(&"cindy"));
        assert!(cache.contains_key(&"c"));
        // counts: a -> 1, b -> 1, c -> 1

        assert!(cache.contains_key(&"a"));
        assert_eq!(cache.get(&"a"), Some(&"alice"));
        assert_eq!(cache.get(&"b"), Some(&"bob"));
        assert!(cache.contains_key(&"b"));
        // counts: a -> 2, b -> 2, c -> 1

        // "d" should not be admitted because its frequency is too low.
        cache.insert("d", "david"); //   count: d -> 0
        assert_eq!(cache.get(&"d"), None); //   d -> 1
        assert!(!cache.contains_key(&"d"));

        cache.insert("d", "david");
        assert!(!cache.contains_key(&"d"));
        assert_eq!(cache.get(&"d"), None); //   d -> 2

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

        assert_eq!(cache.cache.len(), 2);

        cache.invalidate_entries_if(|_k, &v| v == "alice");
        cache.invalidate_entries_if(|_k, &v| v == "bob");

        assert!(cache.get(&1).is_none());
        assert!(cache.get(&3).is_none());

        assert!(!cache.contains_key(&1));
        assert!(!cache.contains_key(&3));

        assert_eq!(cache.cache.len(), 0);
    }

    #[cfg_attr(target_pointer_width = "16", ignore)]
    #[test]
    fn test_skt_capacity_will_not_overflow() {
        // power of two
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
            // due to ceiling to next_power_of_two
            ensure_sketch_len(pot16 + 1, pot(17), "pot16 + 1");
            // due to ceiling to next_power_of_two
            ensure_sketch_len(pot24 - 1, pot24, "pot24 - 1");
            ensure_sketch_len(pot24, pot24, "pot24");
            ensure_sketch_len(pot(27), pot24, "pot(27)");
            ensure_sketch_len(u32::MAX as u64, pot24, "u32::MAX");
        } else {
            // target_pointer_width: 64 or larger.
            let pot30 = pot(30);
            let pot16 = pot(16);
            ensure_sketch_len(0, 128, "0");
            ensure_sketch_len(128, 128, "128");
            ensure_sketch_len(pot16, pot16, "pot16");
            // due to ceiling to next_power_of_two
            ensure_sketch_len(pot16 + 1, pot(17), "pot16 + 1");

            // The following tests will allocate large memory (~8GiB).
            // Skip when running on Circle CI.
            if !cfg!(circleci) {
                // due to ceiling to next_power_of_two
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
}
