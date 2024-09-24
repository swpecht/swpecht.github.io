use std::{
    fmt::Debug,
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

#[derive(Debug, Clone, Hash, PartialEq)]
pub struct Character {
    pub sprite: CharacterSprite,
    stats: Stats,
}

#[derive(Debug, Deserialize, Clone, Copy, Hash, PartialEq, Eq)]
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

#[derive(Resource, CloneFrom, PartialEq)]
pub struct SimState {
    generation: u16,
    next_id: usize,
    queued_results: Vec<ActionResult>,
    applied_results: Vec<AppliedActionResult>,
    initiative: Vec<SimId>, // order of players
    /// Location of each entity, indexed by entity id
    locations: Vec<Option<SimCoords>>,
    entities: Vec<SimEntity>,
    /// Track if start of an entities turn, used to optimize AI search caching
    is_start_of_turn: bool,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionResult {
    Move {
        id: SimId,
        start: SimCoords,
        end: SimCoords,
    },

    MeleeAttack {
        id: SimId,
        target: SimCoords,
    },

    Arrow {
        from: SimCoords,
        to: SimCoords,
    },

    Damage {
        id: SimId,
        amount: u8,
    },
    /// A special action result that is computed after damage is done
    RemoveEntity {
        loc: SimCoords,
        id: SimId,
    },
    SpendActionPoint {
        id: SimId,
    },
    SpendMovement {
        id: SimId,
        amount: u8,
    },
    // Items for reseting at the end of a turn
    /// This only ends the turn, it doesn't do anything to reset, that must be
    /// done by using "restore actions"
    EndTurn,
    /// Restore movement to an entity, often used at the end of a turn to return to full amounts
    RestoreMovement {
        id: SimId,
        amount: u8,
    },
    RestoreActionPoint {
        id: SimId,
    },

    // Items to control gamestate for optimizations
    NewTurn(bool),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AppliedActionResult {
    result: ActionResult,
    /// Track the turn when the result was applied
    generation: u16,
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

#[derive(CloneFrom, Hash, Debug, PartialEq)]
struct SimEntity {
    health: u8,
    id: SimId,
    turn_movement: u8,
    movement: u8,
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
            locations: Vec::new(),
            entities: Vec::new(),
            queued_results: Vec::new(),
            applied_results: Vec::new(),
            generation: 0,
        }
    }
}

impl Default for SimState {
    fn default() -> Self {
        let mut state = SimState::new();

        state.insert_prebuilt(PreBuiltCharacter::HumanSoldier, sc(0, 9), Team::Players(0));
        state.insert_prebuilt(PreBuiltCharacter::HumanSoldier, sc(0, 8), Team::Players(0));

        state.insert_prebuilt(PreBuiltCharacter::Skeleton, sc(5, 10), Team::NPCs(0));
        state.insert_prebuilt(PreBuiltCharacter::Skeleton, sc(4, 10), Team::NPCs(0));
        state.insert_prebuilt(PreBuiltCharacter::Skeleton, sc(6, 10), Team::NPCs(0));
        state.insert_prebuilt(PreBuiltCharacter::Orc, sc(7, 10), Team::NPCs(0));
        state.insert_prebuilt(PreBuiltCharacter::Orc, sc(7, 11), Team::NPCs(0));
        state.insert_prebuilt(PreBuiltCharacter::Orc, sc(8, 11), Team::NPCs(0));

        state
    }
}

impl SimState {
    pub fn apply(&mut self, action: Action) {
        assert_eq!(self.queued_results.len(), 0); // all queued results should have been applied

        if self.is_start_of_turn {
            self.queued_results.push(ActionResult::NewTurn(false));
        }

        match action {
            Action::EndTurn => self.generate_results_end_turn(),
            Action::Move { target } => self.generate_results_move_entity(target),
            Action::UseAbility { target, ability } => {
                self.generate_results_use_ability(target, ability)
            }
        }

        self.apply_queued_results();
        self.generation += 1;
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

        if cur_entity.movement > 0 {
            // only allow moving 1 tile
            for l in CoordIterator::new(cur_loc, 1, 1) {
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
        for entity in self.entities.iter() {
            if entity.health == 0 {
                continue;
            }
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
        for entity in self.entities.iter() {
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
    pub fn undo(&mut self) {
        if self.generation == 0 {
            panic!("tried to undo on generation 0");
        }

        self.generation -= 1;

        while let Some(result) = self.applied_results.last()
            && result.generation == self.generation
        {
            match result.result {
                ActionResult::Move { start, end: _, id } => {
                    self.locations[id.0] = Some(start);
                }
                ActionResult::Damage { id, amount } => {
                    self.entities[id.0].health += amount;
                }
                ActionResult::RemoveEntity { loc, id } => self.locations[id.0] = Some(loc),
                ActionResult::SpendActionPoint { id } => self.entities[id.0].remaining_actions += 1,
                ActionResult::SpendMovement { id, amount } => {
                    self.entities[id.0].movement += amount;
                }
                ActionResult::EndTurn => {
                    loop {
                        self.initiative.rotate_right(1);
                        // keep rotating through initiative until find unit with health
                        if self.entities[self.initiative[0].0].health > 0 {
                            break;
                        }
                    }
                }
                ActionResult::RestoreMovement { id, amount } => {
                    self.entities[id.0].movement -= amount
                }
                ActionResult::RestoreActionPoint { id } => {
                    self.entities[id.0].remaining_actions -= 1
                }
                ActionResult::NewTurn(x) => self.is_start_of_turn = !x,

                // no undo needed for visual results
                ActionResult::MeleeAttack { id: _, target: _ } => {}
                ActionResult::Arrow { from: _, to: _ } => {}
            }

            // actually remove the item from the list
            self.applied_results.pop();
        }
        // assert!(self.entities[self.initiative[0].0].health > 0)
    }
}

impl SimState {
    /// Apply all of the queued results
    fn apply_queued_results(&mut self) {
        let generation = self.generation;

        while let Some(result) = self.queued_results.pop() {
            match result {
                ActionResult::Move { start: _, end, id } => {
                    self.locations[id.0] = Some(end);
                }
                ActionResult::Damage { id, amount } => {
                    self.entities[id.0].health -= amount;
                }
                ActionResult::RemoveEntity { loc: _, id } => {
                    self.locations[id.0] = None;
                    // Here we don't actually remove the entity, TBD if this creates issues doen the line
                    // but this means we don't need to do anything to restore it other than set location
                    // self.entities[id.0] = None;
                }
                ActionResult::SpendActionPoint { id } => {
                    self.entities[id.0].remaining_actions -= 1;
                }
                ActionResult::SpendMovement { id, amount } => {
                    self.entities[id.0].movement -= amount;
                }
                ActionResult::EndTurn => {
                    loop {
                        self.initiative.rotate_left(1);
                        // keep rotating through initiative until find unit with health
                        if self.entities[self.initiative[0].0].health > 0 {
                            break;
                        }
                    }
                }
                ActionResult::RestoreMovement { id, amount } => {
                    self.entities[id.0].movement += amount;
                }
                ActionResult::RestoreActionPoint { id } => {
                    self.entities[id.0].remaining_actions += 1;
                }
                ActionResult::NewTurn(x) => self.is_start_of_turn = x,

                // no update needed for visual result
                ActionResult::MeleeAttack { id: _, target: _ } => {}
                ActionResult::Arrow { from: _, to: _ } => {}
            }

            self.applied_results
                .push(AppliedActionResult { result, generation })
        }
    }

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
        self.entities.push(entity);
        self.locations.push(Some(loc));
        self.next_id += 1;
    }

    pub fn cur_char(&self) -> SimId {
        self.initiative[0]
    }

    fn generate_results_use_ability(&mut self, target: SimCoords, ability: Ability) {
        let target_id = self.get_id(target).unwrap_or_else(|| {
            panic!(
                "failed to find entity at: {:?}, valid locations are: {:?}",
                target, self.locations
            )
        });

        let remaining_health = self.get_entity(target_id).health;
        let expected_dmg = self.calculate_dmg(target_id, &ability, remaining_health);

        self.queued_results.push(ActionResult::SpendActionPoint {
            id: self.cur_char(),
        });
        self.queued_results.push(ActionResult::Damage {
            id: target_id,
            amount: expected_dmg,
        });

        if self.get_entity(target_id).health <= expected_dmg {
            self.queued_results.push(ActionResult::RemoveEntity {
                loc: target,
                id: target_id,
            });
        }

        let cur_loc = self.locations[self.cur_char().0].unwrap();
        use Ability::*;
        match ability {
            MeleeAttack | Ram | Longsword | Shortsword | GreatAxe => {
                self.queued_results.push(ActionResult::MeleeAttack {
                    id: self.cur_char(),
                    target,
                })
            }
            BowAttack | LightCrossbow | Shortbow | Javelin => {
                self.queued_results.push(ActionResult::Arrow {
                    from: cur_loc,
                    to: target,
                })
            }
            Charge => {
                // TODO: implement check to ensure only moving to the closest square and not moving through people
                let closest = CoordIterator::new(target, 1, 1)
                    .filter(|x| !self.is_populated(x))
                    .min_by_key(|x| x.dist(&cur_loc));
                self.queued_results.push(ActionResult::Move {
                    id: self.cur_char(),
                    start: cur_loc,
                    end: closest.expect("no empty squar found for move"),
                });
            }
        }
    }

    fn generate_results_move_entity(&mut self, target: SimCoords) {
        let id = self.initiative[0].0;
        let start = self.locations[id].expect("trying to move deleted entity");
        let distance = target.dist(&start);

        // implement attack of opportunity
        let mut opp_atk_dmg = 0;
        for neighbor in CoordIterator::new(start, 1, 1) {
            if let Some(attacker) = self.locations.iter().position(|x| x == &Some(neighbor)) {
                if self.entities[attacker].team == self.entities[id].team {
                    continue;
                }
                self.queued_results.push(ActionResult::MeleeAttack {
                    id: SimId(attacker),
                    target: start,
                });
                let dmg = self.calculate_dmg(
                    SimId(id),
                    &Ability::Longsword,
                    self.entities[id].health - opp_atk_dmg,
                );
                self.queued_results.push(ActionResult::Damage {
                    id: SimId(id),
                    amount: dmg,
                });
                // track total dmg so we don't overkill
                opp_atk_dmg += dmg;
            }
        }

        self.queued_results.push(ActionResult::Move {
            start,
            end: target,
            id: self.initiative[0],
        });
        self.queued_results.push(ActionResult::SpendMovement {
            id: SimId(id),
            amount: distance as u8,
        });
    }

    fn generate_results_end_turn(&mut self) {
        let cur_char = self.cur_char();
        let entity = self.get_entity(cur_char);
        let amount = entity.turn_movement - entity.movement;

        // only restore action points if spent them
        if entity.remaining_actions == 0 {
            self.queued_results
                .push(ActionResult::RestoreActionPoint { id: cur_char });
        }

        self.queued_results.push(ActionResult::RestoreMovement {
            id: cur_char,
            amount,
        });

        if !self.is_start_of_turn {
            self.queued_results.push(ActionResult::NewTurn(true))
        }

        // Needs to be the last item to process, changes the current player
        self.queued_results.push(ActionResult::EndTurn);
    }

    /// Calcualte dmg accounting for health remaining from un processed actions
    fn calculate_dmg(&self, target: SimId, ability: &Ability, remaining_health: u8) -> u8 {
        let target_entity = self.get_entity(target);

        let target_ac = target_entity.character.stats.ac;
        let to_hit = ability.to_hit();

        // 1 out of 20 times we crit, auto hit and 2x dmg
        let crit_dmg = 1.0 / 20.0 * (ability.dmg() * 2) as f32;
        // for the other 19 possible rolls, only the ones > ac - to_hit actually hit
        // add 1 since if we match ac we hit
        let chance_to_hit_no_crit = (19 - target_ac + to_hit + 1) as f32 / 19.0;
        let expected_dmg = chance_to_hit_no_crit * ability.dmg() as f32 + crit_dmg;
        (expected_dmg as u8).min(remaining_health)
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
        &mut self.entities[id.0]
    }

    fn get_entity(&self, id: SimId) -> &SimEntity {
        &self.entities[id.0]
    }

    pub fn characters(&self) -> Vec<(SimId, SimCoords, Character)> {
        self.entities
            .iter()
            .zip(self.locations.iter())
            .filter(|(_, l)| l.is_some())
            .map(|(e, l)| (e.id, l.unwrap(), e.character.clone()))
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
        self.entities.get(id.0).map(|x| x.health)
    }

    pub fn max_health(&self, id: &SimId) -> Option<u8> {
        self.entities
            .get(id.0)
            .map(|x| x.character.default_health())
    }

    /// Returns all of the action results to go from the previous state to the current one
    pub fn diff(&self) -> Vec<ActionResult> {
        self.applied_results
            .iter()
            .filter(|x| x.generation == self.generation - 1)
            .map(|x| x.result.clone())
            .collect_vec()
    }
}

impl Character {
    fn default_movement(&self) -> u8 {
        self.stats.movement
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
    fn new(middle: SimCoords, max_range: u8, min_range: u8) -> Self {
        let min_x = middle.x.saturating_sub(max_range as usize);
        let min_y = middle.y.saturating_sub(max_range as usize);
        let max_x = (middle.x + max_range as usize).min(WORLD_SIZE);
        let max_y = (middle.y + max_range as usize).min(WORLD_SIZE);

        let raw_iterator = (min_x..max_x + 1).cartesian_product(min_y..max_y + 1);

        Self {
            max_range: max_range as usize,
            middle,
            raw_iterator,
            min_range: min_range as usize,
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

impl std::hash::Hash for SimState {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.next_id.hash(state);
        self.initiative.hash(state);
        self.locations.hash(state);
        self.entities.hash(state);
        self.is_start_of_turn.hash(state);
    }
}

impl Debug for SimState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SimState")
            .field("generation", &self.generation)
            .field("next_id", &self.next_id)
            .field("queued_results", &self.queued_results)
            .field("initiative", &self.initiative)
            .field("locations", &self.locations)
            .field("entities", &self.entities)
            .field("is_start_of_turn", &self.is_start_of_turn)
            .finish()
    }
}

#[cfg(test)]
mod tests {

    use rand::{rngs::StdRng, SeedableRng};

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
        assert_eq!(PreBuiltCharacter::Skeleton.stats().ac, 13);
        assert_eq!(Ability::MeleeAttack.dmg(), 5);
        assert_eq!(PreBuiltCharacter::Skeleton.stats().health, 13);

        // expected crit damage = 5% * 5 * 2 = 0.5
        // rolls that hit, but no crit = 19 - 11 + 5 + 1 = 14
        // expected no crit dmg = 14 / 19 * 5 = 3.7
        // expected damage = 3.7 + 0.5 = 4.2
        // remaining health = 10 - 4 = 6
        assert_eq!(gs.get_entity(SKELETON).health, 10);

        // have the knight move
        gs.apply(Action::Move {
            target: KNIGHT_START + sc(0, 1),
        });

        let cur_loc = gs
            .get_loc(KNIGHT_ID)
            .expect("couldn't get location of knight");
        assert_eq!(cur_loc, KNIGHT_START + sc(0, 1));
    }

    #[test]
    fn test_undo() {
        let mut start_state = SimState::new();

        start_state.insert_prebuilt(
            PreBuiltCharacter::Knight,
            SimCoords { x: 8, y: 10 },
            Team::Players(0),
        );
        start_state.insert_prebuilt(
            PreBuiltCharacter::Skeleton,
            SimCoords { x: 9, y: 10 },
            Team::NPCs(0),
        );
        start_state.insert_prebuilt(
            PreBuiltCharacter::Skeleton,
            SimCoords { x: 9, y: 11 },
            Team::NPCs(0),
        );

        let mut rng: StdRng = SeedableRng::seed_from_u64(42);
        let mut actions = Vec::new();
        let mut index = 0;

        for _ in 0..1000 {
            // times to run the test
            let mut state = start_state.clone();
            for _ in 0..100 {
                // max number of generations
                if state.is_terminal() {
                    break;
                }

                let undo_state = state.clone();
                state.legal_actions(&mut actions);

                use rand::prelude::SliceRandom;
                let a = *actions.choose(&mut rng).unwrap();

                state.apply(a);
                let diff_state = state.clone();
                state.undo();
                assert_eq!(
                    state,
                    undo_state,
                    "failed to undo index {}: {:?}\n{:?}\n{:#?}",
                    index,
                    a,
                    diff_state.diff(),
                    state
                );
                state.apply(a);
                index += 1;
            }
        }
    }

    #[test]
    fn test_undo_dead() {
        let mut state = SimState::new();

        state.insert_prebuilt(
            PreBuiltCharacter::Knight,
            SimCoords { x: 8, y: 10 },
            Team::Players(0),
        );
        state.insert_prebuilt(
            PreBuiltCharacter::Skeleton,
            SimCoords { x: 9, y: 10 },
            Team::NPCs(0),
        );

        state.insert_prebuilt(
            PreBuiltCharacter::Skeleton,
            SimCoords { x: 9, y: 10 },
            Team::NPCs(0),
        );

        assert_eq!(state.cur_char().0, 0);
        state.entities[1].health = 0;
        state.apply(Action::EndTurn);
        // skip entity 1 since no life
        assert_eq!(state.cur_char().0, 2);
        state.undo();
        // undo should also take us back to 0 since 1 is skipped in the other direction
        assert_eq!(state.cur_char().0, 0);
    }
}
