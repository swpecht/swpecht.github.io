use std::{
    fmt::Display,
    ops::{Add, Sub},
};

use bevy::prelude::{Component, Resource};
use itertools::Itertools;

const WORLD_SIZE: usize = 100;

#[derive(Debug, Clone, Copy)]
pub enum Character {
    Knight,
    Orc,
}
#[derive(Resource)]
pub struct SimState {
    next_id: usize,
    initiative: Vec<SimId>, // order of players
    grid: Vec<(SimCoords, SimEntity)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    EndTurn,
    UseAbility { target: SimCoords, ability: Ability },
    Move { target: SimCoords },
}

impl Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Action::EndTurn => f.write_str("End turn"),
            Action::UseAbility { target, ability } => todo!(),
            Action::Move { target } => {
                f.write_fmt(format_args!("Move: {}, {}", target.x, target.y))
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ability {
    MeleeAttack,
    BowAttack { range: usize },
}

#[derive(Clone)]
struct SimEntity {
    id: SimId,
    turn_movement: usize,
    movement: usize,
    character: Character,
    abilities: Vec<Ability>,
}

#[derive(Clone, Copy, Debug, Default, Component, PartialEq, Eq)]
pub struct SimId(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimCoords {
    pub x: usize,
    pub y: usize,
}

impl Add for SimCoords {
    type Output = SimCoords;

    fn add(self, rhs: Self) -> Self::Output {
        let mut out = self;
        out.x += rhs.x;
        out.y += rhs.y;
        out
    }
}

impl Sub for SimCoords {
    type Output = SimCoords;

    fn sub(self, rhs: Self) -> Self::Output {
        let mut out = self;
        out.x -= rhs.x;
        out.y -= rhs.y;
        out
    }
}

fn sc(x: usize, y: usize) -> SimCoords {
    SimCoords { x, y }
}

impl Default for SimState {
    fn default() -> Self {
        let mut state = Self {
            next_id: 0,
            initiative: Vec::new(),
            grid: Vec::new(),
        };

        state.insert_entity(
            Character::Knight,
            vec![Ability::MeleeAttack, Ability::BowAttack { range: 20 }],
            sc(0, 0),
        );
        state.insert_entity(Character::Orc, vec![Ability::MeleeAttack], sc(5, 10));

        state
    }
}

impl SimState {
    fn insert_entity(&mut self, character: Character, abilities: Vec<Ability>, loc: SimCoords) {
        let entity = SimEntity {
            id: SimId(self.next_id),
            character,
            abilities,
            turn_movement: character.default_movement(),
            movement: character.default_movement(),
        };

        self.initiative.push(SimId(self.next_id));
        self.next_id += 1;
        self.grid.push((loc, entity));
    }

    pub fn apply(&mut self, action: Action) {
        match action {
            Action::EndTurn => self.apply_end_turn(),
            Action::Move { target } => self.apply_move_entity(target),
            Action::UseAbility { target, ability } => todo!(),
        }
    }

    pub fn cur_char(&self) -> SimId {
        self.initiative[0]
    }

    fn apply_move_entity(&mut self, target: SimCoords) {
        if let Some((c, entity)) = self
            .grid
            .iter_mut()
            .find(|(_, e)| e.id == self.initiative[0])
        {
            *c = target;
            entity.movement -= 1;
        };
    }

    fn apply_end_turn(&mut self) {
        // reset movement
        let cur_char = self.cur_char();
        if let Some(entity) = self.get_entity_mut(cur_char) {
            entity.movement = entity.turn_movement
        }

        self.initiative.rotate_left(1);
    }

    pub fn get_id(&self, coords: SimCoords) -> Option<SimId> {
        self.grid
            .iter()
            .filter(|(c, _)| *c == coords)
            .map(|(_, e)| e.id)
            .next()
    }

    fn get_entity_mut(&mut self, id: SimId) -> Option<&mut SimEntity> {
        self.grid
            .iter_mut()
            .find(|(_, e)| e.id == id)
            .map(|(_, e)| e)
    }

    fn get_entity(&self, id: SimId) -> &SimEntity {
        self.grid
            .iter()
            .find(|(_, e)| e.id == id)
            .map(|(_, e)| e)
            .unwrap()
    }

    pub fn characters(&self) -> Vec<(SimId, SimCoords, Character)> {
        self.grid
            .iter()
            .map(|(c, x)| (x.id, *c, x.character))
            .collect_vec()
    }

    pub fn loc(&self, id: SimId) -> Option<SimCoords> {
        self.grid.iter().find(|(_, e)| e.id == id).map(|(c, _)| *c)
    }

    pub fn abilities(&self) -> Vec<Ability> {
        self.get_entity(self.cur_char()).abilities.clone()
    }

    pub fn legal_actions(&self) -> Vec<Action> {
        use Action::*;
        let mut actions = vec![EndTurn];

        let loc = self.loc(self.cur_char()).unwrap();
        if self.get_entity(self.cur_char()).movement > 0 {
            actions.push(Move {
                target: loc + sc(0, 1),
            });

            if loc.y > 0 {
                actions.push(Move {
                    target: loc - sc(0, 1),
                });
            }
            actions.push(Move {
                target: loc + sc(1, 0),
            });

            if loc.x > 0 {
                actions.push(Move {
                    target: loc - sc(1, 0),
                });
            }
        }
        actions
    }
}

impl Character {
    fn default_movement(&self) -> usize {
        match self {
            Character::Knight => 2,
            Character::Orc => 2,
        }
    }
}
