use std::collections::{hash_map::Entry, HashMap};

use rand::{rngs::StdRng, seq::SliceRandom, Rng, SeedableRng};
use rustc_hash::FxHashMap;

use crate::{
    actions,
    agents::Agent,
    alloc::Pool,
    cfragent::cfrnode::ActionVec,
    game::{Action, Game, GameState, Player},
    istate::IStateKey,
};

const UNLIMITED_NUM_WORLD_SAMPLES: i32 = -1;
const UNEXPANDED_VISIT_COUNT: i32 = -1;
const TIE_TOLERANCE: f64 = 1e-5;

enum ISMCTSFinalPolicyType {
    _NormalizedVisitedCount,
    MaxVisitCount,
    _MaxValue,
}

enum ChildSelectionPolicy {
    _Uct,
    Puct,
}

/// Child node information for the search tree.
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
#[derive(Default)]
struct ISMCTSNode {
    child_info: HashMap<Action, ChildInfo>,
    total_visits: i32,
    prior_map: FxHashMap<Action, f64>,
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
    fn evaluate(&self, gs: &G) -> Vec<f64>;
    /// Returns a probability for each legal action in the given state.
    fn prior(&self, gs: &G) -> ActionVec<f64>;
}

#[derive(Default)]
struct RandomRolloutEvaluator {}
impl<G> Evaluator<G> for RandomRolloutEvaluator {
    fn evaluate(&self, gs: &G) -> Vec<f64> {
        todo!()
    }

    fn prior(&self, gs: &G) -> ActionVec<f64> {
        todo!()
    }
}

/// Resample the chance nodes to other versions of the gamestate that result in the same istate for a given player
pub trait ResampleFromInfoState {
    fn resample_from_istate<T: Rng>(&self, player: Player, rng: &mut T) -> Self;
}

/// Implementation of Information Set Monte Carlo Tree Search (IS-MCTS).
///
/// Adapted from: https://github.com/deepmind/open_spiel/blob/master/open_spiel/python/algorithms/ismcts.py
pub struct ISMCTSBot<G: GameState, E> {
    game: Game<G>,
    uct_c: f64,
    max_simulations: i32,
    child_selection_policy: ChildSelectionPolicy,
    final_policy_type: ISMCTSFinalPolicyType,
    max_world_samples: i32,
    evaluator: E,
    node_pool: Pool<ISMCTSNode>,
    nodes: HashMap<IStateKey, ISMCTSNode>,
    rng: StdRng,
}

impl<G: GameState + ResampleFromInfoState, E: Evaluator<G>> ISMCTSBot<G, E> {
    pub fn new(game: Game<G>, uct_c: f64, max_simulations: i32, evaluator: E) -> Self {
        Self {
            game,
            uct_c,
            max_simulations,
            child_selection_policy: ChildSelectionPolicy::Puct, // open speil default
            final_policy_type: ISMCTSFinalPolicyType::MaxVisitCount, // open speil default
            max_world_samples: UNLIMITED_NUM_WORLD_SAMPLES,
            evaluator,
            node_pool: Pool::new(ISMCTSNode::default), // open speil default
            nodes: HashMap::default(),
            rng: StdRng::seed_from_u64(42),
        }
    }

    fn run_search(&mut self, gs: &G) -> ActionVec<f64> {
        self.reset();
        //   def run_search(self, state):

        let actions = actions!(gs);
        if actions.len() == 1 {
            let mut policy = ActionVec::new(&actions);
            policy[actions[0]] = 1.0;
            return policy;
        }

        // root node
        self.create_new_node(gs);
        let root_infostate_key = self.get_state_key(gs);
        for _ in 0..self.max_simulations {
            let sampled_root_state = self.sample_root_state(gs);
            let p = gs.cur_player();
            assert_eq!(sampled_root_state.istate_key(p), root_infostate_key);

            self.run_simulation(&sampled_root_state);
        }

        // Don't need to handle inconsistent action sets since have perfect recall
        self.get_final_policy(gs)
    }

    fn get_final_policy(&mut self, gs: &G) -> ActionVec<f64> {
        //   def get_final_policy(self, state, node):
        //     assert node
        //     if self._final_policy_type == ISMCTSFinalPolicyType.NORMALIZED_VISITED_COUNT:
        //       assert node.total_visits > 0
        //       total_visits = node.total_visits
        //       policy = [(action, child.visits / total_visits)
        //                 for action, child in node.child_info.items()]
        //     elif self._final_policy_type == ISMCTSFinalPolicyType.MAX_VISIT_COUNT:
        //       assert node.total_visits > 0
        //       max_visits = -float('inf')
        //       count = 0
        //       for action, child in node.child_info.items():
        //         if child.visits == max_visits:
        //           count += 1
        //         elif child.visits > max_visits:
        //           max_visits = child.visits
        //           count = 1
        //       policy = [(action, 1. / count if child.visits == max_visits else 0.0)
        //                 for action, child in node.child_info.items()]
        //     elif self._final_policy_type == ISMCTSFinalPolicyType.MAX_VALUE:
        //       assert node.total_visits > 0
        //       max_value = -float('inf')
        //       count = 0
        //       for action, child in node.child_info.items():
        //         if child.value() == max_value:
        //           count += 1
        //         elif child.value() > max_value:
        //           max_value = child.value()
        //           count = 1
        //       policy = [(action, 1. / count if child.value() == max_value else 0.0)
        //                 for action, child in node.child_info.items()]
        todo!()
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
        //   def reset(self):
        //     self._nodes = {}
        //     self._node_pool = []
        //     self._root_samples = []
        todo!()
    }

    fn get_policy(&mut self, gs: &G) -> ActionVec<f64> {
        self.run_search(gs)
    }

    fn create_new_node(&mut self, gs: &G) -> &mut ISMCTSNode {
        let key = self.get_state_key(gs);
        let mut node = self.node_pool.detach();
        node.total_visits = UNEXPANDED_VISIT_COUNT;
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
        let node = self.nodes.entry(*node_key).or_default();
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
                ChildSelectionPolicy::_Uct => {
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
        //   def step(self, state):
        //     action_list, prob_list = zip(*self.run_search(state))
        //     return self._random_state.choice(action_list, p=prob_list)
        let action_weights = self.run_search(s).to_vec();
        action_weights
            .choose_weighted(&mut self.rng, |item| item.1)
            .unwrap()
            .0
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_ismcts() {
        todo!()
    }
}
