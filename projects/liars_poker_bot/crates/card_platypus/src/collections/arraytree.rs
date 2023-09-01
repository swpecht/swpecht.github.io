/// Tree data structure that that stores items based on an array
/// of values <32
pub struct ArrayTree<T> {
    root: Node<T>,
}

impl<T> ArrayTree<T> {
    /// Insert a new element into the tree, returning the old value if one existed
    pub fn insert(&mut self, k: &[u8], v: T) -> Option<T> {
        todo!()
    }

    pub fn get(&self, k: &[u8]) -> Option<&T> {
        todo!()
    }
}

impl<T> Default for ArrayTree<T> {
    fn default() -> Self {
        Self {
            root: Node::default(),
        }
    }
}

struct Node<T> {
    value: Option<T>,
    mask: u32,
    children: Vec<Node<T>>,
}

impl<T> Default for Node<T> {
    fn default() -> Self {
        Self {
            value: Default::default(),
            mask: Default::default(),
            children: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_array_tree_basic() {
        let mut tree: ArrayTree<usize> = ArrayTree::default();
    }
}
