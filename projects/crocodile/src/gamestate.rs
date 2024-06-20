use std::fmt::Display;

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
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
}

impl Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Action::EndTurn => f.write_str("End turn"),
            Action::UseAbility { target, ability } => todo!(),
            Action::MoveUp => f.write_str("Move up"),
            Action::MoveDown => f.write_str("Move down"),
            Action::MoveLeft => f.write_str("Move left"),
            Action::MoveRight => f.write_str("Move right"),
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

fn simcoords(x: usize, y: usize) -> SimCoords {
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
            simcoords(0, 0),
        );
        state.insert_entity(Character::Orc, vec![Ability::MeleeAttack], simcoords(5, 10));

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
            Action::MoveUp => self.apply_move_entity(0, 1),
            Action::MoveDown => self.apply_move_entity(0, -1),
            Action::MoveLeft => self.apply_move_entity(-1, 0),
            Action::MoveRight => self.apply_move_entity(1, 0),
            Action::UseAbility { target, ability } => todo!(),
        }
    }

    pub fn cur_char(&self) -> SimId {
        self.initiative[0]
    }

    fn apply_move_entity(&mut self, x: i8, y: i8) {
        if let Some((c, entity)) = self
            .grid
            .iter_mut()
            .find(|(_, e)| e.id == self.initiative[0])
        {
            c.x = (c.x as i8 + x) as usize;
            c.y = (c.y as i8 + y) as usize;
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
        if self.get_entity(self.cur_char()).movement > 0 {
            actions.push(MoveUp);
            actions.push(MoveDown);
            actions.push(MoveLeft);
            actions.push(MoveRight);
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
