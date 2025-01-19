use std::{collections::HashSet, fmt::Debug};

use itertools::Itertools;

/// Collection of weapons
#[derive(PartialEq, Debug, Clone)]
pub(super) struct Arsenal<T: Clone + Debug + PartialEq + Eq + std::hash::Hash> {
    available: HashSet<T>,
    all: HashSet<T>,
}

impl<T> Arsenal<T>
where
    T: Clone + Debug + PartialEq + Eq + std::hash::Hash + Ord,
{
    pub fn from_vec(weapons: Vec<T>) -> Self {
        Arsenal {
            available: HashSet::from_iter(weapons.iter().cloned()),
            all: HashSet::from_iter(weapons.iter().cloned()),
        }
    }

    pub fn enable(&mut self, weapon: T) {
        if !self.all.contains(&weapon) {
            panic!("trying to reset a weapon that's not in arsenal");
        }

        self.available.insert(weapon);
    }

    pub fn disable(&mut self, weapon: &T) {
        if !self.all.contains(weapon) {
            panic!("trying to disable a weapon that's not in arsenal");
        }

        self.available.remove(weapon);
    }

    pub fn is_available(&self, weapon: &T) -> bool {
        self.available.contains(weapon)
    }

    pub fn available(&self) -> impl Iterator<Item = &T> {
        self.available.iter().sorted()
    }

    pub fn all(&self) -> impl Iterator<Item = &T> {
        self.all.iter().sorted()
    }
}
