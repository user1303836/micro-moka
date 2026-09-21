#![allow(dead_code)]

use std::borrow::Borrow;
use std::hash::{BuildHasher, Hash};
use std::num::NonZeroUsize;

pub trait Key: Eq + Hash + Clone + Borrow<Self::Query> {
    type Query: Eq + Hash + ToOwned<Owned = Self> + ?Sized;
    fn make(number: usize, width: usize) -> Self;
}

impl Key for u64 {
    type Query = u64;
    fn make(number: usize, _: usize) -> Self {
        number as u64
    }
}

impl Key for String {
    type Query = str;
    fn make(number: usize, width: usize) -> Self {
        format!("{number:0width$}")
    }
}

pub trait Cache<K: Key, S: BuildHasher>: Sized {
    const NAME: &'static str;
    fn new(capacity: usize, hasher: S) -> Self;
    fn get(&mut self, key: &K) -> Option<u64>;
    fn insert(&mut self, key: K, value: u64);
    fn load(&mut self, key: &K) -> u64;
    fn remove(&mut self, key: &K);
    fn clear(&mut self);
    fn sum(&self) -> u64;
    fn count(&self) -> usize;
}

pub struct Current<K, S>(micro_moka::unsync::Cache<K, u64, S>);
pub struct Baseline<K, S>(baseline::unsync::Cache<K, u64, S>);
pub struct Quick<K, S>(quick_cache::unsync::Cache<K, u64, quick_cache::UnitWeighter, S>);
pub struct Lru<K, S>(lru::LruCache<K, u64, S>);
pub struct Hashlink<K, S>(hashlink::LruCache<K, u64, S>);

macro_rules! micro_adapter {
    ($wrapper:ident, $module:ident, $name:literal) => {
        impl<K: Key, S: BuildHasher> Cache<K, S> for $wrapper<K, S> {
            const NAME: &'static str = $name;
            fn new(capacity: usize, hasher: S) -> Self {
                Self(
                    $module::unsync::Cache::builder()
                        .max_capacity(capacity as u64)
                        .initial_capacity(capacity)
                        .build_with_hasher(hasher),
                )
            }
            fn get(&mut self, key: &K) -> Option<u64> {
                self.0.get::<K::Query>(key.borrow()).copied()
            }
            fn insert(&mut self, key: K, value: u64) {
                self.0.insert(key, value);
            }
            fn load(&mut self, key: &K) -> u64 {
                *self.0.get_or_insert_with(key.clone(), || 7)
            }
            fn remove(&mut self, key: &K) {
                self.0.remove::<K::Query>(key.borrow());
            }
            fn clear(&mut self) {
                self.0.invalidate_all();
            }
            fn sum(&self) -> u64 {
                self.0.iter().fold(0u64, |sum, (_, v)| sum.wrapping_add(*v))
            }
            fn count(&self) -> usize {
                self.0.iter().count()
            }
        }
    };
}
micro_adapter!(Current, micro_moka, "micro");
micro_adapter!(Baseline, baseline, "baseline");

impl<K: Key, S: BuildHasher> Cache<K, S> for Quick<K, S> {
    const NAME: &'static str = "quick_cache";
    fn new(capacity: usize, hasher: S) -> Self {
        Self(quick_cache::unsync::Cache::with(
            capacity,
            capacity as u64,
            quick_cache::UnitWeighter,
            hasher,
            Default::default(),
        ))
    }
    fn get(&mut self, key: &K) -> Option<u64> {
        self.0.get::<K::Query>(key.borrow()).copied()
    }
    fn insert(&mut self, key: K, value: u64) {
        self.0.insert(key, value);
    }
    fn load(&mut self, key: &K) -> u64 {
        *self
            .0
            .get_or_insert_with::<K::Query, ()>(key.borrow(), || Ok(7))
            .unwrap()
            .unwrap()
    }
    fn remove(&mut self, key: &K) {
        self.0.remove::<K::Query>(key.borrow());
    }
    fn clear(&mut self) {
        self.0.clear();
    }
    fn sum(&self) -> u64 {
        self.0.iter().fold(0u64, |sum, (_, v)| sum.wrapping_add(*v))
    }
    fn count(&self) -> usize {
        self.0.iter().count()
    }
}

impl<K: Key, S: BuildHasher> Cache<K, S> for Lru<K, S> {
    const NAME: &'static str = "lru";
    fn new(capacity: usize, hasher: S) -> Self {
        Self(lru::LruCache::with_hasher(
            NonZeroUsize::new(capacity).unwrap(),
            hasher,
        ))
    }
    fn get(&mut self, key: &K) -> Option<u64> {
        self.0.get::<K::Query>(key.borrow()).copied()
    }
    fn insert(&mut self, key: K, value: u64) {
        self.0.put(key, value);
    }
    fn load(&mut self, key: &K) -> u64 {
        *self.0.get_or_insert_ref::<K::Query, _>(key.borrow(), || 7)
    }
    fn remove(&mut self, key: &K) {
        self.0.pop::<K::Query>(key.borrow());
    }
    fn clear(&mut self) {
        self.0.clear();
    }
    fn sum(&self) -> u64 {
        self.0.iter().fold(0u64, |sum, (_, v)| sum.wrapping_add(*v))
    }
    fn count(&self) -> usize {
        self.0.iter().count()
    }
}

impl<K: Key, S: BuildHasher> Cache<K, S> for Hashlink<K, S> {
    const NAME: &'static str = "hashlink";
    fn new(capacity: usize, hasher: S) -> Self {
        Self(hashlink::LruCache::with_hasher(capacity, hasher))
    }
    fn get(&mut self, key: &K) -> Option<u64> {
        self.0.get::<K::Query>(key.borrow()).copied()
    }
    fn insert(&mut self, key: K, value: u64) {
        self.0.insert(key, value);
    }
    fn load(&mut self, key: &K) -> u64 {
        if let Some(&v) = self.0.get::<K::Query>(key.borrow()) {
            v
        } else {
            self.0.insert(key.clone(), 7);
            7
        }
    }
    fn remove(&mut self, key: &K) {
        self.0.remove::<K::Query>(key.borrow());
    }
    fn clear(&mut self) {
        self.0.clear();
    }
    fn sum(&self) -> u64 {
        self.0.iter().fold(0u64, |sum, (_, v)| sum.wrapping_add(*v))
    }
    fn count(&self) -> usize {
        self.0.iter().count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::hash_map::RandomState;

    fn check<C: Cache<String, RandomState>>() {
        let mut c = C::new(8, RandomState::new());
        for i in 0..8 {
            c.insert(i.to_string(), i);
        }
        assert_eq!(c.count(), 8);
        assert_eq!(c.sum(), 28);
        assert_eq!(c.get(&"2".into()), Some(2));
        assert_eq!(c.get(&"absent".into()), None);
        assert_eq!(c.load(&"2".into()), 2);
        c.remove(&"1".into());
        assert_eq!(c.count(), 7);
        assert_eq!(c.load(&"new".into()), 7);
        assert_eq!(c.get(&"new".into()), Some(7));
        c.clear();
        assert_eq!(c.count(), 0);
        assert_eq!(c.sum(), 0);
        c.insert("again".into(), 99);
        assert_eq!(c.load(&"again".into()), 99);
    }

    #[test]
    fn adapters_have_equivalent_observable_operations() {
        check::<Current<_, _>>();
        check::<Baseline<_, _>>();
        check::<Quick<_, _>>();
        check::<Lru<_, _>>();
        check::<Hashlink<_, _>>();
    }
}
