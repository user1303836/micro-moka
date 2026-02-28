use super::{KeyHashDate, ValueEntry};
use crate::common::deque::{DeqNode, Deque};

use std::ptr::NonNull;

pub(crate) struct Deques<K> {
    pub(crate) deque: Deque<KeyHashDate<K>>,
}

impl<K> Default for Deques<K> {
    fn default() -> Self {
        Self {
            deque: Deque::new(),
        }
    }
}

impl<K> Deques<K> {
    pub(crate) fn clear(&mut self) {
        self.deque = Deque::new();
    }

    pub(crate) fn push_back_ao<V>(&mut self, kh: KeyHashDate<K>, entry: &mut ValueEntry<K, V>) {
        let node = Box::new(DeqNode::new(kh));
        let node = self.deque.push_back(node);
        entry.set_access_order_q_node(Some(node));
    }

    pub(crate) fn unlink_ao<V>(&mut self, entry: &mut ValueEntry<K, V>) {
        if let Some(node) = entry.take_access_order_q_node() {
            self.unlink_node_ao(node);
        }
    }

    pub(crate) fn unlink_ao_from_deque<V>(
        deq: &mut Deque<KeyHashDate<K>>,
        entry: &mut ValueEntry<K, V>,
    ) {
        if let Some(node) = entry.take_access_order_q_node() {
            unsafe { Self::unlink_node(deq, node) };
        }
    }

    pub(crate) fn unlink_node_ao(&mut self, node: NonNull<DeqNode<KeyHashDate<K>>>) {
        unsafe { Self::unlink_node(&mut self.deque, node) };
    }

    unsafe fn unlink_node(deq: &mut Deque<KeyHashDate<K>>, node: NonNull<DeqNode<KeyHashDate<K>>>) {
        deq.unlink_and_drop(node);
    }
}
