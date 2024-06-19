use bevy::{
    math::{vec2, Vec2},
    prelude::{Component, Resource},
};
use itertools::Itertools;

use crate::sprite::TILE_SIZE;

const WORLD_SIZE: usize = 100;

#[derive(Debug, Clone, Copy)]
pub enum Character {
    Knight,
    Orc,
}
#[derive(Resource)]
pub struct SimState {
    grid: Vec<(SimCoords, SimEntity)>,
}

#[derive(Clone, Copy, Debug)]
pub enum Action {
    EndTurn,
    Attack {
        dmg: usize,
        range: usize,
        aoe: usize,
    },
    Move {
        x: usize,
        y: usize,
    },
}

#[derive(Clone)]
struct SimEntity {
    id: SimId,
    character: Character,
    actions: Vec<Action>,
}

#[derive(Clone, Copy, Debug, Default, Component, PartialEq, Eq)]
pub struct SimId(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimCoords {
    pub x: usize,
    pub y: usize,
}

fn simcoords(x: usize, y: usize) -> SimCoords {
    SimCoords { x, y }
}

impl Default for SimState {
    fn default() -> Self {
        let player = SimEntity {
            character: Character::Knight,
            id: SimId(0),
            actions: vec![Action::EndTurn],
        };
        let orc = SimEntity {
            character: Character::Orc,
            id: SimId(1),
            actions: vec![Action::EndTurn],
        };

        let grid = vec![(simcoords(0, 0), player), (simcoords(10, 5), orc)];

        Self { grid }
    }
}

impl SimState {
    pub fn apply(&mut self, id: SimId, action: Action) {
        match action {
            Action::EndTurn => todo!(),
            Action::Attack { dmg, range, aoe } => todo!(),
            Action::Move { x, y } => self.move_entity(id, x, y),
        }
    }

    fn move_entity(&mut self, id: SimId, x: usize, y: usize) {
        self.grid
            .iter_mut()
            .filter(|(_, e)| e.id == id)
            .for_each(|(c, _)| {
                c.x += x;
                c.y += y
            });
    }

    pub fn get_entity(&self, coords: SimCoords) -> Option<SimId> {
        self.grid
            .iter()
            .filter(|(c, _)| *c == coords)
            .map(|(_, e)| e.id)
            .next()
    }

    pub fn characters(&self) -> Vec<(SimId, SimCoords, Character)> {
        self.grid
            .iter()
            .map(|(c, x)| (x.id, *c, x.character))
            .collect_vec()
    }
}
