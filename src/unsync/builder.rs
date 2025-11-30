use super::Cache;

use std::{
    collections::hash_map::RandomState,
    hash::{BuildHasher, Hash},
    marker::PhantomData,
};

/// Builds a [`Cache`][cache-struct] with various configuration knobs.
///
/// [cache-struct]: ./struct.Cache.html
///
/// # Examples
///
/// ```rust
/// use micro_moka::unsync::Cache;
///
/// let mut cache = Cache::builder()
///     // Max 10,000 elements
///     .max_capacity(10_000)
///     // Create the cache.
///     .build();
///
/// cache.insert(0, "zero");
/// cache.get(&0);
/// ```
///
#[must_use]
pub struct CacheBuilder<K, V, C> {
    max_capacity: Option<u64>,
    initial_capacity: Option<usize>,
    cache_type: PhantomData<C>,
    _marker: PhantomData<(K, V)>,
}

impl<K, V> Default for CacheBuilder<K, V, Cache<K, V, RandomState>>
where
    K: Eq + Hash,
{
    fn default() -> Self {
        Self {
            max_capacity: None,
            initial_capacity: None,
            cache_type: Default::default(),
            _marker: Default::default(),
        }
    }
}

impl<K, V> CacheBuilder<K, V, Cache<K, V, RandomState>>
where
    K: Eq + Hash,
{
    /// Construct a new `CacheBuilder` that will be used to build a `Cache` holding
    /// up to `max_capacity` entries.
    pub fn new(max_capacity: u64) -> Self {
        Self {
            max_capacity: Some(max_capacity),
            ..Default::default()
        }
    }

    /// Builds a `Cache<K, V>`.
    pub fn build(self) -> Cache<K, V, RandomState> {
        let build_hasher = RandomState::default();
        Cache::with_everything(self.max_capacity, self.initial_capacity, build_hasher)
    }

    /// Builds a `Cache<K, V, S>`, with the given `hasher`.
    pub fn build_with_hasher<S>(self, hasher: S) -> Cache<K, V, S>
    where
        S: BuildHasher + Clone,
    {
        Cache::with_everything(self.max_capacity, self.initial_capacity, hasher)
    }
}

impl<K, V, C> CacheBuilder<K, V, C> {
    /// Sets the max capacity of the cache.
    pub fn max_capacity(self, max_capacity: u64) -> Self {
        Self {
            max_capacity: Some(max_capacity),
            ..self
        }
    }

    /// Sets the initial capacity (number of entries) of the cache.
    pub fn initial_capacity(self, number_of_entries: usize) -> Self {
        Self {
            initial_capacity: Some(number_of_entries),
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CacheBuilder;

    #[test]
    fn build_cache() {
        // Cache<char, String>
        let mut cache = CacheBuilder::<char, String, _>::new(100).build();
        let policy = cache.policy();

        assert_eq!(policy.max_capacity(), Some(100));

        cache.insert('a', "Alice".to_string());
        assert_eq!(cache.get(&'a'), Some(&"Alice".to_string()));
    }
}
