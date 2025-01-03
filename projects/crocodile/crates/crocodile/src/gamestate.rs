use std::{
    fmt::Debug,
    fmt::Display,
    ops::{Add, Sub},
};

use bevy::prelude::{Component, Resource};
use clone_from::CloneFrom;
use itertools::{Itertools, Product};
use petgraph::algo::{has_path_connecting, DfsSpace};

use crate::{sim::info::insert_space_marine_unit, ui::character::ModelSprite};

const WORLD_SIZE: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Team {
    Players,
    #[default]
    NPCs,
}

#[derive(Default, PartialEq, Eq, Clone)]
pub enum Phase {
    #[default]
    Command,
    Movement,
    Shooting,
    Charge,
    Fight,
}

#[derive(Resource, PartialEq, Clone)]
pub struct SimState {
    generation: u16,
    next_id: usize,
    queued_results: Vec<ActionResult>,
    applied_results: Vec<AppliedActionResult>,
    initiative: Vec<Team>,
    /// Location of each entity, indexed by entity id
    locations: Vec<Option<SimCoords>>,
    models: Vec<Model>,
    phase: Phase,
    /// Track if start of an entities turn, used to optimize AI search caching
    is_start_of_turn: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Action {
    #[default]
    EndTurn,
    Move {
        id: SimId,
        from: SimCoords,
        to: SimCoords,
    },
    /// Remove a model due to lack of unit coherency
    RemoveModel { id: SimId },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionResult {
    Move {
        id: SimId,
        from: SimCoords,
        to: SimCoords,
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
    RemoveModel {
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
            Action::Move { id, from, to } => {
                f.write_fmt(format_args!("Moving {:?}: from {:?} to {:?}", id, from, to))
            }
            Action::RemoveModel { id } => f.write_fmt(format_args!("Removing unit: {:?}", id)),
        }
    }
}

/// Represents a 40k style model
#[derive(CloneFrom, Hash, Debug, PartialEq)]
struct Model {
    unit: u8,
    id: SimId,
    is_destroyed: bool,
    turn_movement: u8,
    movement: u8,
    pub sprite: ModelSprite,
    cur_wound: u8,
    max_wound: u8,
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
            initiative: vec![Team::Players, Team::NPCs],
            is_start_of_turn: true,
            locations: Vec::new(),
            models: Vec::new(),
            queued_results: Vec::new(),
            applied_results: Vec::new(),
            generation: 0,
            phase: Phase::Movement,
        }
    }
}

impl Default for SimState {
    fn default() -> Self {
        let mut state = SimState::new();
        insert_space_marine_unit(&mut state, sc(5, 10), Team::Players, 0, 10);
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
            Action::Move { id, from, to } => self.generate_results_move_model(id, from, to),
            Action::RemoveModel { id } => self.generate_results_remove_model(id),
        }

        self.apply_queued_results();
        self.generation += 1;
    }

    pub fn legal_actions(&self, actions: &mut Vec<Action>) {
        actions.clear();
        use Action::*;

        let coherency = self.unit_coherency();

        if coherency.iter().filter(|x| !x.1).count() == 0 {
            actions.push(Action::EndTurn);
        }

        coherency
            .into_iter()
            .filter(|x| !x.1)
            .for_each(|x| actions.push(Action::RemoveModel { id: x.0 }));

        let cur_team = self.cur_team();
        for model in self.models.iter().filter(|m| m.team == cur_team) {
            if model.movement > 0 {
                let model_loc = self.get_loc(model.id).unwrap();
                for l in CoordIterator::new(model_loc, model.movement, 1) {
                    if !self.is_populated(&l) {
                        actions.push(Move {
                            id: model.id,
                            from: model_loc,
                            to: l,
                        });
                    }
                }
            }
        }
    }

    /// Determine if the sim is in a terminal gamestate where all player characters or
    /// all npcs are dead
    pub fn is_terminal(&self) -> bool {
        let mut count_players = 0;
        let mut count_npcs = 0;
        for entity in self.models.iter() {
            if entity.is_destroyed {
                continue;
            }
            match entity.team {
                Team::Players => count_players += 1,
                Team::NPCs => count_npcs += 1,
            };
        }
        count_players == 0 || count_npcs == 0
    }

    pub fn evaluate(&self, team: Team) -> i32 {
        const WIN_VALUE: i32 = 0; //  1000.0;
                                  // todo: add score component for entity count

        // TODO: include wounds in this? Easier to differentiate
        let mut player_models = 0;
        let mut npc_models = 0;
        for entity in self.models.iter().filter(|e| !e.is_destroyed) {
            match entity.team {
                Team::Players => player_models += 1,
                Team::NPCs => npc_models += 1,
            }
        }

        let model_score = match team {
            Team::Players => player_models - npc_models,
            Team::NPCs => npc_models - player_models,
        };

        let win_score = match (team, player_models, npc_models) {
            (Team::Players, 0, _) => -WIN_VALUE,
            (Team::Players, _, 0) => WIN_VALUE,
            (Team::NPCs, 0, _) => WIN_VALUE,
            (Team::NPCs, _, 0) => -WIN_VALUE,
            (_, _, _) => 0,
        };

        model_score + win_score
    }

    pub fn is_chance_node(&self) -> bool {
        false
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
                ActionResult::Move {
                    from: start,
                    to: _,
                    id,
                } => {
                    self.locations[id.0] = Some(start);
                }

                ActionResult::SpendMovement { id, amount } => {
                    self.models[id.0].movement += amount;
                }
                ActionResult::EndTurn => {
                    self.initiative.rotate_right(1);
                }
                ActionResult::RestoreMovement { id, amount } => {
                    self.models[id.0].movement -= amount
                }

                ActionResult::NewTurn(x) => self.is_start_of_turn = !x,
                ActionResult::RemoveModel { id } => self.models[id.0].is_destroyed = false,
            }

            // actually remove the item from the list
            self.applied_results.pop();
        }
        // assert!(self.entities[self.initiative[0].0].health > 0)
    }

    /// Returns a list for every non-destroyed unit if it is in unit coherency or not
    pub fn unit_coherency(&self) -> Vec<(SimId, bool)> {
        const SWARM_MODEL_COUNT: usize = 7;
        const NEIGHBORS_NORMAL: usize = 1;
        const NEIGHBORS_SWARM: usize = 2;

        let units = self.models.iter().map(|m| m.unit).unique();
        let mut results = Vec::new();

        for unit in units {
            let mut is_coherent = true;
            let mut unit_size = 0;

            // We create a graph to represent the models in a unit
            let mut edges = Vec::new();
            for unit_model in self
                .models
                .iter()
                .filter(|m| m.unit == unit)
                .filter(|m| !m.is_destroyed)
            {
                let unit_loc = self.get_loc(unit_model.id).unwrap();
                unit_size += 1;
                for neighbor_id in CoordIterator::new(unit_loc, 1, 1)
                    .filter_map(|l| self.get_id(l))
                    .filter(|id| self.get_entity(*id).unit == unit)
                {
                    edges.push((unit_model.id.0 as u32, neighbor_id.0 as u32));
                }
            }

            // handle case where there are no edges, but more than 1 unit
            if edges.is_empty() && unit_size > 1 {
                is_coherent = false;
            }

            let g = petgraph::graph::UnGraph::<bool, ()>::from_edges(&edges);

            // First check that each model has enough neighbors
            let required_neighbors = if unit_size == 1 {
                0 // no neighbors if only 1 model
            } else if unit_size >= SWARM_MODEL_COUNT {
                NEIGHBORS_SWARM
            } else {
                NEIGHBORS_NORMAL
            };
            if !g
                .node_indices()
                // Divide by 2 since edges are being counted twice
                .all(|n| g.neighbors(n).count() / 2 >= required_neighbors)
            {
                is_coherent = false;
            }

            // Second we check that the graph is fully connected, i.e. aren't two separate groups of units
            if let Some(node) = g.node_indices().next() {
                let mut space = DfsSpace::new(&g);
                if !g
                    .node_indices()
                    .all(|n2| has_path_connecting(&g, node, n2, Some(&mut space)))
                {
                    is_coherent = false;
                }
            }

            self.models
                .iter()
                .filter(|m| m.unit == unit && !m.is_destroyed)
                .for_each(|m| results.push((m.id, is_coherent)));
        }

        results
    }
}

impl SimState {
    /// Apply all of the queued results
    fn apply_queued_results(&mut self) {
        let generation = self.generation;
        // apply them in the order they were added
        self.queued_results.reverse();

        while let Some(result) = self.queued_results.pop() {
            match result {
                ActionResult::Move {
                    from: _,
                    to: end,
                    id,
                } => {
                    self.locations[id.0] = Some(end);
                }

                ActionResult::SpendMovement { id, amount } => {
                    self.models[id.0].movement -= amount;
                }
                ActionResult::EndTurn => {
                    self.initiative.rotate_left(1);
                }
                ActionResult::RestoreMovement { id, amount } => {
                    self.models[id.0].movement += amount;
                }

                ActionResult::NewTurn(x) => self.is_start_of_turn = x,
                ActionResult::RemoveModel { id } => self.models[id.0].is_destroyed = true,
            }

            self.applied_results
                .push(AppliedActionResult { result, generation })
        }
    }

    pub fn insert_model(
        &mut self,
        sprite: ModelSprite,
        loc: SimCoords,
        team: Team,
        unit: u8,
        movement: u8,
        wound: u8,
    ) {
        let entity = Model {
            id: SimId(self.next_id),
            turn_movement: movement,
            movement,
            remaining_actions: 1,
            team,
            unit,
            is_destroyed: false,
            sprite,
            cur_wound: wound,
            max_wound: wound,
        };

        self.models.push(entity);
        self.locations.push(Some(loc));
        self.next_id += 1;
    }

    pub fn cur_team(&self) -> Team {
        self.initiative[0]
    }

    fn generate_results_move_model(&mut self, id: SimId, from: SimCoords, to: SimCoords) {
        let distance = to.dist(&from);

        self.queued_results
            .push(ActionResult::Move { from, to, id });
        self.queued_results.push(ActionResult::SpendMovement {
            id,
            amount: distance as u8,
        });
    }

    fn generate_results_remove_model(&mut self, id: SimId) {
        self.queued_results.push(ActionResult::RemoveModel { id });
    }

    fn generate_results_end_turn(&mut self) {
        let cur_team = self.cur_team();
        for model in self.models.iter().filter(|m| m.team == cur_team) {
            let movement_restore = model.turn_movement - model.movement;
            if movement_restore > 0 {
                self.queued_results.push(ActionResult::RestoreMovement {
                    id: model.id,
                    amount: movement_restore,
                });
            }
        }

        // if !self.is_start_of_turn {
        //     self.queued_results.push(ActionResult::NewTurn(true))
        // }

        // Needs to be the last item to process, changes the current player
        self.queued_results.push(ActionResult::EndTurn);
    }

    pub fn get_id(&self, coords: SimCoords) -> Option<SimId> {
        self.locations
            .iter()
            .enumerate()
            .filter(|(_, &c)| c == Some(coords))
            .map(|(id, _)| SimId(id))
            .next()
    }

    fn get_entity(&self, id: SimId) -> &Model {
        &self.models[id.0]
    }

    pub fn sprites(&self) -> Vec<(SimId, SimCoords, ModelSprite)> {
        self.models
            .iter()
            .zip(self.locations.iter())
            .filter(|(_, l)| l.is_some())
            .map(|(e, l)| (e.id, l.unwrap(), e.sprite))
            .collect_vec()
    }

    pub fn get_loc(&self, id: SimId) -> Option<SimCoords> {
        self.locations[id.0]
    }

    fn is_populated(&self, target: &SimCoords) -> bool {
        self.locations.iter().flatten().any(|x| x == target)
    }

    pub fn health(&self, id: &SimId) -> Option<u8> {
        self.models.get(id.0).map(|x| x.cur_wound)
    }

    pub fn max_health(&self, id: &SimId) -> Option<u8> {
        self.models.get(id.0).map(|x| x.max_wound)
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
        self.models.hash(state);
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
            .field("entities", &self.models)
            .field("is_start_of_turn", &self.is_start_of_turn)
            .finish()
    }
}

#[cfg(test)]
mod tests {

    use rand::{rngs::StdRng, SeedableRng};

    use super::*;

    #[test]
    fn test_unit_coherency() {
        // Single model units are coherent
        let mut gs = SimState::new();
        insert_space_marine_unit(&mut gs, sc(1, 10), Team::Players, 0, 1);
        assert!(gs.unit_coherency().iter().all(|x| x.1));

        // Models in a straight line don't have coherency as swarms
        let mut gs = SimState::new();
        insert_space_marine_unit(&mut gs, sc(1, 10), Team::Players, 0, 10);
        assert!(!gs.unit_coherency().iter().all(|x| x.1));

        // But non-swarm units will
        let mut gs = SimState::new();
        insert_space_marine_unit(&mut gs, sc(1, 10), Team::Players, 0, 5);
        assert!(gs.unit_coherency().iter().all(|x| x.1));

        // Non-swarm aren't coherent with a gap
        let mut gs = SimState::new();
        insert_space_marine_unit(&mut gs, sc(1, 10), Team::Players, 0, 1);
        insert_space_marine_unit(&mut gs, sc(3, 10), Team::Players, 0, 1);
        assert!(
            !gs.unit_coherency().iter().all(|x| x.1),
            "Non-swarm aren't coherent with a gap"
        );

        // Swarm are coherent in a rectangle
        let mut gs = SimState::new();
        for i in 0..2 {
            insert_space_marine_unit(&mut gs, sc(1, 10 + i), Team::Players, 0, 1);
            insert_space_marine_unit(&mut gs, sc(2, 10 + i), Team::Players, 0, 1);
        }
        assert!(gs.unit_coherency().iter().all(|x| x.1));

        // enemy units don't count for coherency
        let mut gs = SimState::new();
        insert_space_marine_unit(&mut gs, sc(1, 10), Team::Players, 0, 1);
        insert_space_marine_unit(&mut gs, sc(2, 10), Team::NPCs, 1, 1);
        insert_space_marine_unit(&mut gs, sc(3, 10), Team::Players, 0, 1);
        assert_eq!(gs.unit_coherency().iter().filter(|x| !x.1).count(), 2);

        // All units in a unit must have a path between them, e.g. can't have two groups
        let mut gs = SimState::new();
        insert_space_marine_unit(&mut gs, sc(1, 10), Team::Players, 0, 1);
        insert_space_marine_unit(&mut gs, sc(2, 10), Team::Players, 0, 1);
        insert_space_marine_unit(&mut gs, sc(1, 12), Team::Players, 0, 1);
        insert_space_marine_unit(&mut gs, sc(2, 12), Team::Players, 0, 1);
        assert!(!gs.unit_coherency().iter().all(|x| x.1));
        let mut actions = Vec::new();
        gs.legal_actions(&mut actions);
        assert!(
            !actions.contains(&Action::EndTurn),
            "Can't end turn when not in unit coherency"
        )
    }

    #[test]
    fn test_move() {
        todo!()
    }

    #[test]
    fn test_undo() {
        let mut start_state = SimState::new();
        insert_space_marine_unit(&mut start_state, sc(1, 10), Team::Players, 0, 10);
        insert_space_marine_unit(&mut start_state, sc(1, 15), Team::NPCs, 1, 10);

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
                    "failed to undo index {}: {:?}\n{:?}",
                    index,
                    a,
                    diff_state.diff()
                );
                state.apply(a);
                index += 1;
            }
        }
    }
}
