// use std::sync::{
//     atomic::{AtomicPtr, Ordering},
//     Arc,
// };

// struct CNode<K, V> {
//     mask: u32,
//     children: Vec<Branch<K, V>>,
// }

// impl<K, V> Default for CNode<K, V> {
//     fn default() -> Self {
//         Self {
//             mask: 0,
//             children: Vec::new(),
//         }
//     }
// }

// #[derive(Clone)]
// struct INode<K, V>(Arc<AtomicPtr<CNode<K, V>>>);

// enum Branch<K, V> {
//     I(INode<K, V>),
//     SNode { k: K, v: V },
// }

// impl<K: Clone, V: Clone> Clone for Branch<K, V> {
//     fn clone(&self) -> Self {
//         match self {
//             Self::I(arg0) => Self::I(arg0.clone()),
//             Self::SNode { k, v } => Self::SNode {
//                 k: k.clone(),
//                 v: v.clone(),
//             },
//         }
//     }
// }

// pub struct CTrie<K, V> {
//     root: INode<K, V>,
// }

// impl<K: Clone, V: Clone> CTrie<K, V> {
//     pub fn insert(&mut self, k: K, v: V) {
//         loop {
//             let current = self.root.0.load(Ordering::SeqCst);
//             unsafe {
//                 let mut new_children = current.as_ref().unwrap().children.clone();
//                 new_children.push(v);
//                 let new = INode(Arc::new(AtomicPtr::new(&mut CNode {
//                     mask: 0,
//                     children: new_children,
//                 })));
//                 if let Ok(_) =
//                     self.root
//                         .0
//                         .compare_exchange(current, new, Ordering::SeqCst, Ordering::Relaxed)
//                 {
//                     break;
//                 }
//             }
//         }
//     }
// }

// impl<K, V> Default for CTrie<K, V> {
//     fn default() -> Self {
//         Self {
//             root: INode {
//                 child: AtomicPtr::new(&mut CNode::default()),
//             },
//         }
//     }
// }

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn test_ctrie_basic() {
//         let mut trie: CTrie<usize, usize> = CTrie::default();
//     }
// }
