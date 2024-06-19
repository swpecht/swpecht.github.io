use bevy::{
    math::{vec2, Vec2},
    prelude::Resource,
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
    grid: Vec<Vec<Option<SimEntity>>>,
}

#[derive(Clone, Copy, Debug)]
pub enum Action {
    EndTurn,
    Attack {
        dmg: usize,
        range: usize,
        aoe: usize,
    },
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
}

#[derive(Clone)]
struct SimEntity {
    id: SimId,
    character: Character,
    actions: Vec<Action>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SimId(usize);

#[derive(Debug, Clone, Copy)]
pub struct SimCoords {
    pub x: usize,
    pub y: usize,
}

impl Default for SimState {
    fn default() -> Self {
        let player = SimEntity {
            character: Character::Knight,
            id: SimId(0),
            actions: vec![Action::MoveUp, Action::EndTurn],
        };
        let mut grid = vec![vec![None; WORLD_SIZE]; WORLD_SIZE];
        grid[0][0] = Some(player);
        grid[10][5] = Some(SimEntity {
            character: Character::Orc,
            id: SimId(1),
            actions: vec![Action::MoveUp, Action::EndTurn],
        });

        Self { grid }
    }
}

impl SimState {
    pub fn apply(&mut self, action: &Action) {
        todo!()
    }

    pub fn get_entity(&self, coords: SimCoords) -> Option<SimId> {
        self.grid
            .get(coords.x)
            .and_then(|x| x.get(coords.y))
            .map(|x| x.as_ref().map(|x| x.id))?
    }

    pub fn characters(&self) -> Vec<(SimCoords, Character)> {
        self.grid
            .iter()
            .flatten()
            .enumerate()
            .filter_map(|(i, x)| {
                x.as_ref().map(|x| {
                    (
                        SimCoords {
                            x: i / WORLD_SIZE,
                            y: i % WORLD_SIZE,
                        },
                        x.character,
                    )
                })
            })
            .collect_vec()
    }
}
