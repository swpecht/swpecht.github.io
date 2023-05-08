use std::collections::HashMap;

use log::trace;

use crate::{
    actions,
    cfragent::cfrnode::ActionVec,
    game::{Action, GameState, Player},
    istate::IStateKey,
    policy::Policy,
};

/// A best response algorithm that can handle hidden actions (like those needed for euchre)
///
/// Adaption from openspeil's best response algorithm:
///     https://github.com/deepmind/open_spiel/blob/master/open_spiel/python/algorithms/best_response.py
pub struct TabularBestResponse<'a, G: GameState, P: Policy<G>> {
    _root_state: G,
    _num_players: usize,
    player: Player,
    info_sets: HashMap<IStateKey, Vec<(G, f64)>>,
    cut_threshold: f64,
    policy: &'a mut P,
}

impl<'a, G: GameState, P: Policy<G>> TabularBestResponse<'a, G, P> {
    pub fn new(policy: &'a mut P, root_state: &G, player: Player, cut_threshold: f64) -> Self {
        let mut br = Self {
            _root_state: root_state.clone(),
            _num_players: root_state.num_players(),
            player,
            info_sets: HashMap::new(),
            cut_threshold,
            policy,
        };

        br.info_sets = br.info_sets(root_state);

        return br;
    }

    /// Returns a dict of infostatekey to list of (state, cf_probability).
    fn info_sets(&mut self, state: &G) -> HashMap<IStateKey, Vec<(G, f64)>> {
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
    fn decision_nodes(&mut self, parent_state: &G) -> Vec<(G, f64)> {
        let mut descendants = Vec::new();

        if parent_state.is_terminal() {
            return descendants;
        }

        if parent_state.cur_player() == self.player && !parent_state.is_chance_node() {
            descendants.push((parent_state.clone(), 1.0));
        }

        for (action, p_action) in self.transitions(&parent_state) {
            let mut child_state = parent_state.clone();
            child_state.apply_action(action);
            for (state, p_state) in self.decision_nodes(&child_state) {
                descendants.push((state, p_state * p_action));
            }
        }

        return descendants;
    }

    /// Returns a list of (action, cf_prob) pairs from the specified state.
    fn transitions(&mut self, gs: &G) -> Vec<(Action, f64)> {
        let mut list = Vec::new();

        if gs.is_chance_node() {
            // only support uniform probability chance outcomes
            let actions = actions!(gs);
            let prob = 1.0 / actions.len() as f64;

            for a in actions {
                list.push((a, prob));
            }
        } else if gs.cur_player() == self.player {
            // Counterfactual reach probabilities exclude the best-responder's actions,
            // hence return probability 1.0 for every action.
            for a in actions!(gs) {
                list.push((a, 1.0));
            }
        } else {
            let probs = self.policy.action_probabilities(&gs);
            for a in actions!(gs) {
                list.push((a, probs[a]));
            }
        }

        return list;
    }

    /// Returns the value of the specified state to the best-responder.
    pub fn value(&mut self, gs: &G) -> f64 {
        if gs.is_terminal() {
            let v = gs.evaluate(self.player);
            trace!("found terminal node: {:?} with value: {}", gs, v);
            return v;
        } else if gs.cur_player() == self.player && !gs.is_chance_node() {
            let action = self.best_response_action(&gs.istate_key(self.player));
            trace!("found best response action of {:?} for {:?}", action, gs);
            let mut ngs = gs.clone();
            ngs.apply_action(action);
            return self.value(&ngs);
        } else {
            let mut v = 0.0;
            trace!("evaluating childre for {:?}", gs);
            for (a, p) in self.transitions(gs) {
                if p > self.cut_threshold {
                    let mut ngs = gs.clone();
                    ngs.apply_action(a);
                    v += p * self.value(&ngs);
                }
            }

            return v;
        }
    }

    /// Returns the best response for this information state.
    pub fn best_response_action(&mut self, infostate: &IStateKey) -> Action {
        let infoset = self.info_sets.get(&infostate);
        if infoset.is_none() {
            panic!("couldn't find key");
        }
        let infoset = infoset.unwrap().clone();

        // Get actions from the first (state, cf_prob) pair in the infoset list.
        // Return the best action by counterfactual-reach-weighted state-value.
        let gs = &infoset[0].0;

        let actions = actions!(gs);
        let mut max_action = actions[0];
        let mut max_v = f64::NEG_INFINITY;
        for a in actions {
            let mut v = 0.0;
            for (gs, cf_p) in &infoset {
                let mut ngs = gs.clone();
                ngs.apply_action(a);
                v += cf_p * self.value(&ngs);
            }

            if v > max_v {
                max_v = v;
                max_action = a;
            }
        }

        return max_action;
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

    use crate::{
        algorithms::tabular_best_response::TabularBestResponse,
        game::kuhn_poker::{KPAction as A, KuhnPoker as KP},
        policy::UniformRandomPolicy,
    };

    #[test]
    fn test_tabular_best_response() {
        let mut policy = UniformRandomPolicy::new();
        let mut br = TabularBestResponse::new(&mut policy, &KP::new_state(), 0, 0.0);

        let expected_policy = HashMap::from([
            (KP::istate_key(&[A::Jack], 0), A::Bet.into()), // Bet in case opponent folds when winning
            (KP::istate_key(&[A::Queen], 0), A::Bet.into()), // Bet in case opponent folds when winning
            (KP::istate_key(&[A::King], 0), A::Bet.into()), // Both equally good (we return the lowest action)
            // Some of these will never happen under the best-response policy,
            // but we have computed best-response actions anyway.
            (
                KP::istate_key(&[A::Jack, A::Queen, A::Pass, A::Bet], 0),
                A::Pass.into(),
            ), // Fold - we're losing
            (
                KP::istate_key(&[A::Queen, A::Jack, A::Pass, A::Bet], 0),
                A::Bet.into(),
            ), // Call - we're 50-50
            (
                KP::istate_key(&[A::King, A::Jack, A::Pass, A::Bet], 0),
                A::Bet.into(),
            ), // Call - we've won
        ]);

        let mut calculated_policy = HashMap::new();

        for &k in expected_policy.keys() {
            if k.len() == 2 {
                panic!()
            }
            calculated_policy.insert(k, br.best_response_action(&k));
        }

        assert_eq!(calculated_policy, expected_policy);
    }
}
