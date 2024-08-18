use std::{
    fmt::Display,
    ops::{Add, Sub},
};

use bevy::prelude::{Component, Resource};
use clone_from::CloneFrom;
use itertools::{Itertools, Product};
use serde::Deserialize;
use tinyvec::ArrayVec;

use crate::{
    sim::info::{Ability, PreBuiltCharacter},
    ui::sprite::CharacterSprite,
};

const WORLD_SIZE: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Team {
    Players(usize),
    NPCs(usize),
}

#[derive(Debug, Clone, Hash)]
pub struct Character {
    pub sprite: CharacterSprite,
    stats: Stats,
}

#[derive(Debug, Deserialize, Clone, Copy, Hash)]
pub struct Stats {
    pub health: u8,
    pub str: u8,
    pub dex: u8,
    pub con: u8,
    pub int: u8,
    pub wis: u8,
    pub cha: u8,
    pub ac: u8,
    pub movement: u8,
}

#[derive(Resource, Hash, CloneFrom)]
pub struct SimState {
    next_id: usize,
    initiative: Vec<SimId>, // order of players
    locations: Vec<Option<SimCoords>>,
    entities: Vec<Option<SimEntity>>,
    /// Track if start of an entities turn, used to optimize AI search caching
    is_start_of_turn: bool,
    /// Don't allow move action after another move action
    can_move: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Action {
    #[default]
    EndTurn,
    UseAbility {
        target: SimCoords,
        ability: Ability,
    },
    Move {
        target: SimCoords,
    },
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

#[derive(CloneFrom, Hash)]
struct SimEntity {
    health: u8,
    id: SimId,
    turn_movement: usize,
    movement: usize,
    character: Character,
    abilities: ArrayVec<[Ability; 5]>,
    remaining_actions: usize,
    team: Team,
}

#[derive(Clone, Copy, Debug, Default, Component, PartialEq, Eq, Hash)]
pub struct SimId(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
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

pub fn sc(x: usize, y: usize) -> SimCoords {
    SimCoords { x, y }
}

impl SimState {
    pub fn new() -> Self {
        Self {
            next_id: 0,
            initiative: Vec::new(),
            is_start_of_turn: true,
            can_move: true,
            locations: Vec::new(),
            entities: Vec::new(),
        }
    }
}

impl Default for SimState {
    fn default() -> Self {
        let mut state = SimState::new();

        state.insert_prebuilt(PreBuiltCharacter::HumanSoldier, sc(0, 9), Team::Players(0));
        state.insert_prebuilt(PreBuiltCharacter::HumanSoldier, sc(0, 8), Team::Players(0));

        state.insert_prebuilt(PreBuiltCharacter::GiantGoat, sc(5, 10), Team::NPCs(0));
        state.insert_prebuilt(PreBuiltCharacter::GiantGoat, sc(4, 10), Team::NPCs(0));
        state.insert_prebuilt(PreBuiltCharacter::GiantGoat, sc(6, 10), Team::NPCs(0));
        state.insert_prebuilt(PreBuiltCharacter::GiantGoat, sc(7, 10), Team::NPCs(0));

        state
    }
}

impl SimState {
    pub fn apply(&mut self, action: Action) {
        self.is_start_of_turn = false;

        match action {
            Action::EndTurn => self.apply_end_turn(),
            Action::Move { target } => self.apply_move_entity(target),
            Action::UseAbility { target, ability } => self.apply_use_ability(target, ability),
        }
    }

    pub fn legal_actions(&self, actions: &mut Vec<Action>) {
        actions.clear();
        use Action::*;
        actions.push(Action::EndTurn);

        let cur_loc = self.get_loc(self.cur_char()).unwrap();
        let cur_entity = self.get_entity(self.cur_char());

        // todo: come up with easy iterator to go through all the world cells within a given range rather than iterating everything

        // We check push the ability actions first so that these are preferentially explored
        // by the ai tree search. This avoids the units moving around without purpose to end in the same spot to attack
        if cur_entity.remaining_actions > 0 {
            for ability in cur_entity.abilities.iter() {
                for l in CoordIterator::new(cur_loc, ability.max_range(), ability.min_range()) {
                    if self.is_populated(&l) && l != cur_loc {
                        actions.push(Action::UseAbility {
                            target: l,
                            ability: *ability,
                        });
                    }
                }
            }
        }

        if cur_entity.movement > 0 && self.can_move {
            for l in CoordIterator::new(cur_loc, cur_entity.movement, 1) {
                if !self.is_populated(&l) {
                    actions.push(Move { target: l });
                }
            }
        }
    }

    /// Determine if the sim is in a terminal gamestate where all player characters or
    /// all npcs are dead
    pub fn is_terminal(&self) -> bool {
        let mut count_players = 0;
        let mut count_npcs = 0;
        for entity in self.entities.iter().flatten() {
            match entity.team {
                Team::Players(_) => count_players += 1,
                Team::NPCs(_) => count_npcs += 1,
            };
        }
        count_players == 0 || count_npcs == 0
    }

    pub fn evaluate(&self, team: Team) -> i32 {
        const WIN_VALUE: i32 = 0; //  1000.0;
                                  // todo: add score component for entity count

        let mut player_health = 0;
        let mut npc_health = 0;
        for entity in self.entities.iter().flatten() {
            match entity.team {
                Team::Players(_) => player_health += entity.health,
                Team::NPCs(_) => npc_health += entity.health,
            }
        }

        let health_score = match team {
            Team::Players(_) => player_health as i32 - npc_health as i32,
            Team::NPCs(_) => npc_health as i32 - player_health as i32,
        };

        let win_score = match (team, player_health, npc_health) {
            (Team::Players(_), 0, _) => -WIN_VALUE,
            (Team::Players(_), _, 0) => WIN_VALUE,
            (Team::NPCs(_), 0, _) => WIN_VALUE,
            (Team::NPCs(_), _, 0) => -WIN_VALUE,
            (_, _, _) => 0,
        };

        health_score + win_score
    }

    pub fn is_chance_node(&self) -> bool {
        false
    }

    pub fn cur_team(&self) -> Team {
        self.get_entity(self.cur_char()).team
    }

    pub fn is_start_of_turn(&self) -> bool {
        self.is_start_of_turn
    }

    /// undo the last action
    pub fn undo(&mut self) {}
}

impl SimState {
    pub fn insert_prebuilt(&mut self, prebuilt: PreBuiltCharacter, loc: SimCoords, team: Team) {
        self.insert_entity(
            Character {
                sprite: prebuilt.sprite(),
                stats: prebuilt.stats(),
            },
            prebuilt.abilities(),
            loc,
            team,
        )
    }

    pub fn insert_entity(
        &mut self,
        character: Character,
        abilities: Vec<Ability>,
        loc: SimCoords,
        team: Team,
    ) {
        let mut ability_vec = ArrayVec::new();
        for a in abilities {
            ability_vec.push(a);
        }

        let entity = SimEntity {
            id: SimId(self.next_id),
            abilities: ability_vec,
            health: character.default_health(),
            turn_movement: character.default_movement(),
            movement: character.default_movement(),
            character,
            remaining_actions: 1,
            team,
        };

        self.initiative.push(SimId(self.next_id));
        self.entities.push(Some(entity));
        self.locations.push(Some(loc));
        self.next_id += 1;
    }

    pub fn cur_char(&self) -> SimId {
        self.initiative[0]
    }

    fn apply_use_ability(&mut self, target: SimCoords, ability: Ability) {
        let target_id = self.get_id(target).unwrap_or_else(|| {
            panic!(
                "failed to find entity at: {:?}, valid locations are: {:?}",
                target, self.locations
            )
        });
        let target_entity = self.get_entity_mut(target_id);

        let target_ac = target_entity.character.stats.ac;
        let to_hit = ability.to_hit();

        // 1 out of 20 times we crit, auto hit and 2x dmg
        let crit_dmg = 1.0 / 20.0 * (ability.dmg() * 2) as f32;
        // for the other 19 possible rolls, only the ones > ac - to_hit actually hit
        // add 1 since if we match ac we hit
        let chance_to_hit_no_crit = (19 - target_ac + to_hit + 1) as f32 / 19.0;
        let expected_dmg = chance_to_hit_no_crit * ability.dmg() as f32 + crit_dmg;

        target_entity.health = target_entity.health.saturating_sub(expected_dmg as u8);

        if target_entity.health == 0 {
            self.remove_entity(target_id);
        }

        self.get_entity_mut(self.cur_char()).remaining_actions -= 1;
        self.can_move = true;
    }

    fn apply_move_entity(&mut self, target: SimCoords) {
        let id = self.initiative[0].0;
        let start = self.locations[id].expect("trying to move deleted entity");
        let distance = target.dist(&start);

        let entity = self.entities[id]
            .as_mut()
            .expect("trying to move deleted entity");
        entity.movement -= distance;
        self.locations[id] = Some(target);

        self.can_move = false;
    }

    fn apply_end_turn(&mut self) {
        // reset movement
        let cur_char = self.cur_char();
        let entity = self.get_entity_mut(cur_char);
        entity.movement = entity.turn_movement;
        entity.remaining_actions = 1;

        self.initiative.rotate_left(1);
        self.is_start_of_turn = true;
        self.can_move = true;
    }

    pub fn get_id(&self, coords: SimCoords) -> Option<SimId> {
        self.locations
            .iter()
            .enumerate()
            .filter(|(_, &c)| c == Some(coords))
            .map(|(id, _)| SimId(id))
            .next()
    }

    fn get_entity_mut(&mut self, id: SimId) -> &mut SimEntity {
        self.entities[id.0]
            .as_mut()
            .expect("accessing deleted entity")
    }

    fn get_entity(&self, id: SimId) -> &SimEntity {
        self.entities[id.0]
            .as_ref()
            .expect("accessing deleted entity")
    }

    fn remove_entity(&mut self, id: SimId) {
        self.locations[id.0] = None;
        self.entities[id.0] = None;
        self.initiative.retain(|x| *x != id);
    }

    pub fn characters(&self) -> Vec<(SimId, SimCoords, Character)> {
        self.entities
            .iter()
            .zip(self.locations.iter())
            .filter(|(e, l)| e.is_some() && l.is_some())
            .map(|(e, l)| {
                (
                    e.as_ref().unwrap().id,
                    l.unwrap(),
                    e.as_ref().unwrap().character.clone(),
                )
            })
            .collect_vec()
    }

    pub fn get_loc(&self, id: SimId) -> Option<SimCoords> {
        self.locations[id.0]
    }

    pub fn abilities(&self) -> Vec<Ability> {
        self.get_entity(self.cur_char()).abilities.to_vec()
    }

    fn is_populated(&self, target: &SimCoords) -> bool {
        self.locations.iter().flatten().any(|x| x == target)
    }

    pub fn health(&self, id: &SimId) -> Option<u8> {
        self.entities[id.0].as_ref().map(|x| x.health)
    }

    pub fn max_health(&self, id: &SimId) -> Option<u8> {
        self.entities[id.0]
            .as_ref()
            .map(|x| x.character.default_health())
    }
}

impl Character {
    fn default_movement(&self) -> usize {
        self.stats.movement as usize
    }

    fn default_health(&self) -> u8 {
        self.stats.health
    }
}

impl Default for Team {
    fn default() -> Self {
        Team::NPCs(0)
    }
}

/// Iterator over all world coords within distance d
struct CoordIterator {
    max_range: usize,
    min_range: usize,
    middle: SimCoords,
    raw_iterator: Product<std::ops::Range<usize>, std::ops::Range<usize>>,
}

impl CoordIterator {
    fn new(middle: SimCoords, max_range: usize, min_range: usize) -> Self {
        let min_x = middle.x.saturating_sub(max_range);
        let min_y = middle.y.saturating_sub(max_range);
        let max_x = (middle.x + max_range).min(WORLD_SIZE);
        let max_y = (middle.y + max_range).min(WORLD_SIZE);

        let raw_iterator = (min_x..max_x + 1).cartesian_product(min_y..max_y + 1);

        Self {
            max_range,
            middle,
            raw_iterator,
            min_range,
        }
    }
}

impl Iterator for CoordIterator {
    type Item = SimCoords;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let cp = self.raw_iterator.next()?;
            let coord = sc(cp.0, cp.1);
            let dist = coord.dist(&self.middle);
            if dist <= self.max_range && dist >= self.min_range {
                return Some(coord);
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    const KNIGHT_ID: SimId = SimId(0);
    const SKELETON: SimId = SimId(1);
    const KNIGHT_START: SimCoords = SimCoords { x: 10, y: 10 };
    const SKELETON_START: SimCoords = SimCoords { x: 9, y: 10 };

    fn create_test_world() -> SimState {
        let mut state = SimState::new();

        state.insert_prebuilt(PreBuiltCharacter::Knight, KNIGHT_START, Team::Players(0));
        state.insert_prebuilt(PreBuiltCharacter::Skeleton, SKELETON_START, Team::NPCs(0));
        state
    }

    #[test]
    fn test_melee_move() {
        let mut gs = create_test_world();

        // have the knight attack the orc
        gs.apply(Action::UseAbility {
            target: SKELETON_START,
            ability: Ability::MeleeAttack,
        });

        assert_eq!(Ability::MeleeAttack.to_hit(), 5);
        assert_eq!(PreBuiltCharacter::Skeleton.stats().ac, 11);
        assert_eq!(Ability::MeleeAttack.dmg(), 5);
        assert_eq!(PreBuiltCharacter::Skeleton.stats().health, 10);

        // expected crit damage = 5% * 5 * 2 = 0.5
        // rolls that hit, but no crit = 19 - 11 + 5 + 1 = 14
        // expected no crit dmg = 14 / 19 * 5 = 3.7
        // expected damage = 3.7 + 0.5 = 4.2
        // remaining health = 10 - 4 = 6
        assert_eq!(gs.get_entity(SKELETON).health, 6);

        // have the knight move
        gs.apply(Action::Move {
            target: KNIGHT_START + sc(0, 1),
        });

        let cur_loc = gs
            .get_loc(KNIGHT_ID)
            .expect("couldn't get location of knight");
        assert_eq!(cur_loc, KNIGHT_START + sc(0, 1));
    }
}
