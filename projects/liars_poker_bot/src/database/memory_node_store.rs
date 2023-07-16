use std::{cell::RefCell, rc::Rc};

use crate::{
    actions,
    cfragent::cfrnode::{ActionVec, CFRNode},
    game::GameState,
    istate::IStateKey,
    policy::Policy,
};

use super::{node_tree::Tree, NodeStore};

pub struct MemoryNodeStore<T> {
    store: Tree<Rc<RefCell<T>>>,
}

impl<T> MemoryNodeStore<T> {
    /// Gets a new connection to the nodestore
    pub fn get_connection(&mut self) {
        // let (tx, rx) = mpsc::sync_channel(0);
    }
}

impl<T> Default for MemoryNodeStore<T> {
    fn default() -> Self {
        Self {
            store: Tree::default(),
        }
    }
}

impl<T> NodeStore<T> for MemoryNodeStore<T> {
    fn get(&mut self, istate: &IStateKey) -> Option<Rc<RefCell<T>>> {
        self.store.get(istate)
    }

    fn insert_node(&mut self, istate: IStateKey, n: Rc<RefCell<T>>) {
        self.store.insert(istate, n)
    }

    fn contains_node(&mut self, istate: &IStateKey) -> bool {
        self.store.contains_key(istate)
    }
}

impl<G: GameState> Policy<G> for MemoryNodeStore<CFRNode> {
    fn action_probabilities(&mut self, gs: &G) -> crate::cfragent::cfrnode::ActionVec<f64> {
        let p = gs.cur_player();
        let key = gs.istate_key(p);

        if self.contains_node(&key) {
            let node = self.get(&key).unwrap();
            let probs = node.borrow().get_average_strategy();
            return probs;
        }

        // otherwise return a uniform random strategy
        let actions = actions!(gs);
        let prob = 1.0 / actions.len() as f64;
        let mut probs = ActionVec::new(&actions);

        for a in actions {
            probs[a] = prob;
        }

        probs
    }
}
