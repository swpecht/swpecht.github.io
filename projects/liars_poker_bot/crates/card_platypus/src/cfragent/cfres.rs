use std::{
    collections::HashMap,
    fs::{self, File},
    io::BufWriter,
    path::Path,
};

use log::{debug, warn};
use rand::{rngs::StdRng, seq::SliceRandom, Rng, SeedableRng};
use rmp_serde::Serializer;
use serde::{Deserialize, Serialize};

use crate::{
    agents::{Agent, Seedable},
    algorithms::{
        ismcts::{Evaluator, ResampleFromInfoState},
        open_hand_solver::OpenHandSolver,
        pimcts::PIMCTSBot,
    },
    alloc::Pool,
    game::{
        euchre::{processors::post_discard_phase, EuchreGameState},
        Action, GameState, Player,
    },
    istate::IStateKey,
    metrics::increment_counter,
    policy::Policy,
};

use super::cfrnode::ActionVec;

#[derive(Default)]
enum AverageType {
    _Full,
    #[default]
    Simple,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct InfoState {
    regrets: ActionVec<f64>,
    avg_strategy: ActionVec<f64>,
    /// Number of times this infostate was updated during training
    update_count: usize,
}

impl InfoState {
    pub(super) fn new(actions: &Vec<Action>) -> Self {
        let mut regrets = ActionVec::new(actions);
        let mut avg_strategy = ActionVec::new(actions);

        // Start with a small amount of regret and total accumulation, to give a
        // uniform policy: this will get erased fast.
        for a in actions {
            regrets[*a] = 1.0 / 1e6;
            avg_strategy[*a] = 1.0 / 1e6;
        }

        Self {
            regrets,
            avg_strategy,
            update_count: 0,
        }
    }

    pub fn regrets(&self) -> &ActionVec<f64> {
        &self.regrets
    }

    pub fn avg_strategy(&self) -> &ActionVec<f64> {
        &self.avg_strategy
    }

    pub fn update_count(&self) -> usize {
        self.update_count
    }
}

/// Implementation of external sampled CFR
///
/// Based on implementation from: OpenSpiel:
///     https://github.com/deepmind/open_spiel/blob/master/open_spiel/python/algorithms/mccfr.py
pub struct CFRES<G> {
    rng: StdRng,
    vector_pool: Pool<Vec<Action>>,
    game_generator: fn() -> G,
    average_type: AverageType,
    infostates: HashMap<IStateKey, InfoState>,
    /// determine if we are at the max depth and should use the rollout
    is_max_depth: fn(&G) -> bool,
    play_bot: PIMCTSBot<G, OpenHandSolver<G>>,
    evaluator: OpenHandSolver<G>,
}

impl<G> CFRES<G> {
    /// Gets the infostates of the agent for external analysis
    pub fn get_infostates(&self) -> HashMap<IStateKey, InfoState> {
        self.infostates.clone()
    }
}

impl<G> Seedable for CFRES<G> {
    /// Sets the seed for the evaluator, it doesn't change the seed used for training
    fn set_seed(&mut self, seed: u64) {
        self.play_bot.set_seed(seed);
    }
}

impl CFRES<EuchreGameState> {
    pub fn new_euchre_bidding(game_generator: fn() -> EuchreGameState, mut rng: StdRng) -> Self {
        let pimcts_seed = rng.gen();
        Self {
            rng,
            vector_pool: Pool::new(Vec::new),
            game_generator,
            average_type: AverageType::default(),
            infostates: HashMap::new(),
            is_max_depth: post_discard_phase,
            play_bot: PIMCTSBot::new(
                50,
                OpenHandSolver::new_euchre(),
                SeedableRng::seed_from_u64(pimcts_seed),
            ),
            evaluator: OpenHandSolver::new_euchre(),
        }
    }
}

impl<G: GameState + ResampleFromInfoState> CFRES<G> {
    pub fn new(game_generator: fn() -> G, mut rng: StdRng) -> Self {
        let pimcts_seed = rng.gen();
        Self {
            rng,
            vector_pool: Pool::new(Vec::new),
            game_generator,
            average_type: AverageType::default(),
            infostates: HashMap::new(),
            is_max_depth: |_: &G| false,
            play_bot: PIMCTSBot::new(
                50,
                OpenHandSolver::default(),
                SeedableRng::seed_from_u64(pimcts_seed),
            ),
            evaluator: OpenHandSolver::default(),
        }
    }

    pub fn train(&mut self, n: usize) {
        for _ in 0..n {
            self.iteration();
        }
        self.play_bot.reset();
        self.evaluator.reset();
    }

    pub fn save(&self, path: &str) {
        if Path::new(path).exists() {
            let mut target = path.to_string();
            target.push_str(".old");
            fs::rename(path, target.as_str()).expect("error backing up previous file");
        }

        let f = File::create(path).unwrap();
        let f = BufWriter::new(f);

        debug!("saving weights for {} infostates...", self.infostates.len());
        self.infostates.serialize(&mut Serializer::new(f)).unwrap();
    }

    pub fn load(&mut self, path: &str) -> usize {
        if Path::new(path).exists() {
            let f = &mut File::open(path);
            let f = f.as_mut().unwrap();
            self.infostates = rmp_serde::from_read(f).unwrap();
            debug!("loaded weights for {} infostates", self.infostates.len());
            self.infostates.len()
        } else {
            warn!("file not found, no infostates loaded");
            0
        }
    }

    /// Performs one iteration of external sampling.
    ///
    /// An iteration consists of one episode for each player as the update
    /// player.
    fn iteration(&mut self) {
        let num_players = (self.game_generator)().num_players();
        for player in 0..num_players {
            self.update_regrets(&mut (self.game_generator)(), player, 0);
        }

        if matches!(self.average_type, AverageType::_Full) {
            let reach_probs = vec![1.0; num_players];
            self.full_update_average(&mut (self.game_generator)(), &reach_probs);
        }
    }

    /// Runs an episode of external sampling.
    ///
    /// Args:
    ///     state: the game state to run from
    ///     player: the player to update regrets for
    ///
    /// Returns:
    ///     value: is the value of the state in the game
    ///     obtained as the weighted average of the values
    ///     of the children
    fn update_regrets(&mut self, gs: &mut G, player: Player, _depth: usize) -> f64 {
        if gs.is_terminal() {
            return gs.evaluate(player);
        }

        if gs.is_chance_node() {
            let mut actions = self.vector_pool.detach();
            gs.legal_actions(&mut actions);
            let outcome = *actions
                .choose(&mut self.rng)
                .expect("error choosing a random action for chance node");
            actions.clear();
            self.vector_pool.attach(actions);

            gs.apply_action(outcome);
            let value = self.update_regrets(gs, player, _depth + 1);
            gs.undo();
            return value;
        }

        // If we're at max depth, do the rollout
        if (self.is_max_depth)(gs) {
            return self.evaluator.evaluate_player(gs, player);
        }

        let cur_player = gs.cur_player();
        let info_state_key = gs.istate_key(cur_player);
        let mut actions = self.vector_pool.detach();
        gs.legal_actions(&mut actions);

        // don't store anything if only 1 valid action
        if actions.len() == 1 {
            gs.apply_action(actions[0]);
            let v = self.update_regrets(gs, player, _depth + 1);
            gs.undo();
            actions.clear();
            self.vector_pool.attach(actions);
            return v;
        }

        increment_counter("cfr.cfres.nodes_touched");
        let policy;
        {
            let infostate_info = self.lookup_infostate_info(&info_state_key, &actions);
            policy = regret_matching(&infostate_info.regrets);
        }

        let mut value = 0.0;
        let mut child_values = ActionVec::new(&actions);

        if cur_player != player {
            // sample at opponent node
            let a = policy
                .to_vec()
                .choose_weighted(&mut self.rng, |a| a.1)
                .expect("error choosing weighted action")
                .0;
            gs.apply_action(a);
            value = self.update_regrets(gs, player, _depth + 1);
            gs.undo();
        } else {
            // walk over all actions at my node
            for &a in actions.iter() {
                gs.apply_action(a);
                child_values[a] = self.update_regrets(gs, player, _depth + 1);
                gs.undo();
                value += policy[a] * child_values[a];
            }
        }

        if cur_player == player {
            // update regrets
            let infostate_info = self.lookup_infostate_info(&info_state_key, &actions);
            for &a in actions.iter() {
                add_regret(infostate_info, a, child_values[a] - value);
                infostate_info.update_count += 1;
            }
        }

        // Simple average does averaging on the opponent node. To do this in a game
        // with more than two players, we only update the player + 1 mod num_players,
        // which reduces to the standard rule in 2 players.
        //
        // We adapt this slightly for euchre where it alternates what team the players are on
        let cur_team = cur_player % 2;
        let player_team = player % 2;
        if matches!(self.average_type, AverageType::Simple) && cur_team != player_team {
            let infostate_info = self.lookup_infostate_info(&info_state_key, &actions);
            for &action in actions.iter() {
                add_avstrat(infostate_info, action, policy[action]);
            }
        }

        actions.clear();
        self.vector_pool.attach(actions);

        value
    }

    /// Looks up an information set table for the given key.
    fn lookup_infostate_info(&mut self, key: &IStateKey, actions: &Vec<Action>) -> &mut InfoState {
        self.infostates
            .entry(*key)
            .or_insert(InfoState::new(actions))
    }

    fn full_update_average(&mut self, gs: &mut G, reach_probs: &Vec<f64>) {
        if gs.is_terminal() {
            return;
        }

        if gs.is_chance_node() {
            let mut actions = self.vector_pool.detach();
            gs.legal_actions(&mut actions);
            for a in &actions {
                gs.apply_action(*a);
                self.full_update_average(gs, reach_probs);
                gs.undo();
            }
            actions.clear();
            self.vector_pool.attach(actions);
            return;
        }

        // If all the probs are zero, no need to keep going.
        let sum_reach_probs: f64 = reach_probs.iter().sum();
        if sum_reach_probs == 0.0 {
            return;
        }

        let cur_player = gs.cur_player();
        let info_state_key = gs.istate_key(cur_player);

        let mut actions = self.vector_pool.detach();
        gs.legal_actions(&mut actions);

        let infostate_info = self.lookup_infostate_info(&info_state_key, &actions);
        let policy = regret_matching(&infostate_info.regrets);

        for a in actions.iter() {
            let mut new_reach_probs = reach_probs.clone();
            new_reach_probs[cur_player] *= policy[*a];
            gs.apply_action(*a);
            self.full_update_average(gs, &new_reach_probs);
            gs.undo();
        }

        // Now update the cumulative policy
        let infostate_info = self.lookup_infostate_info(&info_state_key, &actions);
        for a in actions.iter() {
            add_avstrat(infostate_info, *a, reach_probs[cur_player] * policy[*a])
        }

        actions.clear();
        self.vector_pool.attach(actions);
    }

    pub fn num_info_states(&self) -> usize {
        self.infostates.len()
    }
}

/// Applies regret matching to get a policy.
///
/// Args:
///   regrets: vector regrets for each action.
///
/// Returns:
///   probability of taking each action
fn regret_matching(regrets: &ActionVec<f64>) -> ActionVec<f64> {
    let sum_pos_regrets: f64 = regrets.to_vec().iter().map(|(_, b)| b.max(0.0)).sum();
    let mut policy = ActionVec::new(regrets.actions());

    if sum_pos_regrets <= 0.0 {
        for a in regrets.actions() {
            policy[*a] = 1.0 / regrets.actions().len() as f64;
        }
    } else {
        for a in regrets.actions() {
            policy[*a] = regrets[*a].max(0.0) / sum_pos_regrets;
        }
    }

    policy
}

fn add_regret(infostate: &mut InfoState, action: Action, amount: f64) {
    infostate.regrets[action] += amount;
}

fn add_avstrat(infostate: &mut InfoState, action: Action, amount: f64) {
    infostate.avg_strategy[action] += amount;
}

impl<G: GameState + ResampleFromInfoState + Send> Policy<G> for CFRES<G> {
    /// Returns the MCCFR average policy for a player in a state.
    ///
    /// If the policy is not defined for the provided state, a uniform
    /// random policy is returned.
    fn action_probabilities(&mut self, gs: &G) -> ActionVec<f64> {
        let player = gs.cur_player();

        if (self.is_max_depth)(gs) {
            return self.play_bot.action_probabilities(gs);
        }

        let mut actions = self.vector_pool.detach();
        gs.legal_actions(&mut actions);
        let info_state_key = gs.istate_key(player);
        let retrieved_infostate = self.infostates.get(&info_state_key);

        let mut policy = ActionVec::new(&actions);
        if let Some(retrieved_infostate) = retrieved_infostate {
            let policy_sum: f64 = retrieved_infostate
                .avg_strategy
                .to_vec()
                .iter()
                .map(|(_, v)| *v)
                .sum();
            for a in actions.iter() {
                policy[*a] = retrieved_infostate.avg_strategy[*a] / policy_sum;
            }
        } else {
            for a in actions.iter() {
                policy[*a] = 1.0 / actions.len() as f64;
            }
        }

        self.vector_pool.attach(actions);

        policy
    }
}

impl<G: GameState + ResampleFromInfoState + Send> Agent<G> for CFRES<G> {
    fn step(&mut self, s: &G) -> Action {
        let action_weights = self.action_probabilities(s).to_vec();
        action_weights
            .choose_weighted(&mut self.rng, |item| item.1)
            .unwrap()
            .0
    }
}

#[cfg(test)]
mod tests {

    use rand::SeedableRng;

    use crate::{cfragent::cfres::CFRES, game::kuhn_poker::KuhnPoker};

    #[test]
    fn cfres_train_test() {
        let mut alg = CFRES::new(|| (KuhnPoker::game().new)(), SeedableRng::seed_from_u64(43));
        alg.train(10);
    }
}