use log::debug;
use rustc_hash::FxHashMap;

use crate::{game::Action, istate::IStateKey};

/// A performant datastructure for storing nodes in memory
#[derive(Clone)]
pub struct Tree<T: Clone> {
    nodes: Vec<Node<T>>,
    /// the starting roots of the tree
    roots: FxHashMap<Action, usize>,
    /// a cursor for each root of the tree
    cursors: FxHashMap<Action, Cursor>,
    stats: TreeStats,
    root_value: Option<T>,
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

#[derive(Clone)]
struct Cursor {
    id: usize,
    path: IStateKey,
}

/// Tree structure for looking up values by information state.
///
/// Exploring imlementing storing the private information state data at the end of the node, this would enable us to have the policy looksups for
/// best response be close together. But this would make it difficult to use for cfr -- since we might need intermediate nodes
#[derive(Clone)]
struct Node<T> {
    parent: usize,
    children: FxHashMap<Action, usize>,
    action: Action,
    v: Option<T>,
}

impl<T> Node<T> {
    fn new(p: usize, a: Action, v: Option<T>) -> Self {
        Self {
            parent: p,
            children: FxHashMap::default(),
            action: a,
            v,
        }
    }
}

impl<T: Clone> Default for Tree<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone> Tree<T> {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            roots: FxHashMap::default(),
            cursors: FxHashMap::default(),
            stats: TreeStats::new(),
            root_value: None,
        }
    }

    pub fn insert(&mut self, k: IStateKey, v: T) {
        if k.is_empty() {
            self.root_value = Some(v);
            return;
        }

        let id = self.find_node(&k);
        let n = &mut self.nodes[id];
        n.v = Some(v);
    }

    fn get_or_create_root(&mut self, action: Action) -> usize {
        let root = self.roots.get(&action);
        if let Some(r) = root {
            return *r;
        }

        let n = Node::new(0, action, None); // root node has itself as a parent
        let id = self.nodes.len();
        self.nodes.push(n);
        self.roots.insert(action, id);

        id
    }

    /// Return the index of the child node for a given actions, creating one if needed
    fn get_or_create_child(&mut self, parent: usize, action: Action) -> usize {
        let p = &self.nodes[parent];
        // let c = p.children.get(&action);
        let c = p.children.get(&action);

        if let Some(c) = c {
            return *c;
        }

        let cn: Node<T> = Node::new(parent, action, None);
        let c = self.nodes.len();
        self.nodes.push(cn);

        let p = &mut self.nodes[parent];
        p.children.insert(action, c);

        c
    }

    /// Return the index of the node for a given ka. Creates nodes along the way as needed
    fn find_node(&mut self, k: &IStateKey) -> usize {
        let (mut cur_node, mut depth) = match self.find_cursor_ancestor(k) {
            None => (self.get_or_create_root(k[0]), 0),
            Some(x) => x,
        };

        // check if it's the cursor
        if depth == k.len() - 1 {
            assert_eq!(k[k.len() - 1], self.nodes[cur_node].action);
            return cur_node;
        }

        loop {
            if depth + 1 > k.len() - 1 {
                self.cursors.insert(
                    k[0],
                    Cursor {
                        id: cur_node,
                        path: *k,
                    },
                );
                return cur_node;
            }

            let next_action = k[depth + 1];
            let child = self.get_or_create_child(cur_node, next_action);

            self.stats.nodes_touched += 1;
            cur_node = child;
            depth += 1;
        }
    }

    /// Returns the (id, depth) of the nearest common ancestor to a node
    ///
    /// Because nodes are read "near" each other, this should be much faster than always traversing the tree
    fn find_cursor_ancestor(&self, k: &IStateKey) -> Option<(usize, usize)> {
        let cursor = self.cursors.get(&k[0]);
        cursor?;
        let cursor = cursor.unwrap();
        let c = cursor.path;
        let last_same = find_last_same(k, &c);
        if last_same.is_none() {
            return Some((cursor.id, c.len() - 1));
        }
        let last_same = last_same.unwrap();

        let mut cur_node = cursor.id;
        for _ in 0..(c.len() - last_same - 1) {
            // want to go one above the diff
            let n = &self.nodes[cur_node];
            let p = n.parent;
            cur_node = p;
        }

        Some((cur_node, last_same))
    }

    /// Gets a clone of the value from the tree.
    pub fn get(&mut self, k: &IStateKey) -> Option<T> {
        self.stats.get_calls += 1;

        if k.is_empty() {
            return self.root_value.clone();
        }

        let root = self.roots.get(&k[0]);
        root?;
        self.stats.naive_nodes_touched += k.len();
        let idx = self.find_node(k);

        if self.stats.get_calls % 10_000_000 == 0 {
            debug!("nodestore stats: {:?}", self.stats);
        }

        self.nodes[idx].v.clone()
    }

    pub fn contains_key(&mut self, k: &IStateKey) -> bool {
        self.get(k).is_some()
    }

    pub fn get_stats(&self) -> TreeStats {
        self.stats
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.len() == 0
    }
}

/// finds the index of the last action in the same path
fn find_last_same(k: &IStateKey, c: &IStateKey) -> Option<usize> {
    assert!(!k.is_empty());
    assert!(!c.is_empty());

    if k[0] != c[0] {
        return None;
    }

    let len = k.len().min(c.len());
    for i in 1..len {
        if k[i] != c[i] {
            return Some(i - 1);
        }
    }

    Some(len - 1)
}

#[cfg(test)]
mod tests {

    use crate::{
        actions,
        database::node_tree::find_last_same,
        game::euchre::Euchre,
        game::{Action, GameState},
        istate::IStateKey,
    };

    use super::Tree;

    #[test]
    fn test_node_tree() {
        let mut t = Tree::new();
        let mut gs = (Euchre::game().new)();
        while gs.is_chance_node() {
            let a = actions!(gs)[0];
            gs.apply_action(a);
        }

        assert_eq!(t.get(&gs.istate_key(0)), None);

        gs.apply_action(actions!(gs)[0]);
        let k1 = gs.istate_key(0);
        t.insert(k1, 1);
        let v = t.get(&k1);
        assert_eq!(v, Some(1));
        t.insert(k1, v.unwrap());

        gs.apply_action(actions!(gs)[0]);
        let mut ogs = gs.clone();
        gs.apply_action(actions!(gs)[0]);
        let k2 = gs.istate_key(0);
        t.insert(k2, 2);
        let v = t.get(&k2);
        assert_eq!(v, Some(2));
        t.insert(k2, v.unwrap());

        ogs.apply_action(actions!(ogs)[1]);
        let k3 = ogs.istate_key(0);
        t.insert(k3, 3);

        assert_eq!(t.get(&k1), Some(1));
        assert_eq!(t.get(&k2), Some(2));
        assert_eq!(t.get(&k3), Some(3));

        let k4 = gs.istate_key(1); // differnt player
        assert_eq!(t.get(&k4), None);

        t.insert(k1, 11);
        assert_eq!(t.get(&k1), Some(11));

        t.insert(k2, 12);
        assert_eq!(t.get(&k2), Some(12));
    }

    #[test]
    fn test_node_tree_simple() {
        let mut t = Tree::new();
        let mut k1 = IStateKey::default();
        k1.push(Action(0));
        k1.push(Action(1));

        t.insert(k1, 1);

        assert_eq!(t.get(&k1), Some(1));
    }

    #[test]
    fn test_node_tree_empty_key() {
        let mut t = Tree::new();
        let k1 = IStateKey::default();

        assert!(!t.contains_key(&k1));

        t.insert(k1, 1);

        assert_eq!(t.get(&k1), Some(1));
    }

    #[test]
    fn test_find_last_same() {
        let mut a = IStateKey::default();
        a.push(Action(1));

        let mut b = IStateKey::default();
        b.push(Action(1));

        let fd = find_last_same(&a, &b);
        assert_eq!(fd, Some(0));

        let mut c = IStateKey::default();
        c.push(Action(42));
        let fd = find_last_same(&a, &c);
        assert_eq!(fd, None);

        a.push(Action(2));
        b.push(Action(3));

        let fd = find_last_same(&a, &b);
        assert_eq!(fd.unwrap(), 0);

        a.push(Action(2));
        let fd = find_last_same(&a, &b);
        assert_eq!(fd.unwrap(), 0);

        b.push(Action(3));
        let fd = find_last_same(&a, &b);
        assert_eq!(fd.unwrap(), 0);

        let mut a = IStateKey::default();
        a.push(Action(0));
        a.push(Action(1));
        a.push(Action(2));
        let b = a;
        a.push(Action(3));
        let fd = find_last_same(&a, &b);
        assert_eq!(fd.unwrap(), 2);

        let fd = find_last_same(&b, &a);
        assert_eq!(fd.unwrap(), 2);
    }
}
