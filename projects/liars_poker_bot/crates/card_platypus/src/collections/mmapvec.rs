use std::{marker::PhantomData, ops::Deref};

use anyhow::bail;
use bytemuck::Pod;
use memmap2::MmapMut;

const STARTING_SIZE: usize = 10_000_000;

/// A vector backed by a temporary memory map
pub struct MMapVec<T> {
    len: usize,
    mmap: MmapMut,
    _phantom: PhantomData<T>,
}

impl<T> MMapVec<T> {
    pub fn try_new() -> anyhow::Result<Self> {
        let item_size = std::mem::size_of::<T>();

        let mmap = memmap2::MmapOptions::new()
            .len(STARTING_SIZE * item_size)
            .map_anon()?;

        Ok(Self {
            len: 0,
            mmap,
            _phantom: PhantomData,
        })
    }

    pub fn len(&self) -> usize {
        self.len
    }
}

impl<T: Pod> MMapVec<T> {
    pub fn try_push(&mut self, item: T) -> anyhow::Result<()> {
        let value = [item];
        let data = bytemuck::cast_slice::<T, u8>(&value);

        assert_eq!(data.len(), std::mem::size_of::<T>());

        let item_size = data.len();
        let start = self.len() * item_size;

        if start + data.len() > self.mmap.len() {
            bail!("failed to push, expanding memory map is not yet supported");
        }

        self.mmap[start..start + data.len()].copy_from_slice(data);
        self.len += 1;
        Ok(())
    }
}

impl<T: Pod> Deref for MMapVec<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        let item_size = std::mem::size_of::<T>();
        bytemuck::cast_slice(&self.mmap[..self.len() * item_size])
    }
}

impl<T: Pod> FromIterator<T> for MMapVec<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut mmap_vec = MMapVec::try_new().expect("failed to create mmaped vector");

        for x in iter {
            mmap_vec.try_push(x).expect("failed to push item");
        }

        mmap_vec
    }
}

#[cfg(test)]
mod tests {
    use itertools::Itertools;

    use super::*;

    #[test]
    fn test_disk_backed_vector() {
        let iterator = (0..100);
        let data = MMapVec::from_iter(iterator);

        assert_eq!(&data[..], &(0..100).collect_vec());
    }
}
