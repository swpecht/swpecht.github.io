use std::{
    fmt::Display,
    ops::{Add, Sub},
};

use bevy::prelude::{Component, Resource};
use clone_from::CloneFrom;
use itertools::{Itertools, Product};
use tinyvec::ArrayVec;

const WORLD_SIZE: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Team {
    Players(usize),
    NPCs(usize),
}

#[derive(Debug, Clone, Copy, Hash, Default)]
pub enum Character {
    Knight,
    #[default]
    Orc,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Ability {
    #[default]
    MeleeAttack,
    BowAttack {
        range: usize,
    },
}

#[derive(CloneFrom, Hash, Default)]
struct SimEntity {
    health: usize,
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

        state.insert_entity(
            Character::Knight,
            vec![Ability::MeleeAttack, Ability::BowAttack { range: 20 }],
            sc(0, 9),
        );
        state.insert_entity(Character::Orc, vec![Ability::MeleeAttack], sc(5, 10));
        state.insert_entity(Character::Orc, vec![Ability::MeleeAttack], sc(6, 10));
        state.insert_entity(Character::Orc, vec![Ability::MeleeAttack], sc(4, 10));

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
                for l in CoordIterator::new(cur_loc, ability.range()) {
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
            for l in CoordIterator::new(cur_loc, cur_entity.movement) {
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

    pub fn evaluate(&self, team: Team) -> f32 {
        const WIN_VALUE: f32 = 0.0; //  1000.0;
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
            Team::Players(_) => player_health as f32 - npc_health as f32,
            Team::NPCs(_) => npc_health as f32 - player_health as f32,
        };

        let win_score = match (team, player_health, npc_health) {
            (Team::Players(_), 0, _) => -WIN_VALUE,
            (Team::Players(_), _, 0) => WIN_VALUE,
            (Team::NPCs(_), 0, _) => WIN_VALUE,
            (Team::NPCs(_), _, 0) => -WIN_VALUE,
            (_, _, _) => 0.0,
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
    pub fn insert_entity(&mut self, character: Character, abilities: Vec<Ability>, loc: SimCoords) {
        let team = match character {
            Character::Knight => Team::Players(0),
            Character::Orc => Team::NPCs(0),
        };

        let mut ability_vec = ArrayVec::new();
        for a in abilities {
            ability_vec.push(a);
        }

        let entity = SimEntity {
            id: SimId(self.next_id),
            character,
            abilities: ability_vec,
            health: character.default_health(),
            turn_movement: character.default_movement(),
            movement: character.default_movement(),
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
        let target_id = self
            .get_id(target)
            .unwrap_or_else(|| panic!("failed to find entity at: {:?}", target));
        let target_entity = self.get_entity_mut(target_id);
        target_entity.health = target_entity.health.saturating_sub(ability.dmg());

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
                    e.as_ref().unwrap().character,
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

impl Default for Team {
    fn default() -> Self {
        Team::NPCs(0)
    }
}

/// Iterator over all world coords within distance d
struct CoordIterator {
    distance: usize,
    middle: SimCoords,
    raw_iterator: Product<std::ops::Range<usize>, std::ops::Range<usize>>,
}

impl CoordIterator {
    fn new(middle: SimCoords, distance: usize) -> Self {
        let min_x = middle.x.saturating_sub(distance);
        let min_y = middle.y.saturating_sub(distance);
        let max_x = (middle.x + distance).min(WORLD_SIZE);
        let max_y = (middle.y + distance).min(WORLD_SIZE);

        let raw_iterator = (min_x..max_x + 1).cartesian_product(min_y..max_y + 1);

        Self {
            distance,
            middle,
            raw_iterator,
        }
    }
}

impl Iterator for CoordIterator {
    type Item = SimCoords;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let cp = self.raw_iterator.next()?;
            let coord = sc(cp.0, cp.1);
            if coord.dist(&self.middle) <= self.distance {
                return Some(coord);
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    const KNIGHT_ID: SimId = SimId(0);
    const ORC_ID: SimId = SimId(1);
    const KNIGHT_START: SimCoords = SimCoords { x: 10, y: 10 };
    const ORC_START: SimCoords = SimCoords { x: 9, y: 10 };

    fn create_test_world() -> SimState {
        let mut state = SimState::new();
        state.insert_entity(Character::Knight, vec![Ability::MeleeAttack], KNIGHT_START);
        state.insert_entity(Character::Orc, vec![Ability::MeleeAttack], ORC_START);
        state
    }

    #[test]
    fn test_melee_move() {
        let mut gs = create_test_world();

        // have the knight attack the orc
        gs.apply(Action::UseAbility {
            target: ORC_START,
            ability: Ability::MeleeAttack,
        });

        assert_eq!(
            gs.get_entity(ORC_ID).health,
            Character::Orc.default_health() - Ability::MeleeAttack.dmg()
        );

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
