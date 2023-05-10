use std::collections::HashMap;

use rand::{rngs::StdRng, Rng, SeedableRng};

use crate::{
    actions,
    agents::Agent,
    alloc::Pool,
    cfragent::cfrnode::ActionVec,
    game::{Game, GameState, Player},
    istate::IStateKey,
};

const UNLIMITED_NUM_WORLD_SAMPLES: i32 = -1;
const UNEXPANDED_VISIT_COUNT: i32 = -1;
const TIE_TOLERANCE: f64 = 1e-5;

enum ISMCTSFinalPolicyType {
    NORMALIZED_VISITED_COUNT,
    MAX_VISIT_COUNT,
    MAX_VALUE,
}

enum ChildSelectionPolicy {
    UCT,
    PUCT,
}

/// Child node information for the search tree.
struct ChildInfo {
    visits: usize,
    return_sum: f64,
    prior: usize,
}

impl ChildInfo {
    fn new(visits: usize, return_sum: f64, prior: usize) -> Self {
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
    child_info: HashMap<usize, usize>,
    total_visits: i32,
    prior_map: HashMap<usize, usize>,
}

pub trait Evaluator {}

#[derive(Default)]
struct RandomRolloutEvaluator {}
impl Evaluator for RandomRolloutEvaluator {}

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

impl<G: GameState + ResampleFromInfoState, E: Evaluator> ISMCTSBot<G, E> {
    pub fn new(game: Game<G>, uct_c: f64, max_simulations: i32, evaluator: E) -> Self {
        Self {
            game,
            uct_c,
            max_simulations,
            child_selection_policy: ChildSelectionPolicy::PUCT, // open speil default
            final_policy_type: ISMCTSFinalPolicyType::MAX_VISIT_COUNT, // open speil default
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
            panic!("not yet implemented")
        }
        //     if self._max_world_samples == UNLIMITED_NUM_WORLD_SAMPLES:
        //       return self.resample_from_infostate(state)
        //     elif len(self._root_samples) < self._max_world_samples:
        //       self._root_samples.append(self.resample_from_infostate(state))
        //       return self._root_samples[-1].clone()
        //     elif len(self._root_samples) == self._max_world_samples:
        //       idx = self._random_state.randint(len(self._root_samples))
        //       return self._root_samples[idx].clone()
        //     else:
        //       raise pyspiel.SpielError(
        //           'Case not handled (badly set max_world_samples..?)')
    }

    fn resample_from_infostate(&mut self, gs: &G) -> G {
        gs.resample_from_istate(gs.cur_player(), &mut self.rng)
    }

    fn random_number(&mut self) -> f64 {
        self.rng.gen()
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

    fn run_simulation(&mut self, gs: &G) {
        //   def run_simulation(self, state):
        //     if state.is_terminal():
        //       return state.returns()
        //     elif state.is_chance_node():
        //       action_list, prob_list = zip(*state.chance_outcomes())
        //       chance_action = self._random_state.choice(action_list, p=prob_list)
        //       state.apply_action(chance_action)
        //       return self.run_simulation(state)
        //     legal_actions = state.legal_actions()
        //     cur_player = state.current_player()
        //     node = self.lookup_or_create_node(state)

        //     assert node

        //     if node.total_visits == UNEXPANDED_VISIT_COUNT:
        //       node.total_visits = 0
        //       for action, prob in self._evaluator.prior(state):
        //         node.prior_map[action] = prob
        //       return self._evaluator.evaluate(state)
        //     else:
        //       chosen_action = self.check_expand(
        //           node, legal_actions)  # add one children at a time?
        //       if chosen_action != pyspiel.INVALID_ACTION:
        //         # check if all actions have been expanded, if not, select one?
        //         # if yes, ucb?
        //         self.expand_if_necessary(node, chosen_action)
        //       else:
        //         chosen_action = self.select_action_tree_policy(node, legal_actions)

        //       assert chosen_action != pyspiel.INVALID_ACTION

        //       node.total_visits += 1
        //       node.child_info[chosen_action].visits += 1
        //       state.apply_action(chosen_action)
        //       returns = self.run_simulation(state)
        //       node.child_info[chosen_action].return_sum += returns[cur_player]
        //       return returns
        todo!()
    }
}

//   def step_with_policy(self, state):
//     policy = self.get_policy(state)
//     action_list, prob_list = zip(*policy)
//     sampled_action = self._random_state.choice(action_list, p=prob_list)
//     return policy, sampled_action

//     policy_size = len(policy)
//     legal_actions = state.legal_actions()
//     if policy_size < len(legal_actions):  # do we really need this step?
//       for action in legal_actions:
//         if action not in node.child_info:
//           policy.append((action, 0.0))
//     return policy

//   def set_resampler(self, cb):
//     self._resampler_cb = cb

//   def filter_illegals(self, node, legal_actions):
//     new_node = copy.deepcopy(node)
//     for action, child in node.child_info.items():
//       if action not in legal_actions:
//         new_node.total_visits -= child.visits
//         del new_node.child_info[action]
//     return new_node

//   def expand_if_necessary(self, node, action):
//     if action not in node.child_info:
//       node.child_info[action] = ChildInfo(0.0, 0.0, node.prior_map[action])

//   def select_action_tree_policy(self, node, legal_actions):
//     if self._allow_inconsistent_action_sets:
//       temp_node = self.filter_illegals(node, legal_actions)
//       if temp_node.total_visits == 0:
//         action = legal_actions[self._random_state.randint(
//             len(legal_actions))]  # prior?
//         self.expand_if_necessary(node, action)
//         return action
//       else:
//         return self.select_action(temp_node)
//     else:
//       return self.select_action(node)

//   def select_action(self, node):
//     candidates = []
//     max_value = -float('inf')
//     for action, child in node.child_info.items():
//       assert child.visits > 0

//       action_value = child.value()
//       if self._child_selection_policy == ChildSelectionPolicy.UCT:
//         action_value += (self._uct_c *
//                          np.sqrt(np.log(node.total_visits)/child.visits))
//       elif self._child_selection_policy == ChildSelectionPolicy.PUCT:
//         action_value += (self._uct_c * child.prior *
//                          np.sqrt(node.total_visits)/(1 + child.visits))
//       else:
//         raise pyspiel.SpielError('Child selection policy unrecognized.')
//       if action_value > max_value + TIE_TOLERANCE:
//         candidates = [action]
//         max_value = action_value
//       elif (action_value > max_value - TIE_TOLERANCE and
//             action_value < max_value + TIE_TOLERANCE):
//         candidates.append(action)
//         max_value = action_value

//     assert len(candidates) >= 1
//     return candidates[self._random_state.randint(len(candidates))]

//   def check_expand(self, node, legal_actions):
//     if not self._allow_inconsistent_action_sets and len(
//         node.child_info) == len(legal_actions):
//       return pyspiel.INVALID_ACTION
//     legal_actions_copy = copy.deepcopy(legal_actions)
//     self._random_state.shuffle(legal_actions_copy)
//     for action in legal_actions_copy:
//       if action not in node.child_info:
//         return action
//     return pyspiel.INVALID_ACTION

impl<G: GameState + ResampleFromInfoState, E: Evaluator> Agent<G> for ISMCTSBot<G, E> {
    fn step(&mut self, s: &G) -> crate::game::Action {
        //   def step(self, state):
        //     action_list, prob_list = zip(*self.run_search(state))
        //     return self._random_state.choice(action_list, p=prob_list)
        self.run_search(s);
        todo!()
    }
}
