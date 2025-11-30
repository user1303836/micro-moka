use super::{Cache, ValueEntry};

use std::{
    hash::Hash,
    rc::Rc,
};

type HashMapIter<'i, K, V> = std::collections::hash_map::Iter<'i, Rc<K>, ValueEntry<K, V>>;

pub struct Iter<'i, K, V> {
    iter: HashMapIter<'i, K, V>,
}

impl<'i, K, V> Iter<'i, K, V> {
    pub(crate) fn new(_cache: &'i Cache<K, V, impl std::hash::BuildHasher>, iter: HashMapIter<'i, K, V>) -> Self {
        Self { iter }
    }
}

impl<'i, K, V> Iterator for Iter<'i, K, V>
where
    K: Hash + Eq,
{
    type Item = (&'i K, &'i V);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some((k, entry)) = self.iter.next() {
            return Some((k, &entry.value));
        }
        None
    }
}
