use std::collections::HashMap;

use crate::{cfragent::CFRNode, istate::IStateKey};

use super::NodeStore;

pub struct MemoryNodeStore {
    store: HashMap<IStateKey, CFRNode>,
}

impl MemoryNodeStore {
    pub fn new() -> Self {
        Self {
            store: HashMap::new(),
        }
    }
}

impl NodeStore for MemoryNodeStore {
    fn get_node_mut(&mut self, istate: &IStateKey) -> Option<CFRNode> {
        return self.store.get_mut(istate).cloned();
    }

    fn insert_node(&mut self, istate: IStateKey, n: CFRNode) -> Option<CFRNode> {
        return self.store.insert(istate, n);
    }

    fn contains_node(&mut self, istate: &IStateKey) -> bool {
        return self.store.contains_key(istate);
    }
}
