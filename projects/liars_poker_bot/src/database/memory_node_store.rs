use crate::{cfragent::CFRNode, istate::IStateKey};

use super::{node_tree::Tree, NodeStore};

pub struct MemoryNodeStore {
    store: Tree<CFRNode>,
}

impl MemoryNodeStore {
    pub fn new() -> Self {
        Self { store: Tree::new() }
    }
}

impl NodeStore for MemoryNodeStore {
    fn get_node_mut(&mut self, istate: &IStateKey) -> Option<CFRNode> {
        return self.store.get(istate);
    }

    fn insert_node(&mut self, istate: IStateKey, n: CFRNode) -> Option<CFRNode> {
        return self.store.insert(istate, n);
    }

    fn contains_node(&mut self, istate: &IStateKey) -> bool {
        return self.store.contains_key(istate);
    }
}
