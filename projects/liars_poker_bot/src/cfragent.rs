use core::num;
use std::{fmt::Debug, iter::zip};

use itertools::Itertools;
use log::{debug, info, trace};
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};
use serde::{Deserialize, Serialize};

use crate::{
    agents::Agent,
    database::{NodeStore, Storage},
    game::{Action, Game, GameState},
};

#[derive(Clone)]
pub struct CFRAgent {
    game: Game,
    rng: StdRng,
    store: NodeStore,
    call_count: usize,
}

impl CFRAgent {
    pub fn new(game: Game, seed: u64, iterations: usize, storage: Storage) -> Self {
        let mut agent = Self {
            game,
            rng: SeedableRng::seed_from_u64(seed),
            store: NodeStore::new(storage),
            call_count: 0,
        };

        // Use CFR to train the agent
        info!("Starting self play for CFR");
        for i in 0..iterations {
            let mut s = (agent.game.new)();
            while s.is_chance_node() {
                let actions = s.legal_actions();
                let a = *actions.choose(&mut agent.rng).unwrap();
                s.apply_action(a);
            }
            agent.cfr(s, 1.0, 1.0);
            trace!("Finished iteration {} for CFR", i);
        }

        // Save the trained policy
        debug!("finished training policy");

        return agent;
    }

    /// Recursive CFR implementation
    ///
    /// Adapted from: https://towardsdatascience.com/counterfactual-regret-minimization-ff4204bf4205
    fn cfr(&mut self, s: Box<dyn GameState>, p0: f32, p1: f32) -> f32 {
        self.call_count += 1;
        if self.call_count % 1000000 == 0 {
            debug!("cfr called {} times", self.call_count);
        }

        // If there is only 1 legal move, can skip most of the steps. No need
        // to store the nodes with only a single action. And the probability to
        // reach the next level is 1 * current probability since there are no
        // other optios.
        let actions = s.legal_actions();
        if actions.len() == 1 {
            let mut new_s = dyn_clone::clone_box(&*s);
            let a = s.legal_actions()[0];
            new_s.apply_action(a);
            return -self.cfr(new_s, p0, p1);
        }

        let cur_player = s.cur_player();

        // Get or create the node
        let info_set = s.information_state_string(cur_player);
        trace!("cfr processing:\t{}", info_set);
        trace!("node:\t{}", s);

        if s.is_terminal() {
            return s.evaluate()[cur_player];
        }

        if !self.contains_node(&info_set) {
            let node = CFRNode::new(info_set.clone(), &actions);
            self.insert_node(info_set.clone(), node);
        }
        let mut node = self.get_node_mut(&info_set).unwrap();

        let param = match cur_player {
            0 => p0,
            _ => p1,
        };
        let strategy = node.get_strategy(param);
        // Save the results
        self.insert_node(info_set.clone(), node.clone());

        let mut util = [0.0; 5];

        let mut node_util = 0.0;
        for &a in &actions {
            let mut new_s = dyn_clone::clone_box(&*s);
            new_s.apply_action(a);
            assert_eq!(node.info_set, s.information_state_string(s.cur_player()));
            let idx = node.get_index(a);

            // the sign of the util received is the opposite of the one computed one layer below
            // because what is positive for one player, is neagtive for the other
            // if player == 0 is making the call, the reach probability of the node below depends on the strategy of player 0
            // so we pass reach probability = p0 * strategy[a], likewise if this is player == 1 then reach probability = p1 * strategy[a]
            // https://colab.research.google.com/drive/1SYNxGdR7UmoxbxY-NSpVsKywLX7YwQMN?usp=sharing#scrollTo=NamPieNiykz1
            util[idx] = match cur_player {
                0 => -self.cfr(new_s, p0 * strategy[idx], p1),
                _ => -self.cfr(new_s, p0, p1 * strategy[idx]),
            };
            node_util += strategy[idx] * util[idx];
        }

        let mut node = self.get_node_mut(&info_set).unwrap();
        // For each action, compute and accumulate counterfactual regret
        for a in actions {
            let idx = node.get_index(a);
            let regret = util[idx] - node_util;
            // for the regret of player 0 is multiplied by the reach p1 of player 1
            // because it is the action of player 1 at the layer above that made the current node reachable
            // conversly if this player 1, then the reach p0 is used.
            node.regret_sum[idx] += match cur_player {
                0 => p1,
                _ => p0,
            } * regret;
        }

        // Save the results
        self.insert_node(info_set, node);

        return node_util;
    }

    fn get_node_mut(&mut self, istate: &str) -> Option<CFRNode> {
        self.store.get_node_mut(istate)
    }

    fn contains_node(&mut self, istate: &String) -> bool {
        return self.store.contains_node(istate);
    }
    fn insert_node(&mut self, istate: String, node: CFRNode) -> Option<CFRNode> {
        self.store.insert_node(istate, node)
    }

    fn get_policy(&mut self, istate: &str) -> Vec<f32> {
        let n = self.get_node_mut(istate).unwrap();
        let p = n.get_average_strategy();
        return p;
    }
}

/// Adapted from: https://towardsdatascience.com/counterfactual-regret-minimization-ff4204bf4205
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CFRNode {
    pub info_set: String,
    /// Stores what action each index represents.
    /// There are at most 5 actions (one for each card in hand)
    pub actions: [usize; 5],
    pub num_actions: usize,
    pub regret_sum: [f32; 5],
    pub strategy: [f32; 5],
    pub strategy_sum: [f32; 5],
}

impl CFRNode {
    pub fn new(info_set: String, legal_moves: &Vec<Action>) -> Self {
        let num_actions = legal_moves.len();
        let mut actions = [0; 5];
        for i in 0..num_actions {
            actions[i] = legal_moves[i]
        }

        Self {
            info_set: info_set.clone(),
            actions: actions,
            num_actions: num_actions,
            regret_sum: [0.0; 5],
            strategy: [0.0; 5],
            strategy_sum: [0.0; 5],
        }
    }

    /// Combine the positive regrets into a strategy.
    ///
    /// Defaults to a uniform action strategy if no regrets are present
    fn get_strategy(&mut self, realization_weight: f32) -> [f32; 5] {
        let num_actions = self.num_actions;
        let mut normalizing_sum = 0.0;

        for i in 0..num_actions {
            self.strategy[i] = self.regret_sum[i].max(0.0);
            normalizing_sum += self.strategy[i];
        }

        for i in 0..num_actions {
            if normalizing_sum > 0.0 {
                self.strategy[i] = self.strategy[i] / normalizing_sum;
            } else {
                self.strategy[i] = 1.0 / num_actions as f32;
            }
            self.strategy_sum[i] += realization_weight * self.strategy[i];
        }

        return self.strategy.clone();
    }

    fn get_average_strategy(&self) -> Vec<f32> {
        let mut avg_strat = vec![0.0; self.strategy.len()];
        let mut normalizing_sum = 0.0;
        for i in 0..self.strategy.len() {
            normalizing_sum += self.strategy_sum[i];
        }

        for i in 0..self.strategy.len() {
            if normalizing_sum > 0.0 {
                avg_strat[i] = self.strategy_sum[i] / normalizing_sum;
            } else {
                avg_strat[i] = 1.0 / self.strategy.len() as f32;
            }
        }

        return avg_strat;
    }

    /// Returns the index storing a given action
    fn get_index(&self, action: Action) -> usize {
        for i in 0..self.actions.len() {
            if action == self.actions[i] {
                return i;
            }
        }
        panic!("action not found")
    }
}

impl Agent for CFRAgent {
    /// Chooses a random action weighted by the policy for the current istate.
    ///
    /// If the I state has not be
    fn step(&mut self, s: &dyn GameState) -> Action {
        // Populate new istates with default value
        let istate = s.information_state_string(s.cur_player());
        // if !self.contains_node(&istate) {
        //     // populate an empty state
        //     warn!("new istate encountered during play: {}", istate);
        //     self.policy.insert(
        //         istate.clone(),
        //         vec![1.0 / self.game.max_actions as f32; self.game.max_actions],
        //     );
        // }

        // Otherwise we choose the action based on weights
        let p = self.get_policy(&istate);
        trace!("evaluating istate {} for {:?}", istate, p);
        let mut weights = vec![0.0; s.legal_actions().len()];
        for &a in &s.legal_actions() {
            weights[a] = p[a];
        }
        return zip(s.legal_actions(), weights)
            .collect_vec()
            .choose_weighted(&mut self.rng, |item| item.1)
            .unwrap()
            .0;
    }
}

#[cfg(test)]
mod tests {
    use super::CFRAgent;
    use crate::{
        agents::Agent,
        database::Storage,
        game::GameState,
        kuhn_poker::{KPAction, KuhnPoker},
    };

    #[test]
    fn cfragent_nash_test() {
        let game = KuhnPoker::game();
        // Verify the nash equilibrium is reached. From https://en.wikipedia.org/wiki/Kuhn_poker
        let mut qa = CFRAgent::new(game, 42, 10000, Storage::Memory);

        // The second player has a single equilibrium strategy:
        // Always betting or calling when having a King
        let w = qa.get_policy("2b");
        check_floats(w[KPAction::Bet as usize], 1.0, 2);

        let w = qa.get_policy("2p");
        check_floats(w[KPAction::Bet as usize], 1.0, 2);

        // when having a Queen, checking if possible, otherwise calling with the probability of 1/3
        let w = qa.get_policy("1p");
        check_floats(w[KPAction::Pass as usize], 1.0, 2);
        let w = qa.get_policy("1b");
        check_floats(w[KPAction::Bet as usize], 0.3333, 1);

        // when having a Jack, never calling and betting with the probability of 1/3.
        let w = qa.get_policy("0b");
        check_floats(w[KPAction::Pass as usize], 1.0, 2);
        let w = qa.get_policy("0p");
        check_floats(w[KPAction::Bet as usize], 0.3333, 1);

        // First player equilibrium
        // In one possible formulation, player one freely chooses the probability
        // {\displaystyle \alpha \in [0,1/3]}{\displaystyle \alpha \in [0,1/3]}
        // with which he will bet when having a Jack (otherwise he checks; if the
        //other player bets, he should always fold).
        let alpha = qa.get_policy("0")[KPAction::Bet as usize];
        assert!(alpha < 0.4);

        let w = qa.get_policy("0pb");
        check_floats(w[KPAction::Pass as usize], 1.0, 2);

        // When having a King, he should bet with the probability of {\displaystyle 3\alpha }3\alpha
        // (otherwise he checks; if the other player bets, he should always call)
        let w = qa.get_policy("2");
        check_floats(w[KPAction::Bet as usize], 3.0 * alpha, 1);

        // He should always check when having a Queen, and if the other player bets after this check,
        // he should call with the probability of {\displaystyle \alpha +1/3}{\displaystyle \alpha +1/3}.
        let w = qa.get_policy("1");
        check_floats(w[KPAction::Pass as usize], 1.0, 2);

        let w = qa.get_policy("1pb");
        // We nudge the optimal weight here to save on iterations for convergence
        check_floats(w[KPAction::Bet as usize], alpha + 0.35, 1);
    }

    fn check_floats(x: f32, y: f32, i: i32) {
        assert_eq!(
            (x * (10.0f32).powi(i)).round() / (10.0f32).powi(i),
            (y * (10.0f32).powi(i)).round() / (10.0f32).powi(i)
        );
    }

    #[test]
    fn cfragent_sample_test() {
        let mut qa = CFRAgent::new(KuhnPoker::game(), 42, 10000, Storage::Memory);
        let mut s = KuhnPoker::new_state();
        s.apply_action(1);
        s.apply_action(0);
        s.apply_action(KPAction::Pass as usize);

        assert_eq!(s.information_state_string(1), "0p");

        let mut action_counter = vec![0; 2];
        for _ in 0..1000 {
            let a = qa.step(&s);
            action_counter[a] += 1;
        }

        // For state 0p, should bet about 33% of the time in nash equilibrium
        assert!(action_counter[KPAction::Bet as usize] > 300);
        assert!(action_counter[KPAction::Bet as usize] < 400);
    }
}
