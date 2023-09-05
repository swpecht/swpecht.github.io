use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::{RwLockReadGuard, RwLockWriteGuard},
};

use std::sync::RwLock;

use ::serde::{Deserialize, Serialize};

use crate::game::Action;

use self::treeref::{Ref, RefMut};

pub mod entry;
mod serde;
pub mod treeref;

/// Tree data structure that that stores items based on an array
/// of values <32
///
/// To achieve concurrency, the hash of the istate is used to choose a shard to use for storage
/// The shard based on a hash of the full istate to ensure equal distribution of items

const NUM_SHARDS: usize = 256;

pub struct ArrayTree<T> {
    shards: Vec<RwLock<Shard<T>>>,
}

impl<T> ArrayTree<T> {
    /// Insert a new element into the tree
    pub fn insert(&self, k: &[Action], v: T) {
        assert!(!k.is_empty());

        let mut root = self.get_shard_mut(k);
        root.len += 1;

        let mut cur_node = root.node.get_or_create_child(k[0]);
        let remaining_key = &k[1..];

        for x in remaining_key {
            let child = *x;
            cur_node = cur_node.get_or_create_child(child);
        }

        cur_node.value = Some(v);
    }

    pub fn get(&self, k: &[Action]) -> Option<Ref<T>> {
        assert!(!k.is_empty());
        let root = self.get_shard(k);

        let mut cur_node = root.node.child(k[0]);
        let remaining_key = &k[1..];

        for x in remaining_key {
            if let Some(n) = cur_node {
                let child = *x;
                cur_node = n.child(child);
            } else {
                return None;
            }
        }

        let cur_node = cur_node?;
        if let Some(v) = &cur_node.value {
            unsafe {
                let vptr: *const T = v;
                Some(Ref::new(root, vptr))
            }
        } else {
            None
        }
    }

    pub fn get_or_create_mut(&self, k: &[Action], default: T) -> RefMut<T> {
        assert!(!k.is_empty());
        let mut root = self.get_shard_mut(k);
        root.len += 1;

        let mut cur_node = root.node.get_or_create_child(k[0]);
        let remaining_key = &k[1..];

        for x in remaining_key {
            let child = *x;
            cur_node = cur_node.get_or_create_child(child);
        }

        if cur_node.value.is_none() {
            cur_node.value = Some(default);
        }

        unsafe {
            let vptr: *mut T = cur_node.value.as_mut().unwrap();
            RefMut::new(root, vptr)
        }
    }

    pub fn len(&self) -> usize {
        self.shards.iter().map(|x| x.read().unwrap().len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the a read only root shard
    fn get_shard(&self, k: &[Action]) -> RwLockReadGuard<Shard<T>> {
        let mut hasher = DefaultHasher::new();
        k.hash(&mut hasher);
        let hash = hasher.finish();
        // take the top 8 bits of the hash as the index
        let idx = (hash >> (64 - 8)) as usize;
        let shard = self.shards[idx].read().unwrap();
        shard
    }

    /// Returns the a read only root shard
    fn get_shard_mut(&self, k: &[Action]) -> RwLockWriteGuard<Shard<T>> {
        let mut hasher = DefaultHasher::new();
        k.hash(&mut hasher);
        let hash = hasher.finish();
        // take the top 8 bits of the hash as the index
        let idx = (hash >> (64 - 8)) as usize;
        let shard = self.shards[idx].write().unwrap();
        shard
    }
}

impl<T> Default for ArrayTree<T> {
    fn default() -> Self {
        let mut shards = Vec::new();
        for _ in 0..NUM_SHARDS {
            shards.push(RwLock::new(Shard::default()));
        }
        Self { shards }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Shard<T> {
    node: Node<T>,
    len: usize,
}

impl<T> Default for Shard<T> {
    fn default() -> Self {
        Self {
            node: Node::default(),
            len: Default::default(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(super) struct Node<T> {
    value: Option<T>,
    child_mask: u32,
    children: Vec<Node<T>>,
}

impl<T> Node<T> {
    // how to make this only take &self and not need mut?
    fn child(&self, id: Action) -> Option<&Node<T>> {
        let id = u8::from(id);
        debug_assert_eq!(self.child_mask.count_ones() as usize, self.children.len());
        debug_assert!(id < 32, "attempted to use key >32: {}", id);

        let mask_contains = self.child_mask & (1u32 << id) > 0;

        // child doesn't exist, need to insert it
        if !mask_contains {
            None
        } else {
            let idx = index(self.child_mask, id);
            Some(&self.children[idx])
        }
    }

    fn get_or_create_child(&mut self, id: Action) -> &mut Node<T> {
        let id = u8::from(id);

        let mask_contains = self.child_mask & (1u32 << id) > 0;
        let index = index(self.child_mask, id);
        if !mask_contains {
            let new_child = Node::default();
            self.children.insert(index, new_child);
            self.child_mask |= 1 << id;
        }

        &mut self.children[index]
    }
}

fn index(child_mask: u32, id: u8) -> usize {
    // we want to count the number of 1s before our target index
    // to do this, we mask all the top ones, and then count what remains
    let mask = !(!0 << id);
    (child_mask & mask).count_ones() as usize
}

impl<T> Default for Node<T> {
    fn default() -> Self {
        Self {
            value: Default::default(),
            child_mask: Default::default(),
            children: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use dashmap::DashMap;
    use rand::{thread_rng, Rng};
    use rayon::prelude::*;

    #[test]
    fn test_array_tree_basic() {
        let tree: ArrayTree<usize> = ArrayTree::default();

        assert!(tree.get(&[Action(1), Action(2)]).is_none());
        tree.insert(&[Action(1), Action(2)], 1);
        assert_eq!(*tree.get(&[Action(1), Action(2)]).unwrap(), 1);

        tree.insert(&[Action(0)], 5);
        assert_eq!(*tree.get(&[Action(0)]).unwrap(), 5);
        tree.insert(&[Action(0)], 4);
        assert_eq!(*tree.get(&[Action(0)]).unwrap(), 4);

        // This can deadlock if we hold the reference into the map
        {
            let mut c = tree.get_or_create_mut(&[Action(0), Action(2)], 0);
            assert_eq!(*c, 0);
            *c = 1;
        }

        assert_eq!(*tree.get(&[Action(0), Action(2)]).unwrap(), 1);

        {
            let mut c = tree.get_or_create_mut(&[Action(0), Action(2)], 0);
            assert_eq!(*c, 1);
            *c += 5;
        }

        // touch a different part of the tree
        tree.insert(&[Action(0), Action(1)], 0);

        assert_eq!(*tree.get(&[Action(0), Action(2)]).unwrap(), 6);
    }

    #[test]
    fn test_array_tree_parallel() {
        let tree = Arc::new(ArrayTree::default());
        let dash = Arc::new(DashMap::new());

        (0..1000).into_par_iter().for_each(|x| {
            let mut rng = thread_rng();
            let key = [Action(rng.gen_range(0..32))];
            let t = tree.clone();
            let d = dash.clone();
            d.insert(key, x + 1);
            t.insert(&key, x + 1);
        });

        for e in dash.iter() {
            let t_val = tree.get(e.key()).unwrap();
            assert_eq!(*t_val, *e.value());
        }
    }
}
