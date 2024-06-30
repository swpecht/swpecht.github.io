use std::{
    fmt::Display,
    ops::{Add, Sub},
};

use bevy::prelude::{Component, Resource};
use itertools::Itertools;

const WORLD_SIZE: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Team {
    Players(usize),
    NPCs(usize),
}

#[derive(Debug, Clone, Copy, Hash)]
pub enum Character {
    Knight,
    Orc,
}
#[derive(Resource, Hash, Clone)]
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
            Action::UseAbility { target, ability } => {
                f.write_fmt(format_args!("{}: {}, {}", ability, target.x, target.y))
            }
            Action::Move { target } => {
                f.write_fmt(format_args!("Move: {}, {}", target.x, target.y))
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ability {
    MeleeAttack,
    BowAttack { range: usize },
}

#[derive(Clone, Hash)]
struct SimEntity {
    health: usize,
    id: SimId,
    turn_movement: usize,
    movement: usize,
    character: Character,
    abilities: Vec<Ability>,
    remaining_actions: usize,
    team: Team,
}

#[derive(Clone, Copy, Debug, Default, Component, PartialEq, Eq, Hash)]
pub struct SimId(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

impl SimCoords {
    fn dist(&self, other: &SimCoords) -> usize {
        self.x.abs_diff(other.x) + self.y.abs_diff(other.y)
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
            sc(3, 10),
        );
        state.insert_entity(Character::Orc, vec![Ability::MeleeAttack], sc(5, 10));
        // state.insert_entity(Character::Orc, vec![Ability::MeleeAttack], sc(6, 10));
        state.insert_entity(Character::Orc, vec![Ability::MeleeAttack], sc(4, 10));

        state
    }
}

impl SimState {
    pub fn apply(&mut self, action: Action) {
        match action {
            Action::EndTurn => self.apply_end_turn(),
            Action::Move { target } => self.apply_move_entity(target),
            Action::UseAbility { target, ability } => self.apply_use_ability(target, ability),
        }
    }

    pub fn legal_actions(&self, actions: &mut Vec<Action>) {
        actions.clear();
        use Action::*;
        actions.push(EndTurn);

        let loc = self.loc(self.cur_char()).unwrap();
        let mut candidate_locs = Vec::new();
        let cur_entity = self.get_entity(self.cur_char());

        if cur_entity.movement > 0 {
            if loc.y < WORLD_SIZE {
                candidate_locs.push(loc + sc(0, 1));
            }

            if loc.y > 0 {
                candidate_locs.push(loc - sc(0, 1));
            }

            if loc.x < WORLD_SIZE {
                candidate_locs.push(loc + sc(1, 0));
            }

            if loc.x > 0 {
                candidate_locs.push(loc - sc(1, 0));
            }

            let populdated_locs = self.populated_cells(loc, 1);
            candidate_locs
                .iter()
                .filter(|x| !populdated_locs.contains(x))
                .map(|&target| Move { target })
                .for_each(|x| actions.push(x));
        }

        if cur_entity.remaining_actions > 0 {
            for ability in cur_entity.abilities.iter() {
                let populated_cells = self.populated_cells(loc, ability.range());
                // filter out the characters own cell
                for cell in populated_cells.iter().filter(|x| **x != loc) {
                    actions.push(Action::UseAbility {
                        target: *cell,
                        ability: *ability,
                    });
                }
            }
        }
    }

    /// Determine if the sim is in a terminal gamestate where all player characters or
    /// all npcs are dead
    pub fn is_terminal(&self) -> bool {
        let mut count_players = 0;
        let mut count_npcs = 0;
        for (_, entity) in &self.grid {
            match entity.team {
                Team::Players(_) => count_players += 1,
                Team::NPCs(_) => count_npcs += 1,
            };
        }
        count_players == 0 || count_npcs == 0
    }

    pub fn evaluate(&self, team: Team) -> f32 {
        let mut player_health = 0;
        let mut npc_health = 0;
        for (_, entity) in &self.grid {
            match entity.team {
                Team::Players(_) => player_health += entity.health,
                Team::NPCs(_) => npc_health += entity.health,
            }
        }

        match team {
            Team::Players(_) => player_health as f32 - npc_health as f32,
            Team::NPCs(_) => npc_health as f32 - player_health as f32,
        }
    }

    pub fn is_chance_node(&self) -> bool {
        false
    }

    pub fn cur_team(&self) -> Team {
        self.get_entity(self.cur_char()).team
    }
}

impl SimState {
    fn insert_entity(&mut self, character: Character, abilities: Vec<Ability>, loc: SimCoords) {
        let team = match character {
            Character::Knight => Team::Players(0),
            Character::Orc => Team::NPCs(0),
        };

        let entity = SimEntity {
            id: SimId(self.next_id),
            character,
            abilities,
            health: character.default_health(),
            turn_movement: character.default_movement(),
            movement: character.default_movement(),
            remaining_actions: 1,
            team,
        };

        self.initiative.push(SimId(self.next_id));
        self.grid.push((loc, entity));
        self.next_id += 1;
    }

    pub fn cur_char(&self) -> SimId {
        self.initiative[0]
    }

    fn apply_use_ability(&mut self, target: SimCoords, ability: Ability) {
        let target_id = self.get_id(target).unwrap();
        let target_entity = self.get_entity_mut(target_id);
        target_entity.health = target_entity.health.saturating_sub(ability.dmg());

        if target_entity.health == 0 {
            self.remove_entity(target_id);
        }

        self.get_entity_mut(self.cur_char()).remaining_actions -= 1;
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
        let entity = self.get_entity_mut(cur_char);
        entity.movement = entity.turn_movement;
        entity.remaining_actions = 1;

        self.initiative.rotate_left(1);
    }

    pub fn get_id(&self, coords: SimCoords) -> Option<SimId> {
        self.grid
            .iter()
            .filter(|(c, _)| *c == coords)
            .map(|(_, e)| e.id)
            .next()
    }

    fn get_entity_mut(&mut self, id: SimId) -> &mut SimEntity {
        self.grid
            .iter_mut()
            .find(|(_, e)| e.id == id)
            .map(|(_, e)| e)
            .unwrap()
    }

    fn get_entity(&self, id: SimId) -> &SimEntity {
        self.grid
            .iter()
            .find(|(_, e)| e.id == id)
            .map(|(_, e)| e)
            .unwrap()
    }

    fn remove_entity(&mut self, id: SimId) {
        self.grid.retain(|(_, x)| x.id != id);
        self.initiative.retain(|x| *x != id);
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

    /// Get all empty cells within range of loc
    /// includes loc if it is empty
    fn populated_cells(&self, target: SimCoords, radius: usize) -> Vec<SimCoords> {
        self.grid
            .iter()
            .map(|x| x.0)
            .filter(|loc| loc.dist(&target) <= radius)
            .collect_vec()
    }
}

impl Character {
    fn default_movement(&self) -> usize {
        match self {
            Character::Knight => 4,
            Character::Orc => 4,
        }
    }

    fn default_health(&self) -> usize {
        match self {
            Character::Knight => 15,
            Character::Orc => 10,
        }
    }
}

impl Ability {
    pub fn range(&self) -> usize {
        match self {
            Ability::MeleeAttack => 1,
            Ability::BowAttack { range } => *range,
        }
    }

    pub fn dmg(&self) -> usize {
        match self {
            Ability::MeleeAttack => 5,
            Ability::BowAttack { range: _ } => 2,
        }
    }
}

impl Display for Ability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Ability::MeleeAttack => f.write_str("Melee"),
            Ability::BowAttack { range: _ } => f.write_str("Bow"),
        }
    }
}
