use std::{
    fmt::Debug,
    ops::{Deref, DerefMut},
    sync::{RwLockReadGuard, RwLockWriteGuard},
};

use super::{Node, Shard};

/// Struct to tracking the array tree shard RwLocks when returning values
///
/// This is based on DashMap's implementation:
///     https://github.com/xacrimon/dashmap/blob/master/src/mapref/one.rs
pub struct Ref<'a, T> {
    _guard: RwLockReadGuard<'a, Shard<T>>,
    value: *const T,
}

/// Ref is Send and Sync if it's underlying data is sync
unsafe impl<'a, T: Sync> Send for Ref<'a, T> {}
unsafe impl<'a, T: Sync> Sync for Ref<'a, T> {}

impl<'a, T> Ref<'a, T> {
    pub(super) unsafe fn new(guard: RwLockReadGuard<'a, Shard<T>>, v: *const T) -> Self {
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

pub struct RefMut<'a, T> {
    _guard: RwLockWriteGuard<'a, Shard<T>>,
    value: *mut T,
}

unsafe impl<'a, T: Sync> Send for RefMut<'a, T> {}
unsafe impl<'a, T: Sync> Sync for RefMut<'a, T> {}

impl<'a, T> RefMut<'a, T> {
    pub(super) unsafe fn new(guard: RwLockWriteGuard<'a, Shard<T>>, v: *mut T) -> Self {
        Self {
            _guard: guard,
            value: v,
        }
    }

    pub fn value(&self) -> &T {
        unsafe { &*self.value }
    }

    pub fn value_mut(&mut self) -> &mut T {
        unsafe { &mut *self.value }
    }
}

impl<'a, T> Deref for RefMut<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.value()
    }
}

impl<'a, T> DerefMut for RefMut<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value_mut()
    }
}

impl<'a, T> Debug for RefMut<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ref").field("value", &self.value).finish()
    }
}
