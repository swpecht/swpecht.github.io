use std::{cell::RefCell, rc::Rc};

use crate::istate::IStateKey;

use super::{node_tree::Tree, NodeStore};

pub struct MemoryNodeStore<T> {
    store: Tree<Rc<RefCell<T>>>,
}

impl<T> MemoryNodeStore<T> {
    pub fn new() -> Self {
        Self { store: Tree::new() }
    }
}

impl<T> NodeStore<T> for MemoryNodeStore<T> {
    fn get_owned(&mut self, istate: &IStateKey) -> Option<Rc<RefCell<T>>> {
        return self.store.get_owned(istate);
    }

    fn insert_node(&mut self, istate: IStateKey, n: Rc<RefCell<T>>) {
        return self.store.insert(istate, n);
    }

    fn contains_node(&mut self, istate: &IStateKey) -> bool {
        return self.store.contains_key(istate);
    }
}
