use std::{
    collections::HashMap,
    fs::{self, File},
    io::BufWriter,
    iter,
    path::Path,
};

use itertools::Itertools;
use log::{debug, warn};
use rand::{rngs::StdRng, seq::SliceRandom, Rng, SeedableRng};
use rayon::prelude::*;
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
    istate::{IStateKey, NormalizedAction, NormalizedIstate},
    metrics::increment_counter,
    policy::Policy,
};

use super::cfrnode::ActionVec;
use features::features;

/// Number of iterations to stop doing the linear CFR normalization
///
/// https://www.science.org/doi/10.1126/science.aay2400
///
/// Stop doing the normalizations after a certain number of steps since no longer worth the effort
const LINEAR_CFR_CUTOFF: usize = 10_000_000;

features! {
    pub mod feature {
        const NormalizeSuit = 0b10000000,
        const LinearCFR = 0b01000000
    }
}

#[derive(Default)]
enum AverageType {
    _Full,
    #[default]
    Simple,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct InfoState {
    actions: Vec<NormalizedAction>,
    regrets: Vec<f64>,
    avg_strategy: Vec<f64>,
    last_iteration: usize,
}

impl InfoState {
    pub fn new(normalized_actions: Vec<NormalizedAction>) -> Self {
        let n = normalized_actions.len();
        Self {
            actions: normalized_actions,
            regrets: vec![1.0 / 1e6; n],
            avg_strategy: vec![1.0 / 1e6; n],
            last_iteration: 0,
        }
    }

    pub fn avg_strategy(&self) -> Vec<(NormalizedAction, f64)> {
        self.actions
            .clone()
            .into_iter()
            .zip(self.avg_strategy.clone())
            .collect_vec()
    }

    pub fn regrets(&self) -> Vec<(NormalizedAction, f64)> {
        self.actions
            .clone()
            .into_iter()
            .zip(self.regrets.clone())
            .collect_vec()
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
    iteration: usize,
    infostates: HashMap<IStateKey, InfoState>,
    /// determine if we are at the max depth and should use the rollout
    is_max_depth: fn(&G) -> bool,
    normalize_action: fn(Action, &G) -> NormalizedAction,
    denormalize_action: fn(NormalizedAction, &G) -> Action,
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
        let normalize_action: fn(Action, &EuchreGameState) -> NormalizedAction;
        let denormalize_action: fn(NormalizedAction, &EuchreGameState) -> Action;

        if feature::is_enabled(feature::NormalizeSuit) {
            normalize_action = crate::game::euchre::ismorphic::normalize_action;
            denormalize_action = crate::game::euchre::ismorphic::denormalize_action;
        } else {
            normalize_action = |action, _gs: &EuchreGameState| NormalizedAction::new(action);
            denormalize_action = |action, _| action.get();
        }

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
            iteration: 0,
            evaluator: OpenHandSolver::new_euchre(),
            normalize_action,
            denormalize_action,
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
            normalize_action: |action, _| NormalizedAction::new(action),
            denormalize_action: |action, _| action.get(),
            iteration: 0,
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
        self.iteration += 1;

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
        let info_state_key = normalize_istate(gs.istate_key(cur_player), self.normalize_action, gs);
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
        let normalized_actions = actions
            .iter()
            .map(|&a| (self.normalize_action)(a, gs))
            .collect_vec();

        let policy;
        {
            let infostate_info = self.lookup_infostate_info(&info_state_key, &normalized_actions);
            let regrets = infostate_info
                .regrets()
                .into_iter()
                .map(|(a, v)| ((self.denormalize_action)(a, gs), v))
                .collect_vec();

            policy = regret_matching(&regrets);

            let mut sorted_actions = policy.actions().clone();
            sorted_actions.sort();
            assert_eq!(sorted_actions, actions, "{}", gs);
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
            let normalize = self.normalize_action;
            let iteration = self.iteration;
            let infostate_info = self.lookup_infostate_info(&info_state_key, &normalized_actions);
            for &a in actions.iter() {
                let norm_a = (normalize)(a, gs);
                add_regret(infostate_info, norm_a, child_values[a] - value, iteration);
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
            let normalize = self.normalize_action;
            let infostate_info = self.lookup_infostate_info(&info_state_key, &normalized_actions);
            for &action in actions.iter() {
                let norm_a = (normalize)(action, gs);
                add_avstrat(infostate_info, norm_a, policy[action]);
            }
        }

        actions.clear();
        self.vector_pool.attach(actions);

        value
    }

    /// Looks up an information set table for the given key.
    fn lookup_infostate_info(
        &mut self,
        key: &NormalizedIstate,
        actions: &[NormalizedAction],
    ) -> &mut InfoState {
        self.infostates
            .entry(key.get())
            .or_insert(InfoState::new(actions.to_vec()))
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
        let info_state_key = normalize_istate(gs.istate_key(cur_player), self.normalize_action, gs);

        let mut actions = self.vector_pool.detach();
        gs.legal_actions(&mut actions);

        let normalized_actions = actions
            .iter()
            .map(|&a| (self.normalize_action)(a, gs))
            .collect_vec();

        let infostate_info = self.lookup_infostate_info(&info_state_key, &normalized_actions);
        let regrets = infostate_info
            .regrets()
            .into_iter()
            .map(|(a, v)| ((self.denormalize_action)(a, gs), v))
            .collect_vec();
        let policy = regret_matching(&regrets);

        for a in actions.iter() {
            let mut new_reach_probs = reach_probs.clone();
            new_reach_probs[cur_player] *= policy[*a];
            gs.apply_action(*a);
            self.full_update_average(gs, &new_reach_probs);
            gs.undo();
        }

        // Now update the cumulative policy
        let normalize = self.normalize_action;
        let infostate_info = self.lookup_infostate_info(&info_state_key, &normalized_actions);
        for a in actions.iter() {
            let norm_a = (normalize)(*a, gs);
            add_avstrat(infostate_info, norm_a, reach_probs[cur_player] * policy[*a])
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
/// Returns:
///   probability of taking each action
fn regret_matching(regrets: &Vec<(Action, f64)>) -> ActionVec<f64> {
    let sum_pos_regrets: f64 = regrets.iter().map(|(_, b)| b.max(0.0)).sum();

    let actions = regrets.iter().map(|(a, _)| *a).collect_vec();
    let mut policy = ActionVec::new(&actions);

    if sum_pos_regrets <= 0.0 {
        for a in &actions {
            policy[*a] = 1.0 / actions.len() as f64;
        }
    } else {
        for (a, r) in regrets {
            policy[*a] = r.max(0.0) / sum_pos_regrets;
        }
    }

    policy
}

fn add_regret(infostate: &mut InfoState, action: NormalizedAction, amount: f64, iteration: usize) {
    // Implement linear CFR for the early iterations.
    //
    // We do the update on write of regrets to avoid needing to touch nodes that haven't been updated
    // in a given iteration
    //
    //https://www.science.org/doi/10.1126/science.aay2400
    //
    // Equivalently, one could multiply the accumulated regret by
    // t / t+1 on each iteration. We do this in
    //  our experiments to reduce the risk of numerical instability.
    if feature::is_enabled(feature::LinearCFR)
        && iteration <= LINEAR_CFR_CUTOFF
        // We don't need to do this if the node has never been touched before. This is not only
        // an optimization, but also ensures that we don't set the weights to 0 by accident
        && infostate.last_iteration > 0
    {
        let mut factor = 1.0;
        for i in (infostate.last_iteration)..iteration {
            let f = i as f64 / (i as f64 + 1.0);
            factor *= f;
        }
        infostate.regrets.iter_mut().for_each(|r| *r *= factor);
        infostate.last_iteration = iteration;
    }

    let idx = infostate
        .actions
        .iter()
        .position(|&x| x == action)
        .expect("couldn't find action");
    infostate.regrets[idx] += amount;
}

fn add_avstrat(infostate: &mut InfoState, action: NormalizedAction, amount: f64) {
    let idx = infostate
        .actions
        .iter()
        .position(|&x| x == action)
        .expect("couldn't find action");
    infostate.avg_strategy[idx] += amount;
}

fn normalize_istate<G>(
    key: IStateKey,
    normalizer: fn(Action, &G) -> NormalizedAction,
    gs: &G,
) -> NormalizedIstate {
    let mut new_istate = IStateKey::default();

    for a in key {
        let norm_a = (normalizer)(a, gs);
        new_istate.push(norm_a.get());
    }

    NormalizedIstate::new(new_istate)
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
                .avg_strategy()
                .iter()
                .map(|(_, v)| *v)
                .sum();
            for (norm_a, s) in retrieved_infostate.avg_strategy() {
                let a = (self.denormalize_action)(norm_a, gs);
                policy[a] = s / policy_sum;
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

    use super::feature;

    #[test]
    fn cfres_train_test() {
        feature::enable(feature::NormalizeSuit);
        feature::enable(feature::LinearCFR);

        let mut alg = CFRES::new(|| (KuhnPoker::game().new)(), SeedableRng::seed_from_u64(43));
        alg.train(10);
    }
}
