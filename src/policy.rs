#[derive(Clone, Debug)]
/// The policy of a cache.
pub struct Policy {
    max_capacity: Option<u64>,
    admission_scan_limit: u32,
}

impl Policy {
    pub(crate) fn new(max_capacity: Option<u64>, admission_scan_limit: u32) -> Self {
        Self {
            max_capacity,
            admission_scan_limit,
        }
    }

    /// Returns the `max_capacity` of the cache.
    pub fn max_capacity(&self) -> Option<u64> {
        self.max_capacity
    }

    /// Returns the maximum number of entries inspected by a full-cache
    /// [`try_insert`](crate::unsync::Cache::try_insert) admission attempt.
    pub fn admission_scan_limit(&self) -> u32 {
        self.admission_scan_limit
    }
}
