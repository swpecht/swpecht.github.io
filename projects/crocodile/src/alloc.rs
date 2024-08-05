use std::{
    fmt::Debug,
    ops::{Index, IndexMut},
};

use itertools::Itertools;

#[derive(Clone)]
pub struct Slab<T> {
    /// generation count for each
    generation: Vec<u32>,
    /// actual storage
    mem: Vec<T>,
    /// empty indexes
    empty: Vec<u16>,
}

#[derive(Clone, Debug)]
pub struct SlabIdx {
    loc: u16,
    generation: u32,
}

impl<T> Slab<T> {
    pub fn remove(&mut self, index: &SlabIdx) {
        self.empty.push(index.loc);
        self.generation[index.loc as usize] += 1;
    }
}

impl<T: Default + Clone> Slab<T> {
    pub fn with_capacity(capacity: usize) -> Self {
        Slab {
            generation: vec![0; capacity],
            mem: (0..capacity).map(|_| T::default()).collect_vec(),
            empty: (0..capacity).rev().map(|x| x as u16).collect_vec(),
        }
    }

    /// Allocate new entry if needed.
    fn add_entry(&mut self) {
        let new_item = self.mem.len();
        let new_entry = self.mem.last().unwrap().clone();
        self.mem.push(new_entry);
        self.empty.push(new_item as u16);
        self.generation.push(0);
    }

    /// Get get for a vacant entry. The state of T is unknown
    pub fn get_vacant(&mut self) -> SlabIdx {
        if self.empty.is_empty() {
            self.add_entry();
        }

        let loc = self.empty.pop().expect("failed to allocate a new entry");
        let generation = self.generation[loc as usize];

        SlabIdx { loc, generation }
    }

    pub fn clone_from(&mut self, source_idx: &SlabIdx) -> SlabIdx {
        let target_idx = self.get_vacant();
        assert_eq!(
            self.generation[source_idx.loc as usize], source_idx.generation,
            "tried to clone from key with invalid generation"
        );

        let [source, target] = self
            .mem
            .get_many_mut([source_idx.loc as usize, target_idx.loc as usize])
            .expect("failed to get many mut for slab");

        target.clone_from(source);

        target_idx
    }

    /// Checks if a key is valid
    pub fn is_valid(&self, idx: &SlabIdx) -> bool {
        idx.loc < self.mem.len() as u16 && idx.generation == self.generation[idx.loc as usize]
    }
}

impl<T> IndexMut<&SlabIdx> for Slab<T> {
    fn index_mut(&mut self, index: &SlabIdx) -> &mut Self::Output {
        if index.generation != self.generation[index.loc as usize] {
            panic!(
                "tried to use key with invalid generation, key: {:?}, item gen: {}",
                index, self.generation[index.loc as usize]
            );
        }

        &mut self.mem[index.loc as usize]
    }
}

impl<T> Index<&SlabIdx> for Slab<T> {
    type Output = T;

    fn index(&self, index: &SlabIdx) -> &Self::Output {
        if index.generation != self.generation[index.loc as usize] {
            panic!(
                "tried to use key with invalid generation, key: {:?}, item gen: {}",
                index, self.generation[index.loc as usize]
            );
        }

        &self.mem[index.loc as usize]
    }
}

impl<T> Debug for Slab<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Slab")
            .field("generation", &self.generation)
            .field("empty", &self.empty)
            .finish()
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_slab() {
        let mut slab: Slab<usize> = Slab::with_capacity(2);
        let a = slab.get_vacant();
        assert!(slab.is_valid(&a));
        slab[&a] = 1;

        let b = slab.clone_from(&a);
        slab[&b] += 1;

        assert_eq!(slab[&a], 1);
        assert_eq!(slab[&b], 2);

        slab.remove(&a);
        let c = slab.get_vacant();
        assert_eq!(slab[&c], 1); // should be the old value
        let res = std::panic::catch_unwind(|| slab[&a]);
        assert!(res.is_err());
        slab[&c] = 3;
        let d = slab.get_vacant();
        assert_eq!(slab[&b], 2);
        assert_eq!(slab[&c], 3);
        assert_eq!(slab[&d], 0);
    }
}
