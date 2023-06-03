use std::{
    collections::{
        hash_map::{DefaultHasher, Entry},
        HashMap,
    },
    hash::{Hash, Hasher},
};

use log::info;
use rand::{rngs::StdRng, seq::SliceRandom, Rng, SeedableRng};
use rustc_hash::FxHashMap;

use crate::{
    actions,
    agents::Agent,
    cfragent::cfrnode::ActionVec,
    game::{Action, Game, GameState, Player},
    istate::IStateKey,
    policy::Policy,
};

const UNLIMITED_NUM_WORLD_SAMPLES: i32 = -1;
const UNEXPANDED_VISIT_COUNT: i32 = -1;
const TIE_TOLERANCE: f64 = 1e-5;

#[derive(Clone, Debug)]
pub enum ISMCTSFinalPolicyType {
    NormalizedVisitedCount,
    MaxVisitCount,
    MaxValue,
}

#[derive(Clone, Debug)]
pub enum ChildSelectionPolicy {
    Uct,
    Puct,
}

/// Child node information for the search tree.
#[derive(Clone)]
struct ChildInfo {
    visits: usize,
    return_sum: f64,
    prior: f64,
}

impl ChildInfo {
    fn new(visits: usize, return_sum: f64, prior: f64) -> Self {
        Self {
            visits,
            return_sum,
            prior,
        }
    }

    fn value(&self) -> f64 {
        self.return_sum / self.visits as f64
    }
}

/// Node data structure for the search tree.
#[derive(Clone)]
struct ISMCTSNode {
    child_info: HashMap<Action, ChildInfo>,
    total_visits: i32,
    prior_map: FxHashMap<Action, f64>,
}

impl Default for ISMCTSNode {
    fn default() -> Self {
        Self {
            child_info: Default::default(),
            total_visits: UNEXPANDED_VISIT_COUNT,
            prior_map: Default::default(),
        }
    }
}

/// Abstract class representing an evaluation function for a game.
///
/// The evaluation function takes in an intermediate state in the game and returns
/// an evaluation of that state, which should correlate with chances of winning
/// the game. It returns the evaluation from all player's perspectives.
///
/// https://github.com/deepmind/open_spiel/blob/master/open_spiel/python/algorithms/mcts.py
pub trait Evaluator<G> {
    /// Returns evaluation on given state.
    fn evaluate(&mut self, gs: &G) -> Vec<f64>;
    /// Returns a probability for each legal action in the given state.
    fn prior(&mut self, gs: &G) -> ActionVec<f64>;
}

/// A simple evaluator doing random rollouts. It will always return the same result
/// if called on the same gamestate
///
/// This evaluator returns the average outcome of playing random actions from the
/// given state until the end of the game.  n_rollouts is the number of random
/// outcomes to be considered.
#[derive(Clone)]
pub struct RandomRolloutEvaluator {
    n_rollouts: usize,
}

impl RandomRolloutEvaluator {
    pub fn new(n_rollouts: usize) -> Self {
        Self { n_rollouts }
    }
}

impl<G: GameState> Evaluator<G> for RandomRolloutEvaluator {
    fn evaluate(&mut self, gs: &G) -> Vec<f64> {
        let key = gs.key();
        let mut hasher = DefaultHasher::default();
        key.hash(&mut hasher);
        let seed = hasher.finish();
        let mut rng: StdRng = SeedableRng::seed_from_u64(seed);

        let mut result = vec![0.0; gs.num_players()];
        let mut working_state = gs.clone();
        let mut actions = Vec::new();

        for _ in 0..self.n_rollouts {
            working_state.clone_from(gs);
            actions.clear();

            while !working_state.is_terminal() {
                working_state.legal_actions(&mut actions);
                if actions.is_empty() {
                    info!("{}", working_state);
                }
                let a = *actions.choose(&mut rng).unwrap();
                working_state.apply_action(a);
            }

            for (i, r) in result.iter_mut().enumerate() {
                *r += working_state.evaluate(i);
            }
        }

        for r in result.iter_mut() {
            *r /= self.n_rollouts as f64;
        }
        result
    }

    /// Returns equal probability for all actions
    fn prior(&mut self, gs: &G) -> ActionVec<f64> {
        let actions = actions!(gs);
        let prob = 1.0 / actions.len() as f64;

        let mut r = ActionVec::new(&actions);
        for a in actions {
            r[a] = prob;
        }
        r
    }
}

/// Resample the chance nodes to other versions of the gamestate that result in the same istate for a given player
pub trait ResampleFromInfoState {
    fn resample_from_istate<T: Rng>(&self, player: Player, rng: &mut T) -> Self;
}

pub struct ISMCTBotConfig {
    pub child_selection_policy: ChildSelectionPolicy,
    pub final_policy_type: ISMCTSFinalPolicyType,
    pub max_world_samples: i32,
}

impl Default for ISMCTBotConfig {
    fn default() -> Self {
        Self {
            child_selection_policy: ChildSelectionPolicy::Puct,
            final_policy_type: ISMCTSFinalPolicyType::MaxVisitCount,
            max_world_samples: UNLIMITED_NUM_WORLD_SAMPLES,
        }
    }
}

/// Implementation of Information Set Monte Carlo Tree Search (IS-MCTS).
///
/// Adapted from: https://github.com/deepmind/open_spiel/blob/master/open_spiel/python/algorithms/ismcts.py
#[derive(Clone)]
pub struct ISMCTSBot<G: GameState, E> {
    uct_c: f64,
    max_simulations: i32,
    child_selection_policy: ChildSelectionPolicy,
    final_policy_type: ISMCTSFinalPolicyType,
    max_world_samples: i32,
    evaluator: E,
    nodes: HashMap<IStateKey, ISMCTSNode>,
    rng: StdRng,
    root_samples: Vec<G>,
}

impl<G: GameState + ResampleFromInfoState, E: Evaluator<G>> ISMCTSBot<G, E> {
    pub fn new(
        _game: Game<G>,
        uct_c: f64,
        max_simulations: i32,
        evaluator: E,
        config: ISMCTBotConfig,
    ) -> Self {
        Self {
            uct_c,
            max_simulations,
            child_selection_policy: config.child_selection_policy,
            final_policy_type: config.final_policy_type,
            max_world_samples: config.max_world_samples,
            evaluator,
            nodes: HashMap::default(),
            rng: StdRng::seed_from_u64(42),
            root_samples: Vec::new(),
        }
    }

    fn run_search(&mut self, root_node: &G) -> ActionVec<f64> {
        assert!(!root_node.is_chance_node());
        self.reset();

        let actions = actions!(root_node);
        if actions.len() == 1 {
            let mut policy = ActionVec::new(&actions);
            policy[actions[0]] = 1.0;
            return policy;
        }

        // root node
        self.create_new_node(root_node);
        let root_infostate_key = self.get_state_key(root_node);
        for _ in 0..self.max_simulations {
            let sampled_root_state = self.sample_root_state(root_node);
            let p = root_node.cur_player();
            assert_eq!(sampled_root_state.istate_key(p), root_infostate_key);

            self.run_simulation(&sampled_root_state);
        }

        // Don't need to handle inconsistent action sets since have perfect recall
        self.get_final_policy(root_node)
    }

    fn get_final_policy(&mut self, root_node: &G) -> ActionVec<f64> {
        let node = self
            .nodes
            .get(&root_node.istate_key(root_node.cur_player()))
            .unwrap();

        // see open speil for other policy type implementations
        match self.final_policy_type {
            ISMCTSFinalPolicyType::NormalizedVisitedCount => {
                assert!(node.total_visits > 0);
                let total_visits = node.total_visits;
                let actions = actions!(root_node);
                let mut policy = ActionVec::new(&actions);
                for (action, child) in &node.child_info {
                    policy[*action] = child.visits as f64 / total_visits as f64;
                }
                policy
            }
            ISMCTSFinalPolicyType::MaxVisitCount => {
                assert!(node.total_visits > 0);
                let mut max_visits = i32::MIN;
                let mut count = 0;
                for child in node.child_info.values() {
                    match (child.visits as i32).cmp(&max_visits) {
                        std::cmp::Ordering::Less => {}
                        std::cmp::Ordering::Equal => count += 1,
                        std::cmp::Ordering::Greater => {
                            max_visits = child.visits as i32;
                            count = 1;
                        }
                    }
                }

                let actions = actions!(root_node);
                let mut policy = ActionVec::new(&actions);
                for (action, child) in &node.child_info {
                    policy[*action] = if child.visits as i32 == max_visits {
                        1.0 / count as f64
                    } else {
                        0.0
                    }
                }
                policy
            }
            ISMCTSFinalPolicyType::MaxValue => todo!(),
        }
    }

    fn sample_root_state(&mut self, gs: &G) -> G {
        if self.max_world_samples == UNLIMITED_NUM_WORLD_SAMPLES {
            self.resample_from_infostate(gs)
        } else {
            // see open speil if want to implement
            panic!("not yet implemented")
        }
    }

    fn resample_from_infostate(&mut self, gs: &G) -> G {
        gs.resample_from_istate(gs.cur_player(), &mut self.rng)
    }

    fn get_state_key(&self, gs: &G) -> IStateKey {
        gs.istate_key(gs.cur_player())
    }

    fn reset(&mut self) {
        self.nodes.clear();
        self.root_samples.clear();
    }

    pub fn get_policy(&mut self, gs: &G) -> ActionVec<f64> {
        self.run_search(gs)
    }

    fn create_new_node(&mut self, gs: &G) -> &mut ISMCTSNode {
        let key = self.get_state_key(gs);
        let node = ISMCTSNode {
            total_visits: UNEXPANDED_VISIT_COUNT,
            ..Default::default()
        };
        self.nodes.insert(key, node);
        self.nodes.get_mut(&key).unwrap()
    }

    fn run_simulation(&mut self, gs: &G) -> Vec<f64> {
        if gs.is_terminal() {
            let mut returns = Vec::new();
            for p in 0..gs.num_players() {
                returns.push(gs.evaluate(p));
            }
            return returns;
        } else if gs.is_chance_node() {
            let actions = actions!(gs);
            let chance_action = *actions.choose(&mut self.rng).unwrap();
            let mut ngs = gs.clone();
            ngs.apply_action(chance_action);
            return self.run_simulation(&ngs);
        }

        let actions = actions!(gs);
        let cur_player = gs.cur_player();
        let key = self.get_state_key(gs);

        let node = self.nodes.entry(key).or_default();

        if node.total_visits == UNEXPANDED_VISIT_COUNT {
            node.total_visits = 0;
            let priors = self.evaluator.prior(gs);
            for (action, prob) in priors.to_vec() {
                node.prior_map.insert(action, prob);
            }
            return self.evaluator.evaluate(gs);
        }

        assert_eq!(node.prior_map.len(), actions!(gs).len());

        let mut chosen_action = self.check_expand(&key, &actions);
        if let Some(inner_action) = chosen_action {
            self.expand_if_necessary(&key, inner_action)
        } else {
            chosen_action = Some(self.select_action_tree_policy(&key, &actions));
        }

        let chosen_action = chosen_action.unwrap();

        let node = self.nodes.entry(key).or_default();
        node.total_visits += 1;
        node.child_info
            .entry(chosen_action)
            .and_modify(|x| x.visits += 1);

        let mut ngs = gs.clone();
        ngs.apply_action(chosen_action);
        let returns = self.run_simulation(&ngs);

        let node = self.nodes.entry(key).or_default();
        node.child_info
            .entry(chosen_action)
            .and_modify(|x| x.return_sum += returns[cur_player]);

        returns
    }

    fn check_expand(&mut self, node_key: &IStateKey, actions: &Vec<Action>) -> Option<Action> {
        let node = self.nodes.entry(*node_key).or_default();
        if actions.len() == node.child_info.len() {
            return None;
        }

        let mut shuffled_actions = actions.clone();
        shuffled_actions.shuffle(&mut self.rng);

        shuffled_actions
            .into_iter()
            .find(|&a| !node.child_info.contains_key(&a))
    }

    fn expand_if_necessary(&mut self, node_key: &IStateKey, action: Action) {
        let node = self.nodes.get_mut(node_key).unwrap();
        if let Entry::Vacant(e) = node.child_info.entry(action) {
            e.insert(ChildInfo::new(
                0,
                0.0,
                *node.prior_map.get(&action).unwrap(),
            ));
        }
    }

    fn select_action_tree_policy(&mut self, node_key: &IStateKey, _actions: &[Action]) -> Action {
        self.select_action(node_key)
    }

    fn select_action(&mut self, node_key: &IStateKey) -> Action {
        let node = self.nodes.entry(*node_key).or_default();
        let mut candidates = Vec::new();
        let mut max_value = f64::NEG_INFINITY;

        for (action, child) in &node.child_info {
            assert!(child.visits > 0);
            let mut action_value = child.value();
            action_value += match self.child_selection_policy {
                ChildSelectionPolicy::Uct => {
                    self.uct_c
                        * f64::sqrt(f64::log10(node.total_visits as f64) / child.visits as f64)
                }
                ChildSelectionPolicy::Puct => {
                    self.uct_c * child.prior * f64::sqrt(node.total_visits as f64)
                        / (1.0 + child.visits as f64)
                }
            };
            if action_value > max_value + TIE_TOLERANCE {
                candidates.clear();
                candidates.push(action);
                max_value = action_value;
            } else if (action_value > max_value - TIE_TOLERANCE)
                && (action_value < max_value + TIE_TOLERANCE)
            {
                candidates.push(action);
                max_value = action_value;
            }
        }

        **candidates.choose(&mut self.rng).unwrap()
    }
}

impl<G: GameState + ResampleFromInfoState, E: Evaluator<G>> Agent<G> for ISMCTSBot<G, E> {
    fn step(&mut self, s: &G) -> crate::game::Action {
        let action_weights = self.run_search(s).to_vec();
        action_weights
            .choose_weighted(&mut self.rng, |item| item.1)
            .unwrap()
            .0
    }

    fn get_name(&self) -> String {
        format!("ISMCTS, sims: {}, child policy: {:?}, final policy: {:?}, worlds: {}, evalutator: todo", self.max_simulations, self.child_selection_policy, self.final_policy_type, self.max_world_samples)
    }
}

impl<G: GameState + ResampleFromInfoState, E: Evaluator<G>> Policy<G> for ISMCTSBot<G, E> {
    fn action_probabilities(&mut self, gs: &G) -> ActionVec<f64> {
        self.run_search(gs)
    }
}

#[cfg(test)]
mod tests {
    use approx::assert_ulps_eq;
    use rand::{rngs::StdRng, seq::SliceRandom, thread_rng, SeedableRng};
    use stderrlog::new;

    use crate::{
        actions,
        agents::{Agent, RandomAgent},
        algorithms::ismcts::{Evaluator, ISMCTSBot, RandomRolloutEvaluator},
        game::{
            bluff::Bluff,
            kuhn_poker::{KPAction, KuhnPoker},
            GameState,
        },
        policy::Policy,
    };

    use super::{ISMCTBotConfig, ResampleFromInfoState};

    #[test]
    fn test_ismcts_is_agent() {
        let mut ismcts = ISMCTSBot::new(
            KuhnPoker::game(),
            1.5,
            100,
            RandomRolloutEvaluator::new(100),
            ISMCTBotConfig::default(),
        );
        let mut opponent = RandomAgent::default();

        for _ in 0..10 {
            let mut gs = KuhnPoker::new_state();
            while gs.is_chance_node() {
                let a = *actions!(gs).choose(&mut thread_rng()).unwrap();
                gs.apply_action(a)
            }

            while !gs.is_terminal() {
                let a = match gs.cur_player() {
                    0 => ismcts.step(&gs),
                    1 => opponent.step(&gs),
                    _ => panic!("invalid player"),
                };
                gs.apply_action(a);
            }

            gs.evaluate(0);
        }
    }

    #[test]
    fn test_ismcts_optimal_player() {
        let mut ismcts = ISMCTSBot::new(
            KuhnPoker::game(),
            1.5,
            100,
            RandomRolloutEvaluator::new(100),
            ISMCTBotConfig::default(),
        );

        let gs = KuhnPoker::from_actions(&[KPAction::Queen, KPAction::King, KPAction::Pass]);
        let policy = ismcts.run_search(&gs);
        assert_eq!(policy[KPAction::Bet.into()], 1.0);
        assert_eq!(policy[KPAction::Pass.into()], 0.0);

        let gs = KuhnPoker::from_actions(&[KPAction::Queen, KPAction::Jack, KPAction::Bet]);
        let policy = ismcts.run_search(&gs);
        assert_eq!(policy[KPAction::Bet.into()], 0.0);
        assert_eq!(policy[KPAction::Pass.into()], 1.0);

        let gs = KuhnPoker::from_actions(&[KPAction::Queen, KPAction::Jack]);
        let policy = ismcts.run_search(&gs);
        assert_eq!(policy[KPAction::Bet.into()], 0.0);
        assert_eq!(policy[KPAction::Pass.into()], 1.0);

        let gs = KuhnPoker::from_actions(&[KPAction::Jack, KPAction::Queen, KPAction::Pass]);
        let policy = ismcts.run_search(&gs);
        assert_eq!(policy[KPAction::Bet.into()], 0.0);
        assert_eq!(policy[KPAction::Pass.into()], 1.0);
    }

    /// Starting with a queen should have near-0 expected value.
    /// Root:
    /// |-P0: Pass (50%)
    ///     |-P1: Pass (25%) -- 0.0 (even odds for win or lose)
    ///     |-P1: Bet (25%)
    ///         |-P0: Pass (12.5%) -- -.125
    ///         |-P0: Bet (12.5%) -- 0.0 (even odds for win or lose)
    /// |-P0: Bet (50%)
    ///     |-P1: Pass (25%) -- +0.25
    ///     |-P1: Bet (25%) -- even odes
    ///
    /// Should be +0.125 expected value for player 0
    #[test]
    fn test_random_rollout() {
        let mut e = RandomRolloutEvaluator::new(10000);

        let gs = KuhnPoker::from_actions(&[KPAction::Queen]);
        assert_ulps_eq!(e.evaluate(&gs)[0], 0.125, epsilon = 0.01);
    }

    #[test]
    fn test_random_rollout_repeatable() {
        let mut p = RandomRolloutEvaluator::new(100);
        let mut rng: StdRng = SeedableRng::seed_from_u64(100);
        for _ in 0..100 {
            let mut gs = Bluff::new_state(1, 1);
            while gs.is_chance_node() {
                let actions = actions!(gs);
                let a = actions.choose(&mut rng).unwrap();
                gs.apply_action(*a);
            }
            let e = p.evaluate(&gs);
            let new_e = p.evaluate(&gs);
            assert_eq!(e, new_e);
        }
    }
}
