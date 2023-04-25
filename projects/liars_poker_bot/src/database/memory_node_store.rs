use crate::istate::IStateKey;

use super::{node_tree::Tree, NodeStore};

pub struct MemoryNodeStore<T> {
    store: Tree<T>,
}

impl<T> MemoryNodeStore<T> {
    pub fn new() -> Self {
        Self { store: Tree::new() }
    }
}

impl<T> NodeStore<T> for MemoryNodeStore<T> {
    fn get_node_mut(&mut self, istate: &IStateKey) -> Option<T> {
        return self.store.get(istate);
    }

    fn insert_node(&mut self, istate: IStateKey, n: T) -> Option<T> {
        return self.store.insert(istate, n);
    }

    fn contains_node(&mut self, istate: &IStateKey) -> bool {
        return self.store.contains_key(istate);
    }
}
