use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    ops::Deref,
    sync::{atomic::AtomicUsize, RwLockReadGuard, RwLockWriteGuard},
};

use std::sync::RwLock;

use ::serde::{Deserialize, Serialize};

use crate::game::Action;

use self::treeref::{Ref, RefMut};

pub mod entry;
mod serde;
pub mod treeref;

use std::sync::atomic::Ordering::SeqCst;

/// Tree data structure that that stores items based on an array
/// of values <32
///
/// To achieve concurrency, the hash of the istate is used to choose a shard to use for storage
/// The shard based on a hash of the full istate to ensure equal distribution of items

const NUM_SHARDS: usize = 256;

#[derive(Serialize, Deserialize)]
pub struct ArrayTree<T> {
    shards: ShardList<T>,
    len: AtomicUsize,
}

struct ShardList<T>(Vec<RwLock<Shard<T>>>);

impl<T> Deref for ShardList<T> {
    type Target = Vec<RwLock<Shard<T>>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> ArrayTree<T> {
    /// Insert a new element into the tree
    pub fn insert(&self, k: &[Action], v: T) {
        assert!(!k.is_empty());

        let mut root = self.get_shard_mut(k);

        let mut cur_node = &mut root.node;

        // Save the last item for looking up the value.
        for x in k.iter().take(k.len() - 1) {
            let child = *x;
            cur_node = cur_node.get_or_create_child(child, &self.len);
        }

        let id = k.last().unwrap().0;
        cur_node.insert_value(id, v);
    }

    pub fn get(&self, k: &[Action]) -> Option<Ref<T>> {
        assert!(!k.is_empty());
        let root = self.get_shard(k);

        let mut cur_node = Some(&root.node);

        // Save the last item for looking up the value.
        for x in k.iter().take(k.len() - 1) {
            if let Some(n) = cur_node {
                let child = *x;
                cur_node = n.child(child);
            } else {
                return None;
            }
        }

        let cur_node = cur_node?;
        let id = k.last().unwrap().0;
        if let Some(v) = cur_node.get(id) {
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

        let mut cur_node = &mut root.node;

        // Save the last item for looking up the value.
        for x in k.iter().take(k.len() - 1) {
            let child = *x;
            cur_node = cur_node.get_or_create_child(child, &self.len);
        }

        let id = k.last().unwrap().0;
        if cur_node.get(id).is_none() {
            cur_node.insert_value(id, default);
        }

        unsafe {
            let vptr: *mut T = cur_node.get_mut(id).unwrap();
            RefMut::new(root, vptr)
        }
    }

    pub fn len(&self) -> usize {
        self.len.load(SeqCst)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the a read only root shard
    fn get_shard(&self, k: &[Action]) -> RwLockReadGuard<Shard<T>> {
        let idx = get_shard_index(k);
        let shard = self.shards[idx].read().unwrap();
        shard
    }

    /// Returns the a read only root shard
    fn get_shard_mut(&self, k: &[Action]) -> RwLockWriteGuard<Shard<T>> {
        let idx = get_shard_index(k);
        let shard = self.shards[idx].write().unwrap();
        shard
    }
}

fn get_shard_index(k: &[Action]) -> usize {
    let mut hasher = DefaultHasher::new();

    // we only hash the first 7 actions in the key, for euchre, this will put all hands
    // in the same shard
    k[..k.len().min(6)].hash(&mut hasher);
    let hash = hasher.finish();
    // take the top 8 bits of the hash as the index
    // gives us 2^8 shards
    (hash >> (64 - 8)) as usize
}

impl<T> Default for ArrayTree<T> {
    fn default() -> Self {
        let mut shards = Vec::new();
        for _ in 0..NUM_SHARDS {
            shards.push(RwLock::new(Shard::default()));
        }
        Self {
            shards: ShardList(shards),
            len: AtomicUsize::new(0),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Shard<T> {
    node: Node<T>,
}

impl<T> Default for Shard<T> {
    fn default() -> Self {
        Self {
            node: Node::default(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(super) struct Node<T> {
    child_mask: Mask,
    children: Vec<Node<T>>,

    value_mask: Mask,
    values: Vec<T>,
}

impl<T> Node<T> {
    // how to make this only take &self and not need mut?
    fn child(&self, id: Action) -> Option<&Node<T>> {
        let id = u8::from(id);
        debug_assert_eq!(self.child_mask.len(), self.children.len());
        debug_assert!(id < 32, "attempted to use key >32: {}", id);

        // child doesn't exist, need to insert it
        if !self.child_mask.contains(id) {
            None
        } else {
            let idx = self.child_mask.index(id);
            Some(&self.children[idx])
        }
    }

    fn get_or_create_child(&mut self, id: Action, len: &AtomicUsize) -> &mut Node<T> {
        let id = u8::from(id);

        let index = self.child_mask.index(id);
        if !self.child_mask.contains(id) {
            let new_child = Node::default();
            self.children.insert(index, new_child);
            self.child_mask.insert(id);
            len.fetch_add(1, SeqCst);
        }

        &mut self.children[index]
    }

    fn insert_value(&mut self, id: u8, v: T) {
        assert_eq!(self.values.len(), self.value_mask.len());

        let index = self.value_mask.index(id);

        if !self.value_mask.contains(id) {
            self.values.insert(index, v);
        } else {
            self.values[index] = v;
        }

        self.value_mask.insert(id);
    }

    fn get_mut(&mut self, id: u8) -> Option<&mut T> {
        assert_eq!(self.values.len(), self.value_mask.len());

        if !self.value_mask.contains(id) {
            return None;
        }

        let index = self.value_mask.index(id);
        Some(&mut self.values[index])
    }

    fn get(&self, id: u8) -> Option<&T> {
        assert_eq!(self.values.len(), self.value_mask.len());

        if !self.value_mask.contains(id) {
            return None;
        }

        let index = self.value_mask.index(id);
        Some(&self.values[index])
    }
}

#[derive(Serialize, Deserialize, Default)]
struct Mask(u32);

impl Mask {
    /// Returns the index of a particular id in the current mask
    fn index(&self, id: u8) -> usize {
        // we want to count the number of 1s before our target index
        // to do this, we mask all the top ones, and then count what remains
        let id_mask = !(!0 << id);
        (self.0 & id_mask).count_ones() as usize
    }

    fn contains(&self, id: u8) -> bool {
        self.0 & (1 << id) > 0
    }

    fn insert(&mut self, id: u8) {
        self.0 |= 1 << id;
    }

    fn len(&self) -> usize {
        self.0.count_ones() as usize
    }
}

impl<T> Default for Node<T> {
    fn default() -> Self {
        Self {
            value_mask: Default::default(),
            values: Default::default(),
            child_mask: Default::default(),
            children: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use dashmap::DashMap;
    use rand::{rngs::StdRng, thread_rng, Rng, SeedableRng};
    use rayon::prelude::*;

    #[test]
    fn test_array_tree_basic() {
        let tree: ArrayTree<usize> = ArrayTree::default();

        assert!(tree.get(&[Action(1), Action(2)]).is_none());
        tree.insert(&[Action(1), Action(2)], 1);
        assert_eq!(*tree.get(&[Action(1), Action(2)]).unwrap(), 1);
        tree.insert(&[Action(1), Action(2)], 3);
        assert_eq!(*tree.get(&[Action(1), Action(2)]).unwrap(), 3);

        tree.insert(&[Action(1)], 5);
        assert_eq!(*tree.get(&[Action(1)]).unwrap(), 5);
        tree.insert(&[Action(1)], 4);
        assert_eq!(*tree.get(&[Action(1)]).unwrap(), 4);

        for i in 0..32 {
            tree.insert(&[Action(23)], i)
        }

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
    fn test_array_tree_single_thread() {
        let t = ArrayTree::default();
        let d = DashMap::new();

        let mut rng: StdRng = SeedableRng::seed_from_u64(42);
        (0..100).for_each(|x| {
            let key = [Action(rng.gen_range(0..32))];

            d.insert(key, x + 1);
            t.insert(&key, x + 1);

            assert_eq!(
                *d.get(&key).unwrap(),
                *t.get(&key).unwrap(),
                "key: {:?}",
                key
            );
        });

        for e in d.iter() {
            let t_val = t.get(e.key()).unwrap();
            assert_eq!(*t_val, *e.value());
        }
    }

    #[test]
    fn test_array_tree_parallel() {
        let tree = Arc::new(ArrayTree::default());
        let dash = Arc::new(DashMap::new());

        // use a mutex to ensure writes to dashmap and array tree
        // happen atomically
        let lock = Arc::new(Mutex::new(1));

        (0..100).into_par_iter().for_each(|x| {
            let mut rng = thread_rng();
            let key = [Action(rng.gen_range(0..32))];
            let t = tree.clone();
            let d = dash.clone();

            let l = lock.clone();
            let g = l.lock().unwrap();
            d.insert(key, x + 1);
            t.insert(&key, x + 1);

            drop(g);
            assert_eq!(*d.get(&key).unwrap(), *t.get(&key).unwrap());
        });

        for e in dash.iter() {
            let t_val = tree.get(e.key()).unwrap();
            assert_eq!(*t_val, *e.value());
        }
    }
}
