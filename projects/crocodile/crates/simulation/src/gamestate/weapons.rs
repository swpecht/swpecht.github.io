use std::{collections::HashSet, fmt::Debug};

use itertools::Itertools;

use crate::info::Weapon;

/// Collection of weapons
#[derive(PartialEq, Debug, Clone)]
pub(super) struct Arsenal {
    available: HashSet<Weapon>,
    all: HashSet<Weapon>,
}

impl Arsenal {
    pub fn from_vec(weapons: Vec<Weapon>) -> Self {
        Arsenal {
            available: HashSet::from_iter(weapons.iter().cloned()),
            all: HashSet::from_iter(weapons.iter().cloned()),
        }
    }

    pub fn enable(&mut self, weapon: Weapon) {
        if !self.all.contains(&weapon) {
            panic!("trying to reset a weapon that's not in arsenal");
        }

        self.available.insert(weapon);
    }

    pub fn disable(&mut self, weapon: &Weapon) {
        if !self.all.contains(weapon) {
            panic!("trying to disable a weapon that's not in arsenal");
        }

        self.available.remove(weapon);
    }

    pub fn is_available(&self, weapon: &Weapon) -> bool {
        self.available.contains(weapon)
    }

    pub fn available(&self) -> impl Iterator<Item = &Weapon> {
        self.available.iter().sorted()
    }

    pub fn available_melee(&self) -> impl Iterator<Item = &Weapon> {
        self.available
            .iter()
            .filter(|x| x.stats().range == 0)
            .sorted()
    }

    pub fn available_ranged(&self) -> impl Iterator<Item = &Weapon> {
        self.available
            .iter()
            .filter(|x| x.stats().range != 0)
            .sorted()
    }

    pub fn all(&self) -> impl Iterator<Item = &Weapon> {
        self.all.iter().sorted()
    }

    pub fn all_ranged(&self) -> impl Iterator<Item = &Weapon> {
        self.all.iter().filter(|x| x.stats().range != 0).sorted()
    }

    pub fn all_melee(&self) -> impl Iterator<Item = &Weapon> {
        self.all.iter().filter(|x| x.stats().range == 0).sorted()
    }
}
