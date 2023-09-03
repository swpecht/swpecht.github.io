use std::sync::RwLockWriteGuard;

use super::Node;

pub enum Entry<'a, T> {
    Occupied(OccupiedEntry<'a, T>),
}

pub struct OccupiedEntry<'a, T> {
    shard: RwLockWriteGuard<'a, Node<T>>,
}
