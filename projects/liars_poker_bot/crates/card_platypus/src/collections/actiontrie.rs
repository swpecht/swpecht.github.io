use ::serde::{Deserialize, Serialize};

use games::Action;

#[derive(Serialize, Deserialize)]
pub struct ActionTrie<T> {
    root: Node<T>,
    len: usize,
}

impl<T> ActionTrie<T> {
    /// Insert a new element into the tree
    pub fn insert(&mut self, k: &[Action], v: T) {
        assert!(!k.is_empty());

        let mut cur_node = &mut self.root;

        // Save the last item for looking up the value.
        for x in k.iter().take(k.len() - 1) {
            let child = *x;
            cur_node = cur_node.get_or_create_child(child);
        }

        let id = k.last().unwrap().0;
        cur_node.insert_value(id, v, &mut self.len);
    }

    pub fn get(&self, k: &[Action]) -> Option<&T> {
        assert!(!k.is_empty());

        let mut cur_node = Some(&self.root);

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
        cur_node.get(id)
    }

    pub fn get_or_create_mut(&mut self, k: &[Action], default: T) -> &mut T {
        assert!(!k.is_empty());
        let mut cur_node = &mut self.root;

        // Save the last item for looking up the value.
        for x in k.iter().take(k.len() - 1) {
            let child = *x;
            cur_node = cur_node.get_or_create_child(child);
        }

        let id = k.last().unwrap().0;
        if cur_node.get(id).is_none() {
            cur_node.insert_value(id, default, &mut self.len);
        }

        cur_node.get_mut(id).unwrap()
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T> Default for ActionTrie<T> {
    fn default() -> Self {
        Self {
            root: Node::default(),
            len: 0,
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

    fn get_or_create_child(&mut self, id: Action) -> &mut Node<T> {
        let id = u8::from(id);

        let index = self.child_mask.index(id);
        if !self.child_mask.contains(id) {
            let new_child = Node::default();
            self.children.insert(index, new_child);
            self.child_mask.insert(id);
        }

        &mut self.children[index]
    }

    fn insert_value(&mut self, id: u8, v: T, len: &mut usize) {
        assert_eq!(self.values.len(), self.value_mask.len());

        let index = self.value_mask.index(id);

        if !self.value_mask.contains(id) {
            self.values.insert(index, v);
            *len += 1;
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

    use super::*;
    use dashmap::DashMap;
    use rand::{rngs::StdRng, Rng, SeedableRng};

    #[test]
    fn test_array_tree_basic() {
        let mut tree: ActionTrie<usize> = ActionTrie::default();

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
            let c = tree.get_or_create_mut(&[Action(0), Action(2)], 0);
            assert_eq!(*c, 0);
            *c = 1;
        }

        assert_eq!(*tree.get(&[Action(0), Action(2)]).unwrap(), 1);

        {
            let c = tree.get_or_create_mut(&[Action(0), Action(2)], 0);
            assert_eq!(*c, 1);
            *c += 5;
        }

        // touch a different part of the tree
        tree.insert(&[Action(0), Action(1)], 0);

        assert_eq!(*tree.get(&[Action(0), Action(2)]).unwrap(), 6);
    }

    #[test]
    fn test_array_tree_single_thread() {
        let mut t = ActionTrie::default();
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
}
