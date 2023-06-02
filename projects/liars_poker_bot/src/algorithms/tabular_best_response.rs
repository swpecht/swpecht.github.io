use std::collections::HashMap;

use log::{debug, trace};

use crate::{
    actions,
    cfragent::cfrnode::ActionVec,
    collections::diskbackedvec::DiskBackedVec,
    game::{Action, GameState, Player},
    istate::IStateKey,
    policy::Policy,
};

/// A best response algorithm that can handle hidden actions (like those needed for euchre)
///
/// Adaption from openspeil's best response algorithm:
///     https://github.com/deepmind/open_spiel/blob/master/open_spiel/python/algorithms/best_response.py
pub(super) struct TabularBestResponse<'a, G: GameState, P> {
    _root_state: G,
    _num_players: usize,
    player: Player,
    info_sets: HashMap<IStateKey, Vec<(G, f64)>>,
    cut_threshold: f64,
    policy: &'a mut P,
    value_cache: HashMap<IStateKey, f64>,            //Tree<f64>,
    best_response_cache: HashMap<IStateKey, Action>, // Tree<Action>,
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
            value_cache: HashMap::new(),
            best_response_cache: HashMap::new(),
        };
        br.info_sets = br.info_sets(root_state);

        br
    }

    /// Returns a dict of infostatekey to list of (state, cf_probability).
    fn info_sets(&mut self, state: &G) -> HashMap<IStateKey, Vec<(G, f64)>> {
        debug!("building info sets...");
        let mut infosets = HashMap::new();

        for (s, p) in DecisionNodeIterator::new(state.clone(), self.policy, self.player) {
            let k = s.istate_key(self.player);

            let list = infosets.entry(k).or_insert(Vec::new());
            list.push((s, p));
        }

        debug!("{} infosets found", infosets.len());
        infosets
    }

    /// Optmized version of OpenSpiel decision nodes algorithm
    fn _decision_nodes(
        &mut self,
        parent_state: &G,
        nodes: &mut DiskBackedVec<(G, f64)>,
        p_state: f64,
    ) {
        if parent_state.is_terminal() {
            return;
        }

        if parent_state.cur_player() == self.player && !parent_state.is_chance_node() {
            nodes.push((parent_state.clone(), p_state));
        }

        for (action, p_action) in self.transitions(parent_state) {
            let mut child_state = parent_state.clone();
            child_state.apply_action(action);
            self._decision_nodes(&child_state, nodes, p_state * p_action);
        }
    }

    /// Returns a list of (action, cf_prob) pairs from the specified state.
    fn transitions(&mut self, gs: &G) -> Vec<(Action, f64)> {
        transitions(gs, self.policy, self.player)
    }

    /// Returns the value of the specified state to the best-responder.
    pub fn value(&mut self, gs: &mut G) -> f64 {
        trace!("calling best response value on: {}", gs);
        let key = gs.key();
        if self.value_cache.contains_key(&key) {
            return *self.value_cache.get(&key).unwrap();
        }

        if gs.is_terminal() {
            let v = gs.evaluate(self.player);
            trace!("found terminal node: {} with value: {}", gs, v);

            self.value_cache.insert(key, v);
            v
        } else if gs.cur_player() == self.player && !gs.is_chance_node() {
            let action = self.best_response_action(&gs.istate_key(self.player));
            gs.apply_action(action);
            let v = self.value(gs);
            trace!(
                "found best response action (last action) for {} with value: {}",
                gs,
                v
            );
            self.value_cache.insert(key, v);
            gs.undo();
            return v;
        } else if gs.is_chance_node() {
            trace!("evaluating chance node: {}", gs);
            let mut v = 0.0;
            for (a, p) in self.transitions(gs) {
                if p > self.cut_threshold {
                    gs.apply_action(a);
                    v += p * self.value(gs);
                    gs.undo();
                }
            }

            self.value_cache.insert(key, v);
            trace!("found value for chance node: {}: {}", gs, v);
            return v;
        } else {
            let mut v = 0.0;
            trace!("evaluating children for {}", gs);
            for (a, p) in self.transitions(gs) {
                if p > self.cut_threshold {
                    gs.apply_action(a);
                    v += p * self.value(gs);
                    gs.undo();
                }
            }

            self.value_cache.insert(key, v);
            return v;
        }
    }

    /// Returns the best response for this information state.
    pub fn best_response_action(&mut self, infostate: &IStateKey) -> Action {
        if self.best_response_cache.contains_key(infostate) {
            return *self.best_response_cache.get(infostate).unwrap();
        }

        let infoset = self.info_sets.get(infostate);
        if infoset.is_none() {
            panic!("couldn't find key");
        }
        let mut infoset = infoset.unwrap().clone();

        // Get actions from the first (state, cf_prob) pair in the infoset list.
        // Return the best action by counterfactual-reach-weighted state-value.
        let gs = &infoset[0].0;

        let actions = actions!(gs);
        let mut max_action = actions[0];
        let mut max_v = f64::NEG_INFINITY;
        for a in actions {
            let mut v = 0.0;
            for (gs, cf_p) in infoset.iter_mut() {
                gs.apply_action(a);
                v += *cf_p * self.value(gs);
                gs.undo()
            }

            if v > max_v {
                max_v = v;
                max_action = a;
            }
        }

        self.best_response_cache.insert(*infostate, max_action);
        max_action
    }
}

impl<'a, G: GameState, P: Policy<G> + Clone> Policy<G> for TabularBestResponse<'a, G, P> {
    /// Returns the policy for a player in a state.
    fn action_probabilities(&mut self, gs: &G) -> crate::cfragent::cfrnode::ActionVec<f64> {
        let actions = actions!(gs);
        let mut probs = ActionVec::new(&actions);
        let br = self.best_response_action(&gs.istate_key(gs.cur_player()));
        probs[br] = 1.0; // always do the best actions

        probs
    }
}

fn transitions<G: GameState, P: Policy<G>>(
    gs: &G,
    policy: &mut P,
    player: Player,
) -> Vec<(Action, f64)> {
    let mut list = Vec::new();

    if gs.is_chance_node() {
        // only support uniform probability chance outcomes
        let actions = actions!(gs);
        let prob = 1.0 / actions.len() as f64;

        for a in actions {
            list.push((a, prob));
        }
    } else if gs.cur_player() == player {
        // Counterfactual reach probabilities exclude the best-responder's actions,
        // hence return probability 1.0 for every action.
        for a in actions!(gs) {
            list.push((a, 1.0));
        }
    } else {
        let probs = policy.action_probabilities(gs);
        for a in actions!(gs) {
            list.push((a, probs[a]));
        }
    }

    list
}

struct DecisionNodeIterator<'a, G, P> {
    stack: Vec<(G, f64)>,
    policy: &'a mut P,
    player: Player,
}

impl<'a, G, P> DecisionNodeIterator<'a, G, P> {
    fn new(root_node: G, policy: &'a mut P, player: Player) -> Self {
        let stack = vec![(root_node, 1.0)];
        Self {
            stack,
            policy,
            player,
        }
    }
}

impl<'a, G: GameState, P: Policy<G>> Iterator for DecisionNodeIterator<'a, G, P> {
    type Item = (G, f64);

    fn next(&mut self) -> Option<Self::Item> {
        while let Some((parent_state, p_state)) = self.stack.pop() {
            if parent_state.is_terminal() {
                continue;
            }

            // iterate backwards to maintain the same call stack as recusrive versions
            for (action, p_action) in transitions(&parent_state, self.policy, self.player)
                .iter()
                .rev()
            {
                let mut child_state = parent_state.clone();
                child_state.apply_action(*action);
                self.stack.push((child_state, p_state * p_action));
            }

            if parent_state.cur_player() == self.player && !parent_state.is_chance_node() {
                return Some((parent_state, p_state));
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use approx::assert_relative_eq;

    use crate::{
        algorithms::tabular_best_response::{DecisionNodeIterator, TabularBestResponse},
        collections::diskbackedvec::DiskBackedVec,
        game::{
            bluff::Bluff,
            kuhn_poker::{KPAction as A, KuhnPoker as KP},
        },
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

    #[test]
    fn test_decision_nodes_kuhn_poker() {
        let mut policy = UniformRandomPolicy::new();
        let root_state = KP::new_state();
        let mut br = TabularBestResponse::new(&mut policy, &root_state, 0, 0.0);

        let mut policy = UniformRandomPolicy::new();
        let first_decision_nodes = DecisionNodeIterator::new(root_state.clone(), &mut policy, 0);

        let mut unrolled_decision_nodes = DiskBackedVec::new();
        br._decision_nodes(&root_state, &mut unrolled_decision_nodes, 1.0);

        // assert_eq!(unrolled_decision_nodes.len(), first_decision_nodes.len());

        for (i, fd) in first_decision_nodes.enumerate() {
            assert_eq!(*unrolled_decision_nodes.get(i), fd);
        }
    }

    #[test]
    fn test_decision_nodes_bluff11() {
        let mut policy = UniformRandomPolicy::new();
        let root_state = Bluff::new_state(1, 1);
        let mut br = TabularBestResponse::new(&mut policy, &root_state, 0, 0.0);

        let mut policy = UniformRandomPolicy::new();
        let first_decision_nodes = DecisionNodeIterator::new(root_state.clone(), &mut policy, 0);

        let mut unrolled_decision_nodes = DiskBackedVec::new();
        br._decision_nodes(&root_state, &mut unrolled_decision_nodes, 1.0);

        for (i, fd) in first_decision_nodes.enumerate() {
            assert_eq!(unrolled_decision_nodes.get(i).0, fd.0);
            assert_relative_eq!(unrolled_decision_nodes.get(i).1, fd.1)
        }
    }
}
