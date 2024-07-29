use std::ops::{Index, IndexMut};

use itertools::Itertools;

#[derive(Clone)]
pub struct Slab<T> {
    /// generation count for each
    generation: Vec<u16>,
    /// actual storage
    mem: Vec<T>,
    /// empty indexes
    empty: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct SlabIdx {
    loc: u8,
    generation: u16,
}

impl<T> Slab<T> {
    pub fn remove(&mut self, index: SlabIdx) {
        self.empty.push(index.loc);
        self.generation[index.loc as usize] += 1;
    }
}

impl<T: Default + Clone> Slab<T> {
    pub fn with_capacity(capacity: usize) -> Self {
        Slab {
            generation: vec![0; capacity],
            mem: (0..capacity).map(|_| T::default()).collect_vec(),
            empty: (0..capacity).map(|x| x as u8).collect_vec(),
        }
    }

    /// Allocate new entry if needed.
    fn add_entry(&mut self) {
        let new_item = self.mem.len();
        self.mem.push(T::default());
        self.empty.push(new_item as u8);
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

        let Ok([source, target]) = self
            .mem
            .get_many_mut([source_idx.loc as usize, target_idx.loc as usize])
        else {
            panic!("failed to split slab memory")
        };
        target.clone_from(source);

        target_idx
    }
}

impl<T> IndexMut<&SlabIdx> for Slab<T> {
    fn index_mut(&mut self, index: &SlabIdx) -> &mut Self::Output {
        if index.generation != self.generation[index.loc as usize] {
            panic!("tried to use key with invalid generation, {:?}", index);
        }

        &mut self.mem[index.loc as usize]
    }
}

impl<T> Index<&SlabIdx> for Slab<T> {
    type Output = T;

    fn index(&self, index: &SlabIdx) -> &Self::Output {
        if index.generation != self.generation[index.loc as usize] {
            panic!("tried to use key with invalid generation, {:?}", index);
        }

        &self.mem[index.loc as usize]
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_slab() {
        let mut slab: Slab<usize> = Slab::with_capacity(8);
        let a = slab.get_vacant();
        slab[&a] = 1;

        let b = slab.clone_from(&a);
        slab[&b] += 1;

        assert_eq!(slab[&a], 1);
        assert_eq!(slab[&b], 2);
    }
}
