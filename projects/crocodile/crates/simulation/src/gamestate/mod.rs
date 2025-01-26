use core::{option::Option::None, todo, write};
use std::{
    collections::{HashMap, HashSet},
    fmt::{Debug, Display},
};

use itertools::Itertools;
use petgraph::algo::{has_path_connecting, DfsSpace};
use probability::{attack_success_probs, charge_success_probs, ChanceProbabilities};
use spatial::{sc, CoordIterator, SimCoords};
use utils::{team_models, unit_models, TeamFlags};
use weapons::Arsenal;

use crate::{
    info::{insert_necron_unit, insert_space_marine_unit, ModelStats, Weapon},
    ModelSprite,
};

pub mod ai_interface;
mod gs_debug;
mod probability;
pub mod spatial;
#[cfg(test)]
mod tests;
mod utils;
mod weapons;

const WORLD_SIZE: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Team {
    Players,
    #[default]
    NPCs,
}

impl Team {
    pub fn enemy(&self) -> Team {
        match self {
            Team::Players => Team::NPCs,
            Team::NPCs => Team::Players,
        }
    }
}

impl Display for Team {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Team::Players => "Players",
                Team::NPCs => "NPCs",
            }
        )
    }
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
    // fight phase state
    ended_fight_phase: TeamFlags,
    active_fight_team: Team,
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
    Charge {
        id: ModelId,
        from: SimCoords,
        to: SimCoords,
    },
    UseWeapon {
        from: UnitId,
        to: UnitId,
        weapon: Weapon,
    },
    /// Remove a model due to lack of unit coherency
    RemoveModel {
        id: ModelId,
    },
    RollResult {
        num_success: u8,
    },
    GainChargeDistance {
        unit: UnitId,
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
    /// Change the acting team in the fight phase
    SetActiveFightTeam(Team),
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
        weapon: Weapon,
    },
    ReloadWeapon {
        id: ModelId,
        weapon: Weapon,
    },
    RestoreCharge {
        id: ModelId,
        amount: u8,
    },
    SpendCharge {
        id: ModelId,
        amount: u8,
    },
    SetFinishedFight {
        team: Team,
        value: bool,
    },

    // UI only results
    Hit {
        id: ModelId,
    },
    Miss {
        id: ModelId,
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
            Action::UseWeapon {
                from: _from,
                to: _to,
                weapon: _ranged_weapon,
            } => todo!(),
            Action::RollResult { num_success } => {
                f.write_fmt(format_args!("Succeded {:?} times", num_success))
            }
            Action::GainChargeDistance { unit: _ } => todo!(),
            Action::Charge { id, from, to } => f.write_fmt(format_args!(
                "Charging {:?}: from {:?} to {:?}",
                id, from, to
            )),
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
    charge_movement: u8,
    team: Team,
    weapons: Arsenal,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelId(usize);

/// Denotes the unit a model belongs to
#[derive(Hash, Debug, PartialEq, Clone, Eq, Copy)]
pub struct UnitId(u8);

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
            ended_fight_phase: TeamFlags::new_false(),
            active_fight_team: Team::Players,
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
            Action::UseWeapon {
                from: _,
                to: _,
                weapon: _,
            } => self.generate_results_use_weapon(action),
            Action::RollResult { num_success } => self.generate_results_roll_result(num_success),
            Action::GainChargeDistance { unit: _ } => {
                panic!("this action should never be applied directly")
            }
            Action::Charge { id, from, to } => self.generate_results_charge(id, from, to),
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
            Phase::Fight => self.legal_actions_fight(actions, self.cur_team()),
        }
    }

    /// Return the probabilities for `ChanceOutcomes`
    pub fn chance_outcomes(&self) -> ChanceProbabilities {
        if !self.is_chance_node() {
            panic!("called chance outcomes when not a chance node")
        }

        match self.pending_chance_action.last() {
            Some(Action::UseWeapon {
                from,
                to,
                weapon: ranged_weapon,
            }) => self.chance_outcomes_shoot(*from, *to, *ranged_weapon),
            Some(Action::GainChargeDistance { unit: _ }) => charge_success_probs(),
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

    pub fn is_chance_node(&self) -> bool {
        !self.pending_chance_action.is_empty()
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
                    self.get_model_mut(id).weapons.enable(weapon);
                }
                ActionResult::ReloadWeapon { id, weapon } => {
                    self.get_model_mut(id).weapons.disable(&weapon);
                }
                ActionResult::RestoreCharge { id, amount } => {
                    self.get_model_mut(id).charge_movement -= amount
                }
                ActionResult::SpendCharge { id, amount } => {
                    self.get_model_mut(id).charge_movement += amount
                }

                ActionResult::SetFinishedFight { team, value } => {
                    self.ended_fight_phase.set(team, !value)
                }
                ActionResult::SetActiveFightTeam(team) => self.active_fight_team = team.enemy(),

                // UI only
                ActionResult::Hit { id: _ } => {}
                ActionResult::Miss { id: _ } => {}
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
        let enemy_team = cur_team.enemy();

        for model in team_models!(self, cur_team) {
            for weapon in model.weapons.available_ranged() {
                let range = weapon.stats().range;

                for enemy in team_models!(self, enemy_team) {
                    if self
                        .get_loc(model.id)
                        .unwrap()
                        .dist(&self.get_loc(enemy.id).unwrap())
                        <= range as usize
                    {
                        let action = Action::UseWeapon {
                            from: model.unit,
                            to: enemy.unit,
                            weapon: *weapon,
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
        use Action::*;
        let coherency = self.unit_coherency();
        if coherency
            .iter()
            .filter(|(id, _)| self.get_model(*id).team == self.cur_team())
            .filter(|x| !x.1)
            .count()
            == 0
        {
            actions.push(EndPhase);
        }

        coherency
            .into_iter()
            .filter(|x| !x.1)
            .for_each(|x| actions.push(RemoveModel { id: x.0 }));

        let cur_team = self.cur_team();
        for model in team_models!(self, cur_team) {
            if model.charge_movement > 0 {
                let model_loc = self.get_loc(model.id).unwrap();
                for l in CoordIterator::new(model_loc, model.charge_movement, 1) {
                    // need to check if in engagement range of enemy square
                    if !self.is_populated(&l)
                        && self.is_legal_charge_space(&l, cur_team, model.unit)
                    {
                        // todo: should only be legal if adjacent unit model is adjacent to an enemy
                        actions.push(Charge {
                            id: model.id,
                            from: model_loc,
                            to: l,
                        });
                    }
                }
            }
        }
    }

    fn legal_actions_fight(&self, actions: &mut Vec<Action>, team: Team) {
        let enemy_team = team.enemy();

        for model in team_models!(self, team) {
            for weapon in model.weapons.available_melee() {
                let range = 1;

                for enemy in team_models!(self, enemy_team) {
                    if self
                        .get_loc(model.id)
                        .unwrap()
                        .dist(&self.get_loc(enemy.id).unwrap())
                        <= range as usize
                    {
                        let action = Action::UseWeapon {
                            from: model.unit,
                            to: enemy.unit,
                            weapon: *weapon,
                        };
                        if !actions.contains(&action) {
                            actions.push(action);
                        }
                    }
                }
            }
        }

        if actions.is_empty() {
            actions.push(Action::EndPhase);
        }
    }

    fn chance_outcomes_shoot(
        &self,
        from: UnitId,
        to: UnitId,
        ranged_weapon: Weapon,
    ) -> ChanceProbabilities {
        // We only count attacks from models that have the weapon in question
        let num_modesl = unit_models!(self, from)
            .filter(|m| m.weapons.is_available(&ranged_weapon))
            .count();
        let target = unit_models!(self, to).next().unwrap();
        let num_attacks = ranged_weapon.stats().num_attacks.value();

        attack_success_probs(
            num_modesl as u8 * num_attacks,
            ranged_weapon.stats().skill,
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
                    self.get_model_mut(id).weapons.disable(&weapon);
                }
                ActionResult::ReloadWeapon { id, weapon } => {
                    self.get_model_mut(id).weapons.enable(weapon);
                }
                ActionResult::RestoreCharge { id, amount } => {
                    self.get_model_mut(id).charge_movement += amount
                }
                ActionResult::SpendCharge { id, amount } => {
                    self.get_model_mut(id).charge_movement -= amount
                }

                ActionResult::SetFinishedFight { team, value } => {
                    self.ended_fight_phase.set(team, value)
                }
                ActionResult::SetActiveFightTeam(team) => self.active_fight_team = team,

                // UI only results
                ActionResult::Hit { id: _ } => {}
                ActionResult::Miss { id: _ } => {}
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
        ranged_weapons: Vec<Weapon>,
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
            weapons: Arsenal::from_vec(ranged_weapons),
            charge_movement: 0,
        };

        self.models.push(entity);
        self.locations.push(Some(loc));
        self.next_model_id += 1;
    }

    pub fn cur_team(&self) -> Team {
        if self.phase() != Phase::Fight {
            self.initiative[0]
        } else {
            self.active_fight_team
        }
    }

    fn generate_results_charge(&mut self, id: ModelId, from: SimCoords, to: SimCoords) {
        let distance = to.dist(&from);

        self.queued_results
            .push(ActionResult::Move { from, to, id });
        self.queued_results.push(ActionResult::SpendCharge {
            id,
            amount: distance as u8,
        });
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
        let ending_fight_phase =
            self.phase() == Phase::Fight && self.ended_fight_phase.get(self.cur_team().enemy());

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
            // reload weapons
            let cur_team = self.cur_team();
            for model in team_models!(self, cur_team) {
                for weapon in model.weapons.all_ranged() {
                    if !model.weapons.is_available(weapon) {
                        self.queued_results.push(ActionResult::ReloadWeapon {
                            id: model.id,
                            weapon: *weapon,
                        })
                    }
                }
            }

            // queue up chance nodes
            let mut units = HashSet::new();
            for m in team_models!(self, cur_team) {
                units.insert(m.unit);
            }

            for u in units {
                self.queued_results.push(ActionResult::QueueChanceNode {
                    action: Action::GainChargeDistance { unit: u },
                });
            }
        } else if self.phase == Phase::Charge {
            // zero out all charge
            let cur_team = self.cur_team();
            for model in team_models!(self, cur_team) {
                if model.charge_movement > 0 {
                    self.queued_results.push(ActionResult::SpendCharge {
                        id: model.id,
                        amount: model.charge_movement,
                    })
                }
            }
        } else if ending_fight_phase {
            // reload weapons for both teams
            for model in &self.models {
                for weapon in model.weapons.all_melee() {
                    if !model.weapons.is_available(weapon) {
                        self.queued_results.push(ActionResult::ReloadWeapon {
                            id: model.id,
                            weapon: *weapon,
                        })
                    }
                }
            }
        }

        // if there are any pending chance nodes, skip them. This shouldn't come up
        // in normal play, but is a convience when using the `set_phase` function.
        for pedning_chance in &self.pending_chance_action {
            self.queued_results.push(ActionResult::ResolveChanceNode {
                action: *pedning_chance,
            });
        }

        // Only end the actual phase if it's not the fight phase, or
        // the other player has already ended their phase, in this situation
        // it means both players are ending their phase
        if self.phase() != Phase::Fight || ending_fight_phase {
            self.queued_results.push(ActionResult::EndPhase);
        } else {
            self.queued_results.push(ActionResult::SetFinishedFight {
                team: self.cur_team(),
                value: true,
            });
            self.queued_results
                .push(ActionResult::SetActiveFightTeam(self.cur_team().enemy()));
        }
    }

    fn generate_results_use_weapon(&mut self, action: Action) {
        self.queued_results
            .push(ActionResult::QueueChanceNode { action });
    }

    fn generate_results_roll_result(&mut self, num_success: u8) {
        // start for just the shooting results

        match self.pending_chance_action.last() {
            Some(Action::UseWeapon {
                from,
                to,
                weapon: ranged_weapon,
            }) => self.generate_weapon_resolution_results(*from, *to, *ranged_weapon, num_success),
            Some(Action::GainChargeDistance { unit }) => {
                self.generate_gain_charge_resolution_results(num_success, *unit)
            }
            Some(_) => todo!(),
            None => panic!("trying to apply a chance result when no pending chance action"),
        }

        self.queued_results.push(ActionResult::ResolveChanceNode {
            action: *self.pending_chance_action.last().unwrap(),
        });
    }

    fn generate_gain_charge_resolution_results(&mut self, num_success: u8, unit: UnitId) {
        for model in unit_models!(self, unit) {
            self.queued_results.push(ActionResult::RestoreCharge {
                id: model.id,
                amount: num_success,
            })
        }
    }

    /// Calculate damage and use up weapons
    fn generate_weapon_resolution_results(
        &mut self,
        from: UnitId,
        to: UnitId,
        weapon: Weapon,
        num_success: u8,
    ) {
        let mut remaining_attacks = num_success;
        let damage = weapon.stats().damage;

        let mut models = unit_models!(self, to);
        while let Some(model) = models.next()
            && remaining_attacks > 0
        {
            let mut accumulated_wound = 0;

            while remaining_attacks > 0 && model.cur_stats.wound > accumulated_wound {
                accumulated_wound += damage.min(model.cur_stats.wound - accumulated_wound);
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

        for (i, model) in unit_models!(self, from).enumerate() {
            self.queued_results.push(ActionResult::UseWeapon {
                id: model.id,
                weapon,
            });

            // Spawn the ui events for hits and misses
            if i < num_success as usize {
                self.queued_results.push(ActionResult::Hit { id: model.id });
            } else {
                self.queued_results
                    .push(ActionResult::Miss { id: model.id });
            }
        }

        // Special case fo fight phase, where we alternate who is going
        if self.phase() == Phase::Fight {
            self.queued_results
                .push(ActionResult::SetActiveFightTeam(self.cur_team().enemy()));
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

    fn is_adjacent_enemy(&self, target: &SimCoords, team: Team) -> bool {
        CoordIterator::new(*target, 1, 1).any(|adjacent| {
            self.locations.iter().enumerate().any(|x| {
                x.1 == &Some(adjacent)
                    && self.get_model(ModelId(x.0)).team != team
                    && !self.get_model(ModelId(x.0)).is_destroyed
            })
        })
    }

    fn is_legal_charge_space(&self, target: &SimCoords, team: Team, unit: UnitId) -> bool {
        if self.is_adjacent_enemy(target, team) {
            return true;
        }

        CoordIterator::new(*target, 1, 1)
            .filter(|x| self.is_populated(x))
            .filter(|x| self.get_model(self.get_id(*x).unwrap()).unit == unit)
            .any(|x| self.is_adjacent_enemy(&x, team))
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

    pub fn stats(&self, id: &ModelId) -> ModelStats {
        self.get_model(*id).cur_stats.clone()
    }
}
