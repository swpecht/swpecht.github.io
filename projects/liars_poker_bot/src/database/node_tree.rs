use std::collections::HashMap;

use crate::{
    game::{arrayvec::ArrayVec, Action},
    istate::IStateKey,
};

/// A performant datastructure for storing nodes in memory
pub struct Tree<T: Copy> {
    nodes: Vec<Node<T>>,
    /// the starting roots of the tree
    roots: HashMap<Action, usize>,
}

struct Node<T> {
    children: HashMap<Action, usize>,
    action: Action,
    v: Option<T>,
}

impl<T> Node<T> {
    fn new(a: Action, v: Option<T>) -> Self {
        Self {
            children: HashMap::new(),
            action: a,
            v: v,
        }
    }
}

impl<T: Copy> Tree<T> {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            roots: HashMap::new(),
        }
    }

    pub fn insert(&mut self, k: IStateKey, v: T) -> Option<T> {
        let ka = k.get_actions();

        let root = self.get_or_create_root(ka[0]);
        let id = self.get_node(ka, root);
        let n = &mut self.nodes[id];
        n.v = Some(v);

        return None;
    }

    fn get_or_create_root(&mut self, action: Action) -> usize {
        let root = self.roots.get(&action);
        if root.is_some() {
            return *root.unwrap();
        }

        let n = Node::new(action, None);
        let id = self.nodes.len();
        self.nodes.push(n);
        self.roots.insert(action, id);

        return id;
    }

    /// Return the index of the node that would be the parent of the ka. Creates nodes along the way as needed;
    fn get_node(&mut self, ka: ArrayVec<64>, root: usize) -> usize {
        let mut depth = 0;
        let a = ka[depth];
        let mut idx = root;
        if a != self.nodes[idx].action {
            panic!("trying to insert a node that isn't a child of the root node");
        }

        loop {
            let next_action = ka[depth + 1];

            let mut child;
            {
                let n = &mut self.nodes[idx];
                child = n.children.get(&next_action).copied();
            }
            if child.is_none() {
                let cn: Node<T> = Node {
                    children: HashMap::new(),
                    action: next_action,
                    v: None,
                };

                child = Some(self.nodes.len());
                self.nodes.push(cn);
                let n = &mut self.nodes[idx];
                n.children.insert(next_action, child.unwrap());
            }

            if depth + 1 == ka.len() {
                return child.unwrap();
            }

            idx = child.unwrap();
            depth += 1;
        }
    }

    pub fn get(&self, k: &IStateKey) -> Option<T> {
        let idx = self.roots.get(&k[0]);
        if idx.is_none() {
            return None;
        }
        let mut idx = *idx.unwrap();
        let ka = k.get_actions();
        let mut depth = 0;

        loop {
            let next_action = ka[depth + 1];

            let n = &self.nodes[idx];
            let child = n.children.get(&next_action).copied();

            if child.is_none() {
                return None;
            }

            if depth + 1 == ka.len() {
                idx = child.unwrap();
                break;
            }

            idx = child.unwrap();
            depth += 1;
        }

        return self.nodes[idx].v;
    }

    pub fn contains_key(&self, k: &IStateKey) -> bool {
        return self.get(k).is_some();
    }
}

#[cfg(test)]
mod tests {

    use crate::{euchre::Euchre, game::GameState};

    use super::Tree;

    #[test]
    fn test_node_tree() {
        let mut t = Tree::new();
        let mut gs = (Euchre::game().new)();
        while gs.is_chance_node() {
            let a = gs.legal_actions()[0];
            gs.apply_action(a);
        }

        assert_eq!(t.get(&gs.istate_key(0)), None);

        gs.apply_action(gs.legal_actions()[0]);
        let k1 = gs.istate_key(0);
        t.insert(k1.clone(), 1);
        gs.apply_action(gs.legal_actions()[0]);
        let mut ogs = gs.clone();
        gs.apply_action(gs.legal_actions()[0]);
        let k2 = gs.istate_key(0);
        t.insert(k2, 2);

        ogs.apply_action(ogs.legal_actions()[1]);
        let k3 = ogs.istate_key(0);
        t.insert(k3, 3);

        assert_eq!(t.get(&k1), Some(1));
        assert_eq!(t.get(&k2), Some(2));
        assert_eq!(t.get(&k3), Some(3));

        let k4 = gs.istate_key(1); // differnt player
        assert_eq!(t.get(&k4), None);

        t.insert(k1.clone(), 11);
        assert_eq!(t.get(&k1), Some(11));

        t.insert(k2.clone(), 12);
        assert_eq!(t.get(&k2), Some(12));
    }
}
