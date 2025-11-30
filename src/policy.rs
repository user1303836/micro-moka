#[derive(Clone, Debug)]
/// The policy of a cache.
pub struct Policy {
    max_capacity: Option<u64>,
}

impl Policy {
    pub(crate) fn new(
        max_capacity: Option<u64>,
    ) -> Self {
        Self {
            max_capacity,
        }
    }

    /// Returns the `max_capacity` of the cache.
    pub fn max_capacity(&self) -> Option<u64> {
        self.max_capacity
    }
}
