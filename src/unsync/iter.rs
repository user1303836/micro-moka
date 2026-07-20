use super::SlabEntry;
use std::iter::FusedIterator;

pub struct Iter<'i, K, V> {
    inner: std::slice::Iter<'i, Option<SlabEntry<K, V>>>,
    remaining: usize,
}

impl<'i, K, V> Iter<'i, K, V> {
    pub(crate) fn new(entries: &'i [Option<SlabEntry<K, V>>], remaining: usize) -> Self {
        Self {
            inner: entries.iter(),
            remaining,
        }
    }
}

impl<'i, K, V> Iterator for Iter<'i, K, V> {
    type Item = (&'i K, &'i V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.inner.next() {
                Some(Some(entry)) => {
                    self.remaining -= 1;
                    return Some((&entry.key, &entry.value));
                }
                Some(None) => continue,
                None => return None,
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<K, V> ExactSizeIterator for Iter<'_, K, V> {}
impl<K, V> FusedIterator for Iter<'_, K, V> {}
