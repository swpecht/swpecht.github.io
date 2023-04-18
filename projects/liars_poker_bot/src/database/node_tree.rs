use rustc_hash::FxHashMap;

use crate::{
    game::{arrayvec::ArrayVec, Action},
    istate::IStateKey,
};

type HashMap<K, V> = FxHashMap<K, V>;

/// A performant datastructure for storing nodes in memory
pub struct Tree<T: Copy> {
    nodes: Vec<Node<T>>,
    /// the starting roots of the tree
    roots: HashMap<Action, usize>,
    last_node: (ArrayVec<64>, usize),
    /// a cursor for each root of the tree
    cursors: HashMap<Action, Cursor>,
}

struct Cursor {
    id: usize,
    path: ArrayVec<64>,
}

struct Node<T> {
    parent: usize,
    children: HashMap<Action, usize>,
    action: Action,
    v: Option<T>,
}

impl<T> Node<T> {
    fn new(p: Action, a: Action, v: Option<T>) -> Self {
        Self {
            parent: p,
            children: HashMap::default(),
            action: a,
            v: v,
        }
    }
}

impl<T: Copy> Tree<T> {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            roots: HashMap::default(),
            last_node: (ArrayVec::new(), 0),
            cursors: HashMap::default(),
        }
    }

    pub fn insert(&mut self, k: IStateKey, v: T) -> Option<T> {
        let ka = k.get_actions();

        let root = self.get_or_create_root(ka[0]);
        let id = self.find_node(ka, root);
        let n = &mut self.nodes[id];
        n.v = Some(v);

        return None;
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
        let c = p.children.get(&action);

        if c.is_some() {
            return *c.unwrap();
        }

        let cn: Node<T> = Node::new(parent, action, None);
        let c = self.nodes.len();
        self.nodes.push(cn);

        let p = &mut self.nodes[parent];
        p.children.insert(action, c);

        return c;
    }

    /// Return the index of the node for a given ka. Creates nodes along the way as needed
    fn find_node(&mut self, ka: ArrayVec<64>, root: usize) -> usize {
        let (lka, id) = self.last_node;
        if lka == ka {
            return id;
        }

        let mut depth = 0;
        let a = ka[depth];
        let mut idx = root;
        if a != self.nodes[idx].action {
            panic!("trying to insert a node that isn't a child of the root node");
        }

        let ancestor = self.find_common_ancestor(ka);

        loop {
            let next_action = ka[depth + 1];

            let child = self.get_or_create_child(idx, next_action);

            if depth + 1 == ka.len() {
                self.last_node = (ka, child);
                return child;
            }

            idx = child;
            depth += 1;
        }
    }

    /// Returns the id of the nearest common ancestor to a node
    ///
    /// Because nodes are read "near" each other, this should be much faster than always traversing the tree
    fn find_common_ancestor(&self, ka: ArrayVec<64>) -> Option<usize> {
        let cursor = self.cursors.get(&ka[0]);
        if cursor.is_none() {
            return None;
        }
        let cursor = cursor.unwrap();
        let ca = cursor.path;
        let diff = find_first_diff(ka, ca);
        if diff.is_none() {
            return Some(cursor.id);
        }
        let diff = diff.unwrap();

        let mut cur_node = cursor.id;
        for _ in 0..diff {
            let n = &self.nodes[cur_node];
            let p = n.parent;
            cur_node = p;
        }

        return Some(cur_node);
    }

    pub fn get(&mut self, k: &IStateKey) -> Option<T> {
        let root = self.roots.get(&k[0]);
        if root.is_none() {
            return None;
        }
        let idx = self.find_node(k.get_actions(), *root.unwrap());
        return self.nodes[idx].v;
    }

    pub fn contains_key(&mut self, k: &IStateKey) -> bool {
        return self.get(k).is_some();
    }
}

/// finds the first difference between action lists if one exists
fn find_first_diff(ka: ArrayVec<64>, ca: ArrayVec<64>) -> Option<usize> {
    let len = ka.len().min(ca.len());
    for i in 0..len {
        if ka[i] != ca[i] {
            return Some(i);
        }
    }

    if ka.len() == ca.len() {
        return None;
    }

    return Some(len);
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
