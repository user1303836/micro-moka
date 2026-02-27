use super::{KeyHashDate, ValueEntry};
use crate::common::{
    deque::{DeqNode, Deque},
    CacheRegion,
};

use tagptr::TagNonNull;

pub(crate) struct Deques<K> {
    pub(crate) window: Deque<KeyHashDate<K>>, //    Not used yet.
    pub(crate) probation: Deque<KeyHashDate<K>>,
    pub(crate) protected: Deque<KeyHashDate<K>>, // Not used yet.
}

impl<K> Default for Deques<K> {
    fn default() -> Self {
        Self {
            window: Deque::new(CacheRegion::Window),
            probation: Deque::new(CacheRegion::MainProbation),
            protected: Deque::new(CacheRegion::MainProtected),
        }
    }
}

impl<K> Deques<K> {
    pub(crate) fn clear(&mut self) {
        self.window = Deque::new(CacheRegion::Window);
        self.probation = Deque::new(CacheRegion::MainProbation);
        self.protected = Deque::new(CacheRegion::MainProtected);
    }

    pub(crate) fn push_back_ao<V>(
        &mut self,
        region: CacheRegion,
        kh: KeyHashDate<K>,
        entry: &mut ValueEntry<K, V>,
    ) {
        let node = Box::new(DeqNode::new(kh));
        let node = match region {
            CacheRegion::Window => self.window.push_back(node),
            CacheRegion::MainProbation => self.probation.push_back(node),
            CacheRegion::MainProtected => self.protected.push_back(node),
            CacheRegion::Other => unreachable!(),
        };
        let tagged_node = TagNonNull::compose(node, region as usize);
        entry.set_access_order_q_node(Some(tagged_node));
    }

    pub(crate) fn move_to_back_ao<V>(&mut self, entry: &ValueEntry<K, V>) {
        if let Some(tagged_node) = entry.access_order_q_node() {
            let (node, tag) = tagged_node.decompose();
            match tag.into() {
                CacheRegion::Window => {
                    #[cfg(debug_assertions)]
                    {
                        let p = unsafe { node.as_ref() };
                        debug_assert!(self.window.contains(p));
                    }
                    unsafe { self.window.move_to_back(node) };
                }
                CacheRegion::MainProbation => {
                    #[cfg(debug_assertions)]
                    {
                        let p = unsafe { node.as_ref() };
                        debug_assert!(self.probation.contains(p));
                    }
                    unsafe { self.probation.move_to_back(node) };
                }
                CacheRegion::MainProtected => {
                    #[cfg(debug_assertions)]
                    {
                        let p = unsafe { node.as_ref() };
                        debug_assert!(self.protected.contains(p));
                    }
                    unsafe { self.protected.move_to_back(node) };
                }
                _ => unreachable!(),
            }
        }
    }

    pub(crate) fn unlink_ao<V>(&mut self, entry: &mut ValueEntry<K, V>) {
        if let Some(node) = entry.take_access_order_q_node() {
            self.unlink_node_ao(node);
        }
    }

    pub(crate) fn unlink_ao_from_deque<V>(
        deq_name: &str,
        deq: &mut Deque<KeyHashDate<K>>,
        entry: &mut ValueEntry<K, V>,
    ) {
        if let Some(node) = entry.take_access_order_q_node() {
            unsafe { Self::unlink_node_ao_from_deque(deq_name, deq, node) };
        }
    }

    pub(crate) fn unlink_node_ao(&mut self, tagged_node: TagNonNull<DeqNode<KeyHashDate<K>>, 2>) {
        unsafe {
            match tagged_node.decompose_tag().into() {
                CacheRegion::Window => {
                    Self::unlink_node_ao_from_deque("window", &mut self.window, tagged_node)
                }
                CacheRegion::MainProbation => {
                    Self::unlink_node_ao_from_deque("probation", &mut self.probation, tagged_node)
                }
                CacheRegion::MainProtected => {
                    Self::unlink_node_ao_from_deque("protected", &mut self.protected, tagged_node)
                }
                _ => unreachable!(),
            }
        }
    }

    unsafe fn unlink_node_ao_from_deque(
        deq_name: &str,
        deq: &mut Deque<KeyHashDate<K>>,
        tagged_node: TagNonNull<DeqNode<KeyHashDate<K>>, 2>,
    ) {
        let (node, tag) = tagged_node.decompose();
        if deq.region() != tag {
            panic!(
                "unlink_node - node is not a member of {} deque. {:?}",
                deq_name,
                node.as_ref()
            )
        }

        #[cfg(debug_assertions)]
        {
            if !deq.contains(node.as_ref()) {
                panic!(
                    "unlink_node - node is not a member of {} deque. {:?}",
                    deq_name,
                    node.as_ref()
                )
            }
        }

        // https://github.com/moka-rs/moka/issues/64
        deq.unlink_and_drop(node);
    }
}
