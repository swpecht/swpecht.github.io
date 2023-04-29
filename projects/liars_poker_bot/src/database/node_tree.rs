use log::debug;
use rustc_hash::FxHashMap as HashMap;

use crate::{collections::ArrayVec, game::Action, istate::IStateKey};

const MAX_CHILDREN: usize = 32;

/// A performant datastructure for storing nodes in memory
pub struct Tree<T> {
    nodes: Vec<Node<T>>,
    /// the starting roots of the tree
    roots: HashMap<Action, usize>,
    /// a cursor for each root of the tree
    cursors: HashMap<Action, Cursor>,
    stats: TreeStats,
}

#[derive(Clone, Copy, Debug)]
pub struct TreeStats {
    pub get_calls: usize,
    pub nodes_touched: usize,
    pub naive_nodes_touched: usize,
}

impl TreeStats {
    fn new() -> Self {
        Self {
            get_calls: 0,
            nodes_touched: 0,
            naive_nodes_touched: 0,
        }
    }
}

struct Cursor {
    id: usize,
    path: ArrayVec<64>,
}

struct Node<T> {
    parent: usize,
    children: [usize; MAX_CHILDREN],
    action: usize,
    v: Option<T>,
}

impl<T> Node<T> {
    fn new(p: Action, a: Action, v: Option<T>) -> Self {
        Self {
            parent: p,
            children: [0; MAX_CHILDREN],
            action: a,
            v: v,
        }
    }
}

impl<T: Clone> Tree<T> {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            roots: HashMap::default(),
            cursors: HashMap::default(),
            stats: TreeStats::new(),
        }
    }

    pub fn insert(&mut self, k: IStateKey, v: T) {
        let ka = k.get_actions();

        let id = self.find_node(ka);
        let n = &mut self.nodes[id];
        n.v = Some(v);
    }

    fn get_or_create_root(&mut self, action: Action) -> usize {
        let root = self.roots.get(&action);
        if root.is_some() {
            return *root.unwrap();
        }

        let n = Node::new(0, action, None); // root node has itself as a parent
        let id = self.nodes.len();
        self.nodes.push(n);
        self.roots.insert(action, id);

        return id;
    }

    /// Return the index of the child node for a given actions, creating one if needed
    fn get_or_create_child(&mut self, parent: usize, action: Action) -> usize {
        let p = &self.nodes[parent];
        // let c = p.children.get(&action);
        let c = p.children[action];

        if c != 0 {
            return c;
        }

        let cn: Node<T> = Node::new(parent, action, None);
        let c = self.nodes.len();
        self.nodes.push(cn);

        let p = &mut self.nodes[parent];
        p.children[action] = c;

        return c;
    }

    /// Return the index of the node for a given ka. Creates nodes along the way as needed
    fn find_node(&mut self, ka: ArrayVec<64>) -> usize {
        let (mut cur_node, mut depth) = match self.find_cursor_ancestor(ka) {
            None => (self.get_or_create_root(ka[0]), 0),
            Some(x) => x,
        };

        // check if it's the cursor
        if depth == ka.len() - 1 {
            assert_eq!(ka[ka.len() - 1], self.nodes[cur_node].action);
            return cur_node;
        }

        loop {
            if depth + 1 > ka.len() - 1 {
                self.cursors.insert(
                    ka[0],
                    Cursor {
                        id: cur_node,
                        path: ka,
                    },
                );
                return cur_node;
            }

            let next_action = ka[depth + 1];
            let child = self.get_or_create_child(cur_node, next_action);

            self.stats.nodes_touched += 1;
            cur_node = child;
            depth += 1;
        }
    }

    /// Returns the (id, depth) of the nearest common ancestor to a node
    ///
    /// Because nodes are read "near" each other, this should be much faster than always traversing the tree
    fn find_cursor_ancestor(&self, ka: ArrayVec<64>) -> Option<(usize, usize)> {
        let cursor = self.cursors.get(&ka[0]);
        if cursor.is_none() {
            return None;
        }
        let cursor = cursor.unwrap();
        let ca = cursor.path;
        let last_same = find_last_same(&ka, &ca);
        if last_same.is_none() {
            return Some((cursor.id, ca.len() - 1));
        }
        let last_same = last_same.unwrap();

        let mut cur_node = cursor.id;
        for _ in 0..(ca.len() - last_same - 1) {
            // want to go one above the diff
            let n = &self.nodes[cur_node];
            let p = n.parent;
            cur_node = p;
        }

        return Some((cur_node, last_same));
    }

    /// Gets a clone of the value from the tree.
    pub fn get(&mut self, k: &IStateKey) -> Option<T> {
        self.stats.get_calls += 1;

        let root = self.roots.get(&k[0]);
        if root.is_none() {
            return None;
        }
        let actions = k.get_actions();
        self.stats.naive_nodes_touched += actions.len();
        let idx = self.find_node(actions);

        if self.stats.get_calls % 10_000_000 == 0 {
            debug!("nodestore stats: {:?}", self.stats);
        }

        return self.nodes[idx].v.clone();
    }

    pub fn contains_key(&mut self, k: &IStateKey) -> bool {
        return self.get(k).is_some();
    }

    pub fn get_stats(&self) -> TreeStats {
        return self.stats;
    }
}

/// finds the index of the last action in the same path
fn find_last_same<const N: usize>(ka: &ArrayVec<N>, ca: &ArrayVec<N>) -> Option<usize> {
    assert!(ka.len() != 0);
    assert!(ca.len() != 0);

    if ka[0] != ca[0] {
        return None;
    }

    let len = ka.len().min(ca.len());
    for i in 1..len {
        if ka[i] != ca[i] {
            return Some(i - 1);
        }
    }

    return Some(len - 1);
}

#[cfg(test)]
mod tests {

    use crate::{
        collections::ArrayVec, database::node_tree::find_last_same, game::euchre::Euchre,
        game::GameState, istate::IStateKey,
    };

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
        let v = t.get(&k1);
        assert_eq!(v, Some(1));
        t.insert(k1, v.unwrap());

        gs.apply_action(gs.legal_actions()[0]);
        let mut ogs = gs.clone();
        gs.apply_action(gs.legal_actions()[0]);
        let k2 = gs.istate_key(0);
        t.insert(k2, 2);
        let v = t.get(&k2);
        assert_eq!(v, Some(2));
        t.insert(k2, v.unwrap());

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

    #[test]
    fn test_node_tree_simple() {
        let mut t = Tree::new();
        let mut k1 = IStateKey::new();
        k1.push(0);
        k1.push(1);

        t.insert(k1.clone(), 1);

        assert_eq!(t.get(&k1), Some(1));
    }

    #[test]
    fn test_find_last_same() {
        let mut a = ArrayVec::<10>::new();
        a.push(1);

        let mut b = ArrayVec::new();
        b.push(1);

        let fd = find_last_same(&a, &b);
        assert_eq!(fd, Some(0));

        let mut c = ArrayVec::new();
        c.push(42);
        let fd = find_last_same(&a, &c);
        assert_eq!(fd, None);

        a.push(2);
        b.push(3);

        let fd = find_last_same(&a, &b);
        assert_eq!(fd.unwrap(), 0);

        a.push(2);
        let fd = find_last_same(&a, &b);
        assert_eq!(fd.unwrap(), 0);

        b.push(3);
        let fd = find_last_same(&a, &b);
        assert_eq!(fd.unwrap(), 0);

        let mut a = ArrayVec::<10>::new();
        a.push(0);
        a.push(1);
        a.push(2);
        let b = a.clone();
        a.push(3);
        let fd = find_last_same(&a, &b);
        assert_eq!(fd.unwrap(), 2);

        let fd = find_last_same(&b, &a);
        assert_eq!(fd.unwrap(), 2);
    }
}
