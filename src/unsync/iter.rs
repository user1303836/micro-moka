use super::SlabEntry;

pub struct Iter<'i, K, V> {
    inner: std::slice::Iter<'i, Option<SlabEntry<K, V>>>,
}

impl<'i, K, V> Iter<'i, K, V> {
    pub(crate) fn new(entries: &'i [Option<SlabEntry<K, V>>]) -> Self {
        Self {
            inner: entries.iter(),
        }
    }
}

impl<'i, K, V> Iterator for Iter<'i, K, V> {
    type Item = (&'i K, &'i V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.inner.next() {
                Some(Some(entry)) => return Some((&entry.key, &entry.value)),
                Some(None) => continue,
                None => return None,
            }
        }
    }
}
