//! Provides a *not* thread-safe cache implementation built upon
//! [`std::collections::HashMap`][std-hashmap].
//!
//! [std-hashmap]: https://doc.rust-lang.org/std/collections/struct.HashMap.html

mod builder;
mod cache;
mod deques;
mod iter;

use std::rc::Rc;
use tagptr::TagNonNull;

pub use builder::CacheBuilder;
pub use cache::Cache;
pub use iter::Iter;

use crate::common::deque::DeqNode;

pub(crate) struct KeyHashDate<K> {
    pub(crate) key: Rc<K>,
    pub(crate) hash: u64,
}

impl<K> KeyHashDate<K> {
    pub(crate) fn new(key: Rc<K>, hash: u64) -> Self {
        Self { key, hash }
    }
}

// DeqNode for an access order queue.
type KeyDeqNodeAo<K> = TagNonNull<DeqNode<KeyHashDate<K>>, 2>;

struct EntryInfo<K> {
    access_order_q_node: Option<KeyDeqNodeAo<K>>,
}

pub(crate) struct ValueEntry<K, V> {
    pub(crate) value: V,
    info: EntryInfo<K>,
}

impl<K, V> ValueEntry<K, V> {
    pub(crate) fn new(value: V) -> Self {
        Self {
            value,
            info: EntryInfo {
                access_order_q_node: None,
            },
        }
    }

    #[inline]
    pub(crate) fn replace_deq_nodes_with(&mut self, mut other: Self) {
        self.info.access_order_q_node = other.info.access_order_q_node.take();
    }

    #[inline]
    pub(crate) fn access_order_q_node(&self) -> Option<KeyDeqNodeAo<K>> {
        self.info.access_order_q_node
    }

    #[inline]
    pub(crate) fn set_access_order_q_node(&mut self, node: Option<KeyDeqNodeAo<K>>) {
        self.info.access_order_q_node = node;
    }

    #[inline]
    pub(crate) fn take_access_order_q_node(&mut self) -> Option<KeyDeqNodeAo<K>> {
        self.info.access_order_q_node.take()
    }

    #[inline]
    pub(crate) fn policy_weight(&self) -> u32 {
        1
    }

    #[inline]
    pub(crate) fn set_policy_weight(&mut self, _policy_weight: u32) {
        // No-op
    }
}
