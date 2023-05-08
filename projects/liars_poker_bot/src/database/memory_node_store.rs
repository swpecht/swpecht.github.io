use std::{cell::RefCell, rc::Rc};

use crate::{
    actions, cfragent::cfrnode::CFRNode, game::GameState, istate::IStateKey, policy::Policy,
};

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
    fn get(&mut self, istate: &IStateKey) -> Option<Rc<RefCell<T>>> {
        return self.store.get(istate);
    }

    fn insert_node(&mut self, istate: IStateKey, n: Rc<RefCell<T>>) {
        return self.store.insert(istate, n);
    }

    fn contains_node(&mut self, istate: &IStateKey) -> bool {
        return self.store.contains_key(istate);
    }
}

impl<G: GameState> Policy<G> for MemoryNodeStore<CFRNode> {
    fn action_probabilities(&mut self, gs: &G) -> crate::cfragent::cfrnode::ActionVec<f64> {
        let p = gs.cur_player();
        let actions = actions!(gs);
        let node = self
            .get(&gs.istate_key(p))
            .unwrap_or(Rc::new(RefCell::new(CFRNode::new(actions))));
        let probs = node.borrow().get_average_strategy();

        return probs;
    }
}
