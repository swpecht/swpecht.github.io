use core::{option::Option::None, todo};
use std::{
    collections::{HashMap, HashSet},
    fmt::{Debug, Display},
    ops::{Add, Sub},
};

use itertools::{Itertools, Product};
use macros::{team_models, unit_models};
use petgraph::algo::{has_path_connecting, DfsSpace};
use probability::{attack_success_probs, ChanceProbabilities};

use crate::{
    info::{insert_necron_unit, insert_space_marine_unit, AttackValue, ModelStats, RangedWeapon},
    ModelSprite,
};

mod gs_debug;
mod macros;
mod probability;

const WORLD_SIZE: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Team {
    Players,
    #[default]
    NPCs,
}

pub enum UnitType {
    NewUnit,
    LastUnit,
}

#[derive(Default, PartialEq, Eq, Clone, Debug)]
pub enum Phase {
    #[default]
    Command,
    Movement,
    Shooting,
    Charge,
    Fight,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum ShootingPhase {
    SelectUnit,
    SelectTargets,
    MakeRangedAttacks,
}

#[derive(PartialEq, Clone, Debug)]
pub struct SimState {
    pub(super) generation: u16,
    pub(super) next_model_id: usize,
    pub(super) next_unit_id: u8,
    pub(super) queued_results: Vec<ActionResult>,
    pub(super) applied_results: Vec<AppliedActionResult>,
    pub(super) initiative: Vec<Team>,
    /// Location of each entity, indexed by entity id
    pub(super) locations: Vec<Option<SimCoords>>,
    pub(super) models: Vec<Model>,
    pub(super) phase: Phase,
    /// Track if start of an entities turn, used to optimize AI search caching
    pub(super) is_start_of_turn: bool,
    pub(super) pending_chance_action: Vec<Action>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Action {
    #[default]
    EndPhase,
    Move {
        id: ModelId,
        from: SimCoords,
        to: SimCoords,
    },
    Shoot {
        from: UnitId,
        to: UnitId,
        ranged_weapon: RangedWeapon,
    },
    /// Remove a model due to lack of unit coherency
    RemoveModel {
        id: ModelId,
    },
    RollResult {
        num_success: u8,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionResult {
    Move {
        id: ModelId,
        from: SimCoords,
        to: SimCoords,
    },
    SpendMovement {
        id: ModelId,
        amount: u8,
    },
    // Items for reseting at the end of a turn
    /// This only ends the turn, it doesn't do anything to reset, that must be
    /// done by using "restore actions"
    EndPhase,
    /// Restore movement to an entity, often used at the end of a turn to return to full amounts
    RestoreMovement {
        id: ModelId,
        amount: u8,
    },
    RemoveModel {
        id: ModelId,
    },
    // Items to control gamestate for optimizations
    NewTurn(bool),

    /// Requires a chance resolution before the action can be resolved
    QueueChanceNode {
        action: Action,
    },
    ResolveChanceNode {
        action: Action,
    },
    ApplyWound {
        id: ModelId,
        num_wounds: u8,
    },
    UseWeapon {
        id: ModelId,
        weapon: RangedWeapon,
    },
    ReloadWeapon {
        id: ModelId,
        weapon: RangedWeapon,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AppliedActionResult {
    result: ActionResult,
    /// Track the turn when the result was applied
    generation: u16,
}

impl Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Action::EndPhase => f.write_str("End phase"),
            Action::Move { id, from, to } => {
                f.write_fmt(format_args!("Moving {:?}: from {:?} to {:?}", id, from, to))
            }
            Action::RemoveModel { id } => f.write_fmt(format_args!("Removing unit: {:?}", id)),
            Action::Shoot {
                from: _from,
                to: _to,
                ranged_weapon: _ranged_weapon,
            } => todo!(),
            Action::RollResult { num_success } => {
                f.write_fmt(format_args!("Succeded {:?} times", num_success))
            }
        }
    }
}

impl Display for Phase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Phase::Command => f.write_str("Command phase"),
            Phase::Movement => f.write_str("Movement phase"),
            Phase::Shooting => f.write_str("Shooting phase"),
            Phase::Charge => f.write_str("Charge phase"),
            Phase::Fight => f.write_str("Fight phase"),
        }
    }
}

/// Represents a 40k style model
#[derive(Debug, PartialEq, Clone)]
pub(super) struct Model {
    unit: UnitId,
    id: ModelId,
    is_destroyed: bool,
    pub sprite: ModelSprite,
    cur_stats: ModelStats,
    base_stats: ModelStats,
    remaining_actions: usize,
    team: Team,
    available_ranged_weapons: HashSet<RangedWeapon>,
    ranged_weapons: HashSet<RangedWeapon>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelId(usize);

/// Denotes the unit a model belongs to
#[derive(Hash, Debug, PartialEq, Clone, Eq, Copy)]
pub struct UnitId(u8);

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
            next_model_id: 0,
            initiative: vec![Team::Players, Team::NPCs],
            is_start_of_turn: true,
            locations: Vec::new(),
            models: Vec::new(),
            queued_results: Vec::new(),
            applied_results: Vec::new(),
            generation: 0,
            phase: Phase::Movement,
            next_unit_id: 0,
            pending_chance_action: Vec::new(),
        }
    }
}

impl Default for SimState {
    fn default() -> Self {
        let mut gs = SimState::new();
        // insert_space_marine_unit(&mut state, sc(5, 10), Team::Players, 0, 10);
        insert_space_marine_unit(
            &mut gs,
            vec![sc(1, 10), sc(2, 10), sc(3, 10)],
            Team::Players,
        );

        insert_necron_unit(&mut gs, vec![sc(1, 15), sc(2, 15), sc(3, 15)], Team::NPCs);
        insert_necron_unit(&mut gs, vec![sc(1, 16), sc(2, 16), sc(3, 16)], Team::NPCs);
        gs
    }
}

impl SimState {
    pub fn apply(&mut self, action: Action) {
        assert_eq!(self.queued_results.len(), 0); // all queued results should have been applied

        if self.is_start_of_turn {
            self.queued_results.push(ActionResult::NewTurn(false));
        }

        match action {
            Action::EndPhase => self.generate_results_end_phase(),
            Action::Move { id, from, to } => self.generate_results_move_model(id, from, to),
            Action::RemoveModel { id } => self.generate_results_remove_model(id),
            Action::Shoot {
                from: _,
                to: _,
                ranged_weapon: _,
            } => self.generate_results_shoot(action),
            Action::RollResult { num_success } => self.generate_results_roll_result(num_success),
        }

        self.apply_queued_results();
        self.generation += 1;
    }

    pub fn legal_actions(&self, actions: &mut Vec<Action>) {
        actions.clear();
        if self.is_chance_node() {
            return;
        }

        match &self.phase {
            Phase::Command => self.legal_actions_command(actions),
            Phase::Movement => self.legal_actions_movement(actions),
            Phase::Shooting => self.legal_actions_shooting(actions),
            Phase::Charge => self.legal_actions_charge(actions),
            Phase::Fight => self.legal_actions_fight(actions),
        }
    }

    /// Return the probabilities for `ChanceOutcomes`
    pub fn chance_outcomes(&self) -> ChanceProbabilities {
        if !self.is_chance_node() {
            panic!("called chance outcomes when not a chance node")
        }

        match self.pending_chance_action.last() {
            Some(Action::Shoot {
                from,
                to,
                ranged_weapon,
            }) => self.chance_outcomes_shoot(*from, *to, *ranged_weapon),
            Some(_) => todo!(),
            None => panic!("no pending chance action"),
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
        !self.pending_chance_action.is_empty()
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
                    self.models[id.0].cur_stats.movement += amount;
                }
                ActionResult::EndPhase => {
                    if self.phase == Phase::Command {
                        self.initiative.rotate_right(1);
                    }
                    self.phase = match self.phase {
                        Phase::Command => Phase::Fight,
                        Phase::Movement => Phase::Command,
                        Phase::Shooting => Phase::Movement,
                        Phase::Charge => Phase::Shooting,
                        Phase::Fight => Phase::Charge,
                    };
                }
                ActionResult::RestoreMovement { id, amount } => {
                    self.models[id.0].cur_stats.movement -= amount
                }

                ActionResult::NewTurn(x) => self.is_start_of_turn = !x,
                ActionResult::RemoveModel { id } => self.models[id.0].is_destroyed = false,
                ActionResult::QueueChanceNode { action: _ } => {
                    self.pending_chance_action.pop();
                }
                ActionResult::ApplyWound { id, num_wounds } => {
                    self.get_model_mut(id).cur_stats.wound += num_wounds
                }
                ActionResult::ResolveChanceNode { action } => {
                    self.pending_chance_action.push(action)
                }
                ActionResult::UseWeapon { id, weapon } => {
                    self.get_model_mut(id)
                        .available_ranged_weapons
                        .insert(weapon);
                }
                ActionResult::ReloadWeapon { id, weapon } => {
                    self.get_model_mut(id)
                        .available_ranged_weapons
                        .remove(&weapon);
                }
            }

            // actually remove the item from the list
            self.applied_results.pop();
        }
        // assert!(self.entities[self.initiative[0].0].health > 0)
    }

    /// Returns a list for every non-destroyed unit if it is in unit coherency or not
    pub fn unit_coherency(&self) -> Vec<(ModelId, bool)> {
        const SWARM_MODEL_COUNT: usize = 7;
        const NEIGHBORS_NORMAL: usize = 1;
        const NEIGHBORS_SWARM: usize = 2;

        let units = self.models.iter().map(|m| m.unit).unique();
        let mut results = Vec::new();

        for unit in units {
            let mut is_coherent = true;
            let mut unit_size = 0;

            // We create a graph to represent the models in a unit
            let mut g = petgraph::graph::UnGraph::<ModelId, ()>::new_undirected();
            let mut node_lookup = HashMap::new();

            self.models
                .iter()
                .filter(|m| m.unit == unit)
                .filter(|m| !m.is_destroyed)
                .for_each(|m| {
                    let idx = g.add_node(m.id);
                    node_lookup.insert(m.id, idx);
                });

            for unit_model in unit_models!(self, unit) {
                let m1_idx = node_lookup.get(&unit_model.id).unwrap();
                let unit_loc = self.get_loc(unit_model.id).unwrap();
                unit_size += 1;
                for neighbor_id in CoordIterator::new(unit_loc, 1, 1)
                    .filter_map(|l| self.get_id(l))
                    .filter(|id| {
                        let m2 = self.get_model(*id);
                        m2.unit == unit && !m2.is_destroyed
                    })
                {
                    let m2_idx = node_lookup.get(&neighbor_id).unwrap();
                    g.add_edge(*m1_idx, *m2_idx, ());
                }
            }

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

    pub fn phase(&self) -> Phase {
        self.phase.clone()
    }

    pub fn set_phase(&mut self, phase: Phase, team: Team) {
        while !(self.phase() == phase && self.cur_team() == team) {
            self.apply(Action::EndPhase);
        }
    }
}

impl SimState {
    fn legal_actions_command(&self, actions: &mut Vec<Action>) {
        actions.push(Action::EndPhase);
    }

    fn legal_actions_movement(&self, actions: &mut Vec<Action>) {
        use Action::*;
        let coherency = self.unit_coherency();
        if coherency
            .iter()
            .filter(|(id, _)| self.get_model(*id).team == self.cur_team())
            .filter(|x| !x.1)
            .count()
            == 0
        {
            actions.push(Action::EndPhase);
        }

        coherency
            .into_iter()
            .filter(|x| !x.1)
            .for_each(|x| actions.push(Action::RemoveModel { id: x.0 }));

        let cur_team = self.cur_team();
        for model in team_models!(self, cur_team) {
            if model.cur_stats.movement > 0 {
                let model_loc = self.get_loc(model.id).unwrap();
                for l in CoordIterator::new(model_loc, model.cur_stats.movement, 1) {
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

    fn legal_actions_shooting(&self, actions: &mut Vec<Action>) {
        let cur_team = self.cur_team();
        let enemy_team = match cur_team {
            Team::Players => Team::NPCs,
            Team::NPCs => Team::Players,
        };

        for model in team_models!(self, cur_team) {
            for weapon in &model.available_ranged_weapons {
                let range = weapon.stats().range;

                for enemy in team_models!(self, enemy_team) {
                    if self
                        .get_loc(model.id)
                        .unwrap()
                        .dist(&self.get_loc(enemy.id).unwrap())
                        <= range as usize
                    {
                        let action = Action::Shoot {
                            from: model.unit,
                            to: enemy.unit,
                            ranged_weapon: *weapon,
                        };
                        if !actions.contains(&action) {
                            actions.push(action);
                        }
                    }
                }
            }
        }

        actions.push(Action::EndPhase);
    }

    fn legal_actions_charge(&self, actions: &mut Vec<Action>) {
        actions.push(Action::EndPhase);
    }

    fn legal_actions_fight(&self, actions: &mut Vec<Action>) {
        actions.push(Action::EndPhase);
    }

    fn chance_outcomes_shoot(
        &self,
        from: UnitId,
        to: UnitId,
        ranged_weapon: RangedWeapon,
    ) -> ChanceProbabilities {
        // We only count attacks from models that have the weapon in question
        let num_attacks = unit_models!(self, from)
            .filter(|m| m.ranged_weapons.contains(&ranged_weapon))
            .count();
        let target = unit_models!(self, to).next().unwrap();

        attack_success_probs(
            num_attacks.try_into().unwrap(),
            ranged_weapon.stats().ballistic_skill,
            ranged_weapon.stats().strength,
            target.cur_stats.toughness,
            ranged_weapon.stats().armor_penetration,
            target.cur_stats.save,
        )
    }

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
                    self.models[id.0].cur_stats.movement -= amount;
                }
                ActionResult::EndPhase => {
                    if self.phase == Phase::Fight {
                        self.initiative.rotate_left(1);
                    }

                    self.phase = match self.phase {
                        Phase::Command => Phase::Movement,
                        Phase::Movement => Phase::Shooting,
                        Phase::Shooting => Phase::Charge,
                        Phase::Charge => Phase::Fight,
                        Phase::Fight => Phase::Command,
                    };
                }
                ActionResult::RestoreMovement { id, amount } => {
                    self.models[id.0].cur_stats.movement += amount;
                }

                ActionResult::NewTurn(x) => self.is_start_of_turn = x,
                ActionResult::RemoveModel { id } => self.models[id.0].is_destroyed = true,
                ActionResult::QueueChanceNode { action } => self.pending_chance_action.push(action),
                ActionResult::ApplyWound { num_wounds, id } => {
                    self.get_model_mut(id).cur_stats.wound -= num_wounds;
                }
                ActionResult::ResolveChanceNode { action: _ } => {
                    self.pending_chance_action.pop();
                }
                ActionResult::UseWeapon { id, weapon } => {
                    self.get_model_mut(id)
                        .available_ranged_weapons
                        .remove(&weapon);
                }
                ActionResult::ReloadWeapon { id, weapon } => {
                    self.get_model_mut(id)
                        .available_ranged_weapons
                        .insert(weapon);
                }
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
        unit_type: UnitType,
        model_stats: ModelStats,
        ranged_weapons: Vec<RangedWeapon>,
    ) {
        if matches!(unit_type, UnitType::NewUnit) {
            self.next_unit_id += 1;
        }

        let entity = Model {
            id: ModelId(self.next_model_id),
            cur_stats: model_stats.clone(),
            base_stats: model_stats,
            remaining_actions: 1,
            team,
            unit: UnitId(self.next_unit_id),
            is_destroyed: false,
            sprite,
            available_ranged_weapons: HashSet::from_iter(ranged_weapons.iter().cloned()),
            ranged_weapons: HashSet::from_iter(ranged_weapons.iter().cloned()),
        };

        self.models.push(entity);
        self.locations.push(Some(loc));
        self.next_model_id += 1;
    }

    pub fn cur_team(&self) -> Team {
        self.initiative[0]
    }

    fn generate_results_move_model(&mut self, id: ModelId, from: SimCoords, to: SimCoords) {
        let distance = to.dist(&from);

        self.queued_results
            .push(ActionResult::Move { from, to, id });
        self.queued_results.push(ActionResult::SpendMovement {
            id,
            amount: distance as u8,
        });
    }

    fn generate_results_remove_model(&mut self, id: ModelId) {
        self.queued_results.push(ActionResult::RemoveModel { id });
    }

    fn generate_results_end_phase(&mut self) {
        if self.phase == Phase::Movement {
            let cur_team = self.cur_team();
            for model in team_models!(self, cur_team) {
                let movement_restore = model.base_stats.movement - model.cur_stats.movement;
                if movement_restore > 0 {
                    self.queued_results.push(ActionResult::RestoreMovement {
                        id: model.id,
                        amount: movement_restore,
                    });
                }
            }
        } else if self.phase == Phase::Shooting {
            let cur_team = self.cur_team();
            for model in team_models!(self, cur_team) {
                for weapon in &model.ranged_weapons {
                    if !model.available_ranged_weapons.contains(weapon) {
                        self.queued_results.push(ActionResult::ReloadWeapon {
                            id: model.id,
                            weapon: *weapon,
                        })
                    }
                }
            }
        }

        self.queued_results.push(ActionResult::EndPhase);
    }

    fn generate_results_shoot(&mut self, action: Action) {
        self.queued_results
            .push(ActionResult::QueueChanceNode { action });

        if let Action::Shoot {
            from,
            to: _,
            ranged_weapon,
        } = action
        {
            for model in unit_models!(self, from) {
                self.queued_results.push(ActionResult::UseWeapon {
                    id: model.id,
                    weapon: ranged_weapon,
                });
            }
        }
    }

    fn generate_results_roll_result(&mut self, num_success: u8) {
        // start for just the shooting results

        match self.pending_chance_action.last() {
            Some(Action::Shoot {
                from: _,
                to,
                ranged_weapon,
            }) => self.generate_shooting_results(num_success, ranged_weapon.stats().attack, *to),
            Some(_) => todo!(),
            None => panic!("trying to apply a chance result when no pending chance action"),
        }

        self.queued_results.push(ActionResult::ResolveChanceNode {
            action: *self.pending_chance_action.last().unwrap(),
        });
    }

    fn generate_shooting_results(&mut self, num_success: u8, attack: AttackValue, target: UnitId) {
        let attack = match attack {
            AttackValue::One => 1,
            AttackValue::Two => 2,
            AttackValue::Three => 3,
            AttackValue::D6 => todo!(),
            AttackValue::D3 => todo!(),
        };
        let mut remaining_attacks = num_success;
        {
            let mut models = unit_models!(self, target);
            while let Some(model) = models.next()
                && remaining_attacks > 0
            {
                let mut accumulated_wound = 0;

                while remaining_attacks > 0 && model.cur_stats.wound > accumulated_wound {
                    accumulated_wound += attack.min(model.cur_stats.wound - accumulated_wound);
                    remaining_attacks -= 1;
                }

                self.queued_results.push(ActionResult::ApplyWound {
                    id: model.id,
                    num_wounds: accumulated_wound,
                });

                if accumulated_wound == model.cur_stats.wound {
                    self.queued_results
                        .push(ActionResult::RemoveModel { id: model.id })
                }

                assert!(accumulated_wound <= model.cur_stats.wound);
            }
        }
    }

    pub fn get_id(&self, coords: SimCoords) -> Option<ModelId> {
        self.locations
            .iter()
            .enumerate()
            .filter(|(i, &c)| c == Some(coords) && !self.get_model(ModelId(*i)).is_destroyed)
            .map(|(id, _)| ModelId(id))
            .next()
    }

    fn get_model(&self, id: ModelId) -> &Model {
        &self.models[id.0]
    }

    fn get_model_mut(&mut self, id: ModelId) -> &mut Model {
        &mut self.models[id.0]
    }

    pub fn get_model_unit(&self, id: ModelId) -> UnitId {
        self.get_model(id).unit
    }

    pub fn sprites(&self) -> Vec<(ModelId, SimCoords, ModelSprite)> {
        self.models
            .iter()
            .zip(self.locations.iter())
            .filter(|(m, l)| l.is_some() && !m.is_destroyed)
            .map(|(e, l)| (e.id, l.unwrap(), e.sprite))
            .collect_vec()
    }

    pub fn unit_sprites(&self, unit: UnitId) -> Vec<(ModelId, SimCoords, ModelSprite)> {
        self.models
            .iter()
            .zip(self.locations.iter())
            .filter(|(m, l)| l.is_some() && !m.is_destroyed && m.unit == unit)
            .map(|(e, l)| (e.id, l.unwrap(), e.sprite))
            .collect_vec()
    }

    pub fn get_loc(&self, id: ModelId) -> Option<SimCoords> {
        self.locations[id.0]
    }

    fn is_populated(&self, target: &SimCoords) -> bool {
        self.locations
            .iter()
            .enumerate()
            .any(|x| x.1 == &Some(*target) && !self.get_model(ModelId(x.0)).is_destroyed)
    }

    pub fn health(&self, id: &ModelId) -> Option<u8> {
        self.models.get(id.0).map(|x| x.cur_stats.wound)
    }

    pub fn max_health(&self, id: &ModelId) -> Option<u8> {
        self.models.get(id.0).map(|x| x.base_stats.wound)
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
        self.next_model_id.hash(state);
        self.initiative.hash(state);
        self.locations.hash(state);
        // self.models.hash(state);
        self.is_start_of_turn.hash(state);
        todo!()
    }
}

#[cfg(test)]
mod tests {

    use core::{assert, assert_eq};

    use rand::{rngs::StdRng, SeedableRng};

    use super::*;

    #[test]
    fn test_unit_coherency() {
        // Single model units are coherent
        let mut gs = SimState::new();
        insert_space_marine_unit(&mut gs, vec![sc(1, 10)], Team::Players);
        assert!(gs.unit_coherency().iter().all(|x| x.1));

        // Models in a straight line don't have coherency as swarms
        let mut gs = SimState::new();
        insert_space_marine_unit(
            &mut gs,
            (0..10).map(|x| sc(1 + x, 10)).collect_vec(),
            Team::Players,
        );
        assert!(!gs.unit_coherency().iter().all(|x| x.1));

        // But non-swarm units will
        let mut gs = SimState::new();
        insert_space_marine_unit(
            &mut gs,
            (0..5).map(|i| sc(1 + i, 5)).collect_vec(),
            Team::Players,
        );
        assert!(gs.unit_coherency().iter().all(|x| x.1));

        // Non-swarm aren't coherent with a gap
        let mut gs = SimState::new();
        insert_space_marine_unit(&mut gs, vec![sc(1, 10), sc(3, 10)], Team::Players);
        assert!(
            !gs.unit_coherency().iter().all(|x| x.1),
            "Non-swarm aren't coherent with a gap"
        );

        // Swarm are coherent in a rectangle
        let mut gs = SimState::new();
        insert_space_marine_unit(
            &mut gs,
            (0..20).map(|i| sc(1 + i % 10, 5 + i / 10)).collect_vec(),
            Team::Players,
        );
        assert!(gs.unit_coherency().iter().all(|x| x.1));

        // enemy units don't count for coherency
        let mut gs = SimState::new();
        insert_space_marine_unit(&mut gs, vec![sc(1, 10), sc(3, 10)], Team::Players);
        insert_space_marine_unit(&mut gs, vec![sc(2, 10)], Team::NPCs);
        assert_eq!(gs.unit_coherency().iter().filter(|x| !x.1).count(), 2);

        // player models but different units don't count for coherency
        let mut gs = SimState::new();
        insert_space_marine_unit(&mut gs, vec![sc(1, 10), sc(3, 10)], Team::Players);
        insert_space_marine_unit(&mut gs, vec![sc(2, 10)], Team::Players);
        assert_eq!(gs.unit_coherency().iter().filter(|x| !x.1).count(), 2);

        // All units in a unit must have a path between them, e.g. can't have two groups
        let mut gs = SimState::new();
        insert_space_marine_unit(
            &mut gs,
            vec![sc(1, 10), sc(2, 10), sc(1, 12), sc(2, 12)],
            Team::Players,
        );
        assert!(!gs.unit_coherency().iter().all(|x| x.1));
        let mut actions = Vec::new();
        gs.legal_actions(&mut actions);
        assert!(
            !actions.contains(&Action::EndPhase),
            "Can't end turn when not in unit coherency"
        );

        // Removing a unit should fix unit coherency
        let mut gs = SimState::new();
        insert_space_marine_unit(
            &mut gs,
            vec![sc(1, 10), sc(2, 10), sc(4, 10)],
            Team::Players,
        );

        assert!(!gs.unit_coherency().iter().all(|x| x.1));
        gs.apply(Action::RemoveModel { id: ModelId(2) });
        assert!(gs.unit_coherency().iter().all(|x| x.1));

        // Coherency works with multiple units and teams
        let mut gs = SimState::new();
        // insert_space_marine_unit(&mut state, sc(5, 10), Team::Players, 0, 10);
        insert_space_marine_unit(
            &mut gs,
            vec![sc(1, 10), sc(2, 10), sc(3, 10)],
            Team::Players,
        );
        insert_necron_unit(&mut gs, vec![sc(1, 15), sc(2, 15), sc(3, 15)], Team::NPCs);
        assert!(gs.unit_coherency().iter().all(|x| x.1));
    }

    #[test]
    fn test_phase_change() {
        let mut gs = SimState::new();
        assert_eq!(gs.phase(), Phase::Movement); // for now starting in movement phase
        assert_eq!(gs.cur_team(), Team::Players);
        gs.apply(Action::EndPhase);
        assert_eq!(gs.phase(), Phase::Shooting);
        assert_eq!(gs.cur_team(), Team::Players);
        gs.apply(Action::EndPhase);
        assert_eq!(gs.phase(), Phase::Charge);
        assert_eq!(gs.cur_team(), Team::Players);
        gs.apply(Action::EndPhase);
        assert_eq!(gs.phase(), Phase::Fight);
        assert_eq!(gs.cur_team(), Team::Players);
        gs.apply(Action::EndPhase);
        assert_eq!(gs.phase(), Phase::Command);
        assert_eq!(gs.cur_team(), Team::NPCs);
    }

    #[test]
    fn test_set_phase() {
        let mut gs = SimState::new();
        gs.set_phase(Phase::Fight, Team::NPCs);
        assert_eq!(gs.phase(), Phase::Fight);
        assert_eq!(gs.cur_team(), Team::NPCs);
    }

    #[test]
    fn test_shooting_legal_actions() {
        let mut gs = SimState::new();
        insert_space_marine_unit(&mut gs, vec![sc(1, 10)], Team::Players);
        gs.set_phase(Phase::Shooting, Team::Players);
        let mut actions = Vec::new();

        // no targets
        gs.legal_actions(&mut actions);
        assert_eq!(actions, vec![Action::EndPhase]);

        // single target out of range
        insert_necron_unit(&mut gs, vec![sc(50, 50)], Team::NPCs);
        gs.legal_actions(&mut actions);
        assert_eq!(actions, vec![Action::EndPhase]);

        // single target in range
        insert_necron_unit(&mut gs, vec![sc(3, 10), sc(4, 10)], Team::NPCs);
        gs.legal_actions(&mut actions);
        assert_eq!(
            actions,
            vec![
                Action::Shoot {
                    from: UnitId(1),
                    to: UnitId(3),
                    ranged_weapon: RangedWeapon::BoltPistol
                },
                Action::Shoot {
                    from: UnitId(1),
                    to: UnitId(3),
                    ranged_weapon: RangedWeapon::Boltgun
                },
                Action::EndPhase
            ]
        );

        // add in when part of the unit is in range and part is out of range, on both the attacking a fired upon units
        insert_necron_unit(
            &mut gs,
            vec![sc(
                (1 + RangedWeapon::BoltPistol.stats().range + 1).into(),
                10,
            )],
            Team::NPCs,
        );
        gs.legal_actions(&mut actions);
        assert_eq!(
            actions,
            vec![
                Action::Shoot {
                    from: UnitId(1),
                    to: UnitId(3),
                    ranged_weapon: RangedWeapon::BoltPistol
                },
                Action::Shoot {
                    from: UnitId(1),
                    to: UnitId(3),
                    ranged_weapon: RangedWeapon::Boltgun
                },
                Action::Shoot {
                    from: UnitId(1),
                    to: UnitId(4),
                    ranged_weapon: RangedWeapon::Boltgun
                },
                Action::EndPhase
            ]
        );
    }

    #[test]
    fn test_shoot_phase() {
        let mut gs = SimState::new();
        insert_space_marine_unit(&mut gs, vec![sc(1, 10)], Team::Players);
        insert_necron_unit(&mut gs, vec![sc(3, 10), sc(4, 10)], Team::NPCs);

        assert_eq!(
            unit_models!(gs, UnitId(2))
                .map(|m| m.cur_stats.wound)
                .sum::<u8>(),
            2
        );

        gs.set_phase(Phase::Shooting, Team::Players);
        gs.apply(Action::Shoot {
            from: UnitId(1),
            to: UnitId(2),
            ranged_weapon: RangedWeapon::Boltgun,
        });

        let mut actions = Vec::new();
        gs.legal_actions(&mut actions);
        assert_eq!(actions, vec![]);

        assert!(gs.is_chance_node());
        let probs = gs.chance_outcomes();
        let mut rng: StdRng = SeedableRng::seed_from_u64(43);
        let a = probs.sample(&mut rng);
        // should be one success from seeded rng
        assert!(matches!(a, Action::RollResult { num_success: 1 }));
        gs.apply(a);

        assert!(!gs.is_chance_node());

        // Should have 1 wound, the extra damage from the boltrifle doesn't spill over
        assert_eq!(
            unit_models!(gs, UnitId(2))
                .map(|m| m.cur_stats.wound)
                .sum::<u8>(),
            1
        );
    }

    #[test]
    fn test_undo() {
        let mut start_state = SimState::new();
        insert_space_marine_unit(
            &mut start_state,
            (0..10).map(|i| sc(1 + i, 10)).collect_vec(),
            Team::Players,
        );
        insert_space_marine_unit(
            &mut start_state,
            (0..10).map(|i| sc(1 + i, 15)).collect_vec(),
            Team::NPCs,
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
                let a = if state.is_chance_node() {
                    let probs = state.chance_outcomes();
                    probs.sample(&mut rng)
                } else {
                    state.legal_actions(&mut actions);
                    use rand::prelude::SliceRandom;
                    *actions.choose(&mut rng).unwrap()
                };

                state.apply(a);
                state.undo();
                assert_eq!(
                    state,
                    undo_state,
                    "failed to undo index {}: {:?}\n{:#?}",
                    index,
                    a,
                    state._diff_between(&undo_state)
                );
                state.apply(a);
                index += 1;
            }
        }
    }
}
