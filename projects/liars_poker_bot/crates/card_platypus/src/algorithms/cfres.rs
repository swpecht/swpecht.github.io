use std::{
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use dashmap::{
    mapref::one::{Ref, RefMut},
    DashMap,
};
use dyn_clone::DynClone;
use itertools::Itertools;
use log::{debug, warn};
use rand::{rngs::StdRng, seq::SliceRandom, thread_rng, Rng, SeedableRng};
use rayon::prelude::*;

use serde::{Deserialize, Serialize};

use crate::{
    agents::{Agent, Seedable},
    algorithms::{
        ismcts::{Evaluator, ResampleFromInfoState},
        open_hand_solver::OpenHandSolver,
        pimcts::PIMCTSBot,
    },
    alloc::Pool,
    collections::{actionlist::ActionList, actionvec::ActionVec, diskstore::DiskStore},
    counter,
    game::{
        euchre::{ismorphic::EuchreNormalizer, processors::post_cards_played, EuchreGameState},
        Action, GameState, Player,
    },
    istate::{IStateKey, IStateNormalizer, NoOpNormalizer, NormalizedAction, NormalizedIstate},
    policy::Policy,
};

use features::features;

/// Number of iterations to stop doing the linear CFR normalization
///
/// https://www.science.org/doi/10.1126/science.aay2400
///
/// Stop doing the normalizations after a certain number of steps since no longer worth the effort
const LINEAR_CFR_CUTOFF: usize = 1_000_000;

counter!(nodes_touched);

features! {
    pub mod feature {
        const LinearCFR = 0b01000000,
        const SingleThread = 0b00100000
    }
}

#[derive(Default, Clone)]
enum AverageType {
    _Full,
    #[default]
    Simple,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct InfoState {
    pub actions: ActionList,
    pub regrets: Vec<f64>,
    pub avg_strategy: Vec<f64>,
    pub last_iteration: usize,
}

impl InfoState {
    pub fn new(normalized_actions: Vec<NormalizedAction>) -> Self {
        let n = normalized_actions.len();
        Self {
            actions: ActionList::new(&normalized_actions),
            regrets: vec![1.0 / 1e6; n],
            avg_strategy: vec![1.0 / 1e6; n],
            last_iteration: 0,
        }
    }

    pub fn avg_strategy(&self) -> Vec<(NormalizedAction, f64)> {
        self.actions
            .to_vec()
            .into_iter()
            .zip(self.avg_strategy.clone())
            .collect_vec()
    }

    pub fn regrets(&self) -> Vec<(NormalizedAction, f64)> {
        self.actions
            .to_vec()
            .into_iter()
            .zip(self.regrets.clone())
            .collect_vec()
    }
}

/// Implementation of external sampled CFR
///
/// Based on implementation from: OpenSpiel:
///     https://github.com/deepmind/open_spiel/blob/master/open_spiel/python/algorithms/mccfr.py
#[derive(Clone)]
pub struct CFRES<G> {
    vector_pool: Pool<Vec<Action>>,
    game_generator: fn() -> G,
    average_type: AverageType,
    iteration: Arc<AtomicUsize>,
    infostates: Arc<DiskStore>,
    /// determine if we are at the max depth and should use the rollout
    depth_checker: Box<dyn DepthChecker<G>>,
    normalizer: Box<dyn IStateNormalizer<G>>,
    play_bot: PIMCTSBot<G, OpenHandSolver<G>>,
    evaluator: OpenHandSolver<G>,
}

impl<G> CFRES<G> {
    /// Gets the infostates of the agent for external analysis
    pub fn get_infostates(&self) -> Arc<DashMap<IStateKey, InfoState>> {
        todo!("update to array tree");
    }

    pub fn iterations(&self) -> usize {
        self.iteration.load(Ordering::Relaxed)
    }
}

impl<G> Seedable for CFRES<G> {
    /// Sets the seed for the evaluator, it doesn't change the seed used for training
    fn set_seed(&mut self, seed: u64) {
        self.play_bot.set_seed(seed);
    }
}

impl CFRES<EuchreGameState> {
    pub fn new_euchre(
        game_generator: fn() -> EuchreGameState,
        mut rng: StdRng,
        max_cards_played: usize,
    ) -> Self {
        assert_eq!(max_cards_played, 0, "only implemented for 0 right now");

        let normalizer: Box<dyn IStateNormalizer<EuchreGameState>> =
            Box::<EuchreNormalizer>::default();

        let pimcts_seed = rng.gen();

        Self {
            vector_pool: Pool::new(Vec::new),
            game_generator,
            average_type: AverageType::default(),
            infostates: Arc::new(DiskStore::new(None).unwrap()),
            // is_max_depth: post_discard_phase,
            depth_checker: Box::new(EuchreDepthChecker { max_cards_played }),
            play_bot: PIMCTSBot::new(
                50,
                OpenHandSolver::new_euchre(),
                SeedableRng::seed_from_u64(pimcts_seed),
            ),
            iteration: Arc::new(AtomicUsize::new(0)),
            evaluator: OpenHandSolver::new_euchre(),
            normalizer,
        }
    }
}

impl<G: GameState + ResampleFromInfoState + Sync> CFRES<G> {
    pub fn new(game_generator: fn() -> G, mut rng: StdRng) -> Self {
        let pimcts_seed = rng.gen();
        Self {
            vector_pool: Pool::new(Vec::new),
            game_generator,
            average_type: AverageType::default(),
            infostates: Arc::new(DiskStore::new(None).unwrap()),
            depth_checker: Box::new(NoOpDepthChecker {}),
            play_bot: PIMCTSBot::new(
                50,
                OpenHandSolver::default(),
                SeedableRng::seed_from_u64(pimcts_seed),
            ),
            evaluator: OpenHandSolver::default(),
            normalizer: Box::<NoOpNormalizer>::default(),
            iteration: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn train(&mut self, n: usize) {
        if feature::is_enabled(feature::SingleThread) {
            for _ in 0..n {
                self.iteration();
            }
        } else {
            (0..n)
                .into_par_iter()
                .for_each(|_| self.clone().iteration())
        }

        self.play_bot.reset();
        self.evaluator.reset();
    }

    pub fn save(&self) {
        self.infostates.commit();
    }

    pub fn load(&mut self, path: &Path) -> usize {
        self.infostates = Arc::new(DiskStore::new(Some(path)).unwrap());
        let len = self.infostates.len();
        debug!(
            "loaded weights for {} infostates with {} iterations",
            self.infostates.len(),
            0
        );

        if len == 0 {
            warn!("no infostates loaded");
        }

        len
    }

    /// Performs one iteration of external sampling.
    ///
    /// An iteration consists of one episode for each player as the update
    /// player.
    fn iteration(&mut self) {
        // We probably don't need this strict of ordering, but will start with this and relax if becomes performance
        // issue.
        self.iteration
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

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
                .choose(&mut thread_rng())
                .expect("error choosing a random action for chance node");
            actions.clear();
            self.vector_pool.attach(actions);

            gs.apply_action(outcome);
            let value = self.update_regrets(gs, player, _depth + 1);
            gs.undo();
            return value;
        }

        // If we're at max depth, do the rollout
        if self.depth_checker.is_max_depth(gs) {
            return self.evaluator.evaluate_player(gs, player);
        }

        let cur_player = gs.cur_player();
        let info_state_key = self
            .normalizer
            .normalize_istate(&gs.istate_key(cur_player), gs);
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

        nodes_touched::increment();
        let normalized_actions = actions
            .iter()
            .map(|&a| self.normalizer.normalize_action(a, gs))
            .collect_vec();

        let policy;
        {
            let normalizer = self.normalizer.clone();
            let infostate_info = self.lookup_entry_mut(&info_state_key, &normalized_actions);
            let regrets = infostate_info
                .regrets()
                .into_iter()
                .map(|(a, v)| (normalizer.denormalize_action(a, gs), v))
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
                .choose_weighted(&mut thread_rng(), |a| a.1)
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
            let iteration = self.iteration.load(Ordering::SeqCst);
            let normalizer = self.normalizer.clone();
            let mut entry = self.lookup_entry_mut(&info_state_key, &normalized_actions);
            let infostate_info = entry.value_mut();
            for &a in actions.iter() {
                let norm_a = normalizer.normalize_action(a, gs);
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
            let normalizer = self.normalizer.clone();
            let mut entry = self.lookup_entry_mut(&info_state_key, &normalized_actions);
            let infostate_info = entry.value_mut();
            for &action in actions.iter() {
                let norm_a = normalizer.normalize_action(action, gs);
                add_avstrat(infostate_info, norm_a, policy[action]);
            }
        }

        actions.clear();
        self.vector_pool.attach(actions);

        value
    }

    fn full_update_average(&mut self, _gs: &mut G, _reach_probs: &[f64]) {
        // deleted implementation as too slow to use in practice
        todo!("not supported")
    }

    pub fn num_info_states(&self) -> usize {
        self.infostates.len()
    }
}

impl<G> CFRES<G> {
    /// Looks up an information set table for the given key.
    fn lookup_entry_mut(
        &mut self,
        key: &NormalizedIstate,
        actions: &[NormalizedAction],
    ) -> RefMut<IStateKey, InfoState> {
        self.infostates
            .get_or_create_mut(&key.get(), InfoState::new(actions.to_vec()))
    }

    /// Can deadlock if we hold onto handle
    fn lookup_entry(&self, key: &NormalizedIstate) -> Option<Ref<IStateKey, InfoState>> {
        self.infostates.get(&key.get())
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
        // We don't need to do this if the node has never been touched before. This is not only
        // an optimization, but also ensures that we don't set the weights to 0 by accident
        && infostate.last_iteration > 0
    {
        // We only apply the factor up to the cutoff amount
        let factor: f64 = (infostate.last_iteration..iteration.min(LINEAR_CFR_CUTOFF))
            .map(|i| i as f64 / (i as f64 + 1.0))
            .product();

        infostate.regrets.iter_mut().for_each(|r| *r *= factor);
    }

    infostate.last_iteration = iteration;

    let idx = infostate
        .actions
        .index(action)
        .expect("couldn't find action");
    infostate.regrets[idx] += amount;
}

fn add_avstrat(infostate: &mut InfoState, action: NormalizedAction, amount: f64) {
    let idx = infostate
        .actions
        .index(action)
        .expect("couldn't find action");
    infostate.avg_strategy[idx] += amount;
}

impl<G: GameState + ResampleFromInfoState + Send> Policy<G> for CFRES<G> {
    /// Returns the MCCFR average policy for a player in a state.
    ///
    /// If the policy is not defined for the provided state, a uniform
    /// random policy is returned.
    fn action_probabilities(&mut self, gs: &G) -> ActionVec<f64> {
        let player = gs.cur_player();

        if self.depth_checker.is_max_depth(gs) {
            return self.play_bot.action_probabilities(gs);
        }

        let mut actions = self.vector_pool.detach();
        gs.legal_actions(&mut actions);
        let info_state_key = self.normalizer.normalize_istate(&gs.istate_key(player), gs);

        let mut policy = ActionVec::new(&actions);

        {
            let retrieved_infostate = self.lookup_entry(&info_state_key);
            if let Some(retrieved_infostate) = retrieved_infostate {
                let policy_sum: f64 = retrieved_infostate
                    .avg_strategy()
                    .iter()
                    .map(|(_, v)| *v)
                    .sum();
                for (norm_a, s) in retrieved_infostate.avg_strategy() {
                    let a = self.normalizer.denormalize_action(norm_a, gs);
                    policy[a] = s / policy_sum;
                }
            } else {
                for a in actions.iter() {
                    policy[*a] = 1.0 / actions.len() as f64;
                }
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
            .choose_weighted(&mut thread_rng(), |item| item.1)
            .unwrap()
            .0
    }
}

pub trait DepthChecker<G>: Sync + Send + DynClone {
    fn is_max_depth(&self, gs: &G) -> bool;
}
dyn_clone::clone_trait_object!(<G>DepthChecker<G>);

#[derive(Clone)]
struct NoOpDepthChecker;
impl<G> DepthChecker<G> for NoOpDepthChecker {
    fn is_max_depth(&self, _: &G) -> bool {
        false
    }
}

#[derive(Clone)]
struct EuchreDepthChecker {
    max_cards_played: usize,
}

impl DepthChecker<EuchreGameState> for EuchreDepthChecker {
    fn is_max_depth(&self, gs: &EuchreGameState) -> bool {
        post_cards_played(gs, self.max_cards_played)
    }
}

#[cfg(test)]
mod tests {

    use rand::SeedableRng;

    use crate::game::kuhn_poker::KuhnPoker;

    use super::{feature, CFRES};

    #[test]
    fn cfres_train_test() {
        feature::enable(feature::LinearCFR);

        let mut alg = CFRES::new(|| (KuhnPoker::game().new)(), SeedableRng::seed_from_u64(43));
        alg.train(10);
    }
}
