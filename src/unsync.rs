//! Provides a *not* thread-safe cache implementation built upon
//! [`hashbrown::HashMap`][hb-hashmap].
//!
//! [hb-hashmap]: https://docs.rs/hashbrown/latest/hashbrown/struct.HashMap.html

mod builder;
mod cache;
mod deques;
mod iter;

use std::ptr::NonNull;
use std::rc::Rc;

pub use builder::CacheBuilder;
pub use cache::Cache;
pub use iter::Iter;

use crate::common::deque::DeqNode;

pub(crate) struct KeyHashDate<K> {
    pub(crate) key: Rc<K>,
}

impl<K> KeyHashDate<K> {
    pub(crate) fn new(key: Rc<K>) -> Self {
        Self { key }
    }
}

type KeyDeqNodeAo<K> = NonNull<DeqNode<KeyHashDate<K>>>;

struct EntryInfo<K> {
    access_order_q_node: Option<KeyDeqNodeAo<K>>,
    visited: bool,
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
                visited: false,
            },
        }
    }

    #[inline]
    pub(crate) fn set_visited(&mut self, visited: bool) {
        self.info.visited = visited;
    }

    #[inline]
    pub(crate) fn visited(&self) -> bool {
        self.info.visited
    }

    #[inline]
    pub(crate) fn replace_deq_nodes_with(&mut self, mut other: Self) {
        self.info.access_order_q_node = other.info.access_order_q_node.take();
        self.info.visited = other.info.visited;
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
}
