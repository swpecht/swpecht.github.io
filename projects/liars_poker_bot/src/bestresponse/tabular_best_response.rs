use std::collections::HashMap;

use crate::{
    actions,
    cfragent::cfrnode::{ActionVec, CFRNode},
    database::NodeStore,
    game::{Action, Game, GameState, Player},
    istate::IStateKey,
    policy::Policy,
};

/// A best response algorithm that can handle hidden actions (like those needed for euchre)
///
/// Adaption from openspeil's best response algorithm:
///     https://github.com/deepmind/open_spiel/blob/master/open_spiel/python/algorithms/best_response.py
pub struct TabularBestResponse<'a, G: GameState, P: Policy<G>> {
    root_state: G,
    num_players: usize,
    player: Player,
    info_sets: HashMap<IStateKey, Vec<(G, f64)>>,
    cut_threshold: f64,
    policy: &'a mut P,
}

impl<'a, G: GameState, P: Policy<G>> TabularBestResponse<'a, G, P> {
    pub fn new(ns: &'a mut P, root_state: G, player: Player, cut_threshold: f64) -> Self {
        let mut br = Self {
            root_state: root_state.clone(),
            num_players: root_state.num_players(),
            player,
            info_sets: HashMap::new(),
            cut_threshold,
            policy: ns,
        };

        br.info_sets = br.info_sets(root_state);

        return br;
    }

    /// Returns a dict of infostatekey to list of (state, cf_probability).
    fn info_sets(&mut self, state: G) -> HashMap<IStateKey, Vec<(G, f64)>> {
        let mut infosets = HashMap::new();

        for (s, p) in self.decision_nodes(state) {
            let k = s.istate_key(self.player);

            if !infosets.contains_key(&k) {
                infosets.insert(k.clone(), Vec::new());
            }
            let list = infosets.get_mut(&k).unwrap();
            list.push((s, p));
        }

        return infosets;
    }

    /// Yields a (state, cf_prob) pair for each descendant decision node.
    fn decision_nodes(&mut self, parent_state: G) -> Vec<(G, f64)> {
        let mut descendants = Vec::new();

        if parent_state.is_terminal() {
            return descendants;
        }

        if parent_state.cur_player() == self.player {
            descendants.push((parent_state.clone(), 1.0));
        }

        for (action, p_action) in self.transitions(parent_state.clone()) {
            let mut child_state = parent_state.clone();
            child_state.apply_action(action);
            for (state, p_state) in self.decision_nodes(child_state) {
                descendants.push((state, p_state * p_action));
            }
        }

        return descendants;
    }

    /// Returns a list of (action, cf_prob) pairs from the specified state.
    fn transitions(&mut self, gs: G) -> Vec<(Action, f64)> {
        let mut list = Vec::new();

        if gs.cur_player() == self.player {
            // Counterfactual reach probabilities exclude the best-responder's actions,
            // hence return probability 1.0 for every action.
            for a in actions!(gs) {
                list.push((a, 1.0));
            }
        } else if gs.is_chance_node() {
            // only support uniform probability chance outcomes
            let actions = actions!(gs);
            let prob = 1.0 / actions.len() as f64;

            for a in actions {
                list.push((a, prob));
            }
        } else {
            let probs = self.policy.action_probabilities(&gs);
            for a in actions!(gs) {
                list.push((a, probs[a]));
            }
        }

        return list;
    }

    //   @_memoize_method(key_fn=lambda state: state.history_str())
    //   def value(self, state):
    //     """Returns the value of the specified state to the best-responder."""
    //     if state.is_terminal():
    //       return state.player_return(self._player_id)
    //     elif (state.current_player() == self._player_id or
    //           state.is_simultaneous_node()):
    //       action = self.best_response_action(
    //           state.information_state_string(self._player_id))
    //       return self.q_value(state, action)
    //     else:
    //       return sum(p * self.q_value(state, a)
    //                  for a, p in self.transitions(state)
    //                  if p > self._cut_threshold)

    //   def q_value(self, state, action):
    //     """Returns the value of the (state, action) to the best-responder."""
    //     if state.is_simultaneous_node():

    //       def q_value_sim(sim_state, sim_actions):
    //         child = sim_state.clone()
    //         # change action of _player_id
    //         sim_actions[self._player_id] = action
    //         child.apply_actions(sim_actions)
    //         return self.value(child)

    //       actions, probabilities = zip(*self.transitions(state))
    //       return sum(p * q_value_sim(state, a)
    //                  for a, p in zip(actions, probabilities / sum(probabilities))
    //                  if p > self._cut_threshold)
    //     else:
    //       return self.value(state.child(action))

    /// Returns the best response for this information state.
    pub fn best_response_action(&mut self, infostate: &IStateKey) -> Action {
        //     infoset = self.infosets[infostate]
        //     # Get actions from the first (state, cf_prob) pair in the infoset list.
        //     # Return the best action by counterfactual-reach-weighted state-value.
        //     return max(
        //         infoset[0][0].legal_actions(self._player_id),
        //         key=lambda a: sum(cf_p * self.q_value(s, a) for s, cf_p in infoset))

        todo!()
    }
}

impl<'a, G: GameState, P: Policy<G>> Policy<G> for TabularBestResponse<'a, G, P> {
    /// Returns the policy for a player in a state.
    fn action_probabilities(&mut self, gs: &G) -> crate::cfragent::cfrnode::ActionVec<f64> {
        let actions = actions!(gs);
        let mut probs = ActionVec::new(&actions);
        let br = self.best_response_action(&gs.istate_key(gs.cur_player()));
        probs[br] = 1.0; // always do the best actions

        return probs;
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::game::GameState;
    use crate::{
        bestresponse::tabular_best_response::TabularBestResponse,
        game::kuhn_poker::{KPAction as A, KuhnPoker},
        policy::UniformRandomPolicy,
    };

    #[test]
    fn test_tabular_best_response() {
        let mut policy = UniformRandomPolicy::new();
        let mut br = TabularBestResponse::new(&mut policy, KuhnPoker::new_state(), 0, 0.0);

        let expected_policy = HashMap::from([
            (KuhnPoker::istate_key(&[A::Jack], 0), 1), // Bet in case opponent folds when winning
            (KuhnPoker::istate_key(&[A::Queen], 0), 1), // Bet in case opponent folds when winning
            (KuhnPoker::istate_key(&[A::King], 0), 0), // Both equally good (we return the lowest action)
            // Some of these will never happen under the best-response policy,
            // but we have computed best-response actions anyway.
            (KuhnPoker::istate_key(&[A::Jack, A::Pass, A::Bet], 0), 0), // Fold - we're losing
            (KuhnPoker::istate_key(&[A::Queen, A::Pass, A::Bet], 0), 1), // Call - we're 50-50
            (KuhnPoker::istate_key(&[A::King, A::Pass, A::Bet], 0), 1), // Call - we've won
        ]);

        let mut calculated_policy = HashMap::new();

        for &k in expected_policy.keys() {
            calculated_policy.insert(k, br.best_response_action(&k).0);
        }

        assert_eq!(calculated_policy, expected_policy);
    }
}
