use std::{fmt::Debug, ops::Deref, sync::RwLockReadGuard};

use super::Node;

/// Struct to tracking the array tree shard RwLocks when returning values
///
/// This is based on DashMap's implementation:
///     https://github.com/xacrimon/dashmap/blob/master/src/mapref/one.rs
pub struct Ref<'a, T> {
    _guard: RwLockReadGuard<'a, Node<T>>,
    value: *const T,
}

/// Ref is Send and Sync if it's underlying data is sync
unsafe impl<'a, T: Sync> Send for Ref<'a, T> {}
unsafe impl<'a, T: Sync> Sync for Ref<'a, T> {}

impl<'a, T> Ref<'a, T> {
    pub(super) unsafe fn new(guard: RwLockReadGuard<'a, Node<T>>, v: *const T) -> Self {
        Self {
            _guard: guard,
            value: v,
        }
    }

    pub fn value(&self) -> &T {
        unsafe { &*self.value }
    }
}

impl<'a, T> Deref for Ref<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.value()
    }
}

impl<'a, T> Debug for Ref<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ref").field("value", &self.value).finish()
    }
}

pub struct RefMut {}
