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

    fn count(self) -> usize {
        self.remaining
    }
}

impl<K, V> ExactSizeIterator for Iter<'_, K, V> {}
impl<K, V> FusedIterator for Iter<'_, K, V> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_iterator_is_fused_and_counts_zero() {
        let entries: [Option<SlabEntry<u64, u64>>; 3] = [None, None, None];
        let mut iter = Iter::new(&entries, 0);
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next(), None);
        assert_eq!(iter.count(), 0);
    }

    #[test]
    fn count_and_size_hint_follow_partial_sparse_consumption() {
        let entries = [
            None,
            Some(SlabEntry::new(1, 2, 0)),
            None,
            Some(SlabEntry::new(3, 4, 0)),
            None,
        ];
        let mut iter = Iter::new(&entries, 2);
        assert_eq!(iter.size_hint(), (2, Some(2)));
        assert_eq!(iter.next(), Some((&1, &2)));
        assert_eq!(iter.size_hint(), (1, Some(1)));
        assert_eq!(iter.count(), 1);
        assert_eq!(Iter::new(&entries, 2).count(), 2);
    }

    #[test]
    fn partial_folding_and_short_circuiting_preserve_remaining_items() {
        let entries = [
            Some(SlabEntry::new(1, 2, 0)),
            None,
            Some(SlabEntry::new(3, 4, 0)),
        ];
        let mut iter = Iter::new(&entries, 2);
        assert_eq!(iter.try_fold(0, |_, (_, v)| Err::<i32, _>(*v)), Err(2));
        assert_eq!(iter.len(), 1);
        assert_eq!(iter.fold(0, |acc, (_, v)| acc + v), 4);
        let mut iter = Iter::new(&entries, 2);
        assert_eq!(iter.next(), Some((&1, &2)));
        assert_eq!(iter.next(), Some((&3, &4)));
        assert_eq!(iter.len(), 0);
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next(), None);
    }
}
