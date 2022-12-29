use std::{collections::HashMap, iter::zip};

use itertools::Itertools;
use log::{debug, info, trace, warn};
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

use crate::{
    agents::Agent,
    game::{Action, Game, GameState},
    kuhn_poker::{KPGameState, KuhnPoker},
};

#[derive(Clone)]
pub struct CFRAgent {
    game: Game,
    rng: StdRng,
    policy: HashMap<String, Vec<f32>>,
    nodes: HashMap<String, CFRNode>,
}

impl CFRAgent {
    pub fn new(seed: u64, iterations: usize) -> Self {
        let game = Game {
            max_players: 2,
            max_actions: 3,
        };

        let mut agent = Self {
            game,
            rng: SeedableRng::seed_from_u64(seed),
            policy: HashMap::new(),
            nodes: HashMap::new(),
        };

        // Use CFR to train the agent
        info!("Starting self play for CFR");
        for i in 0..iterations {
            let mut s = KuhnPoker::new();
            while s.is_chance_node() {
                let actions = s.legal_actions();
                let a = *actions.choose(&mut agent.rng).unwrap();
                s.apply_action(a);
            }
            let history = Vec::new();
            agent.cfr(&s, history, 1.0, 1.0);
            trace!(
                "Finished iteration {} for CFR, nodes: {:#?}",
                i,
                agent.nodes
            );
        }

        // Save the trained policy
        for (_, n) in &agent.nodes {
            agent
                .policy
                .insert(n.info_set.clone(), n.get_average_strategy());
        }
        debug!("finished training, policy: {:#?}", agent.policy);

        return agent;
    }

    /// Recursive CFR implementation
    ///
    /// Adapted from: https://towardsdatascience.com/counterfactual-regret-minimization-ff4204bf4205
    fn cfr(&mut self, s: &KPGameState, history: Vec<usize>, p0: f32, p1: f32) -> f32 {
        let cur_player = s.cur_player();
        if s.is_terminal() {
            return s.evaluate()[cur_player];
        }

        // Get or create the node
        let info_set = s.information_state_string(cur_player);
        if !self.nodes.contains_key(&info_set) {
            let node = CFRNode {
                info_set: info_set.clone(),
                regret_sum: vec![0.0; self.game.max_actions],
                strategy: vec![0.0; self.game.max_actions],
                strategy_sum: vec![0.0; self.game.max_actions],
            };
            self.nodes.insert(info_set.clone(), node);
        }
        let node = self.nodes.get_mut(&info_set).unwrap();

        let param = match cur_player {
            0 => p0,
            _ => p1,
        };
        let strategy = node.get_strategy(param, s);
        let actions = s.legal_actions();
        let mut util = vec![0.0; self.game.max_actions];

        let mut node_util = 0.0;
        for &a in &actions {
            let mut new_s = s.clone();
            new_s.apply_action(a);
            let mut next_history = history.clone();
            next_history.push(a);

            // the sign of the util received is the opposite of the one computed one layer below
            // because what is positive for one player, is neagtive for the other
            // if player == 0 is making the call, the reach probability of the node below depends on the strategy of player 0
            // so we pass reach probability = p0 * strategy[a], likewise if this is player == 1 then reach probability = p1 * strategy[a]
            // https://colab.research.google.com/drive/1SYNxGdR7UmoxbxY-NSpVsKywLX7YwQMN?usp=sharing#scrollTo=NamPieNiykz1
            util[a] = match cur_player {
                0 => -self.cfr(&new_s, next_history, p0 * strategy[a], p1),
                _ => -self.cfr(&new_s, next_history, p0, p1 * strategy[a]),
            };
            node_util += strategy[a] * util[a];
        }

        let node = self.nodes.get_mut(&info_set).unwrap();
        // For each action, compute and accumulate counterfactual regret
        for a in actions {
            let regret = util[a] - node_util;
            // for the regret of player 0 is multiplied by the reach p1 of player 1
            // because it is the action of player 1 at the layer above that made the current node reachable
            // conversly if this player 1, then the reach p0 is used.
            node.regret_sum[a] += match cur_player {
                0 => p1,
                _ => p0,
            } * regret;
        }

        return node_util;
    }
}

/// Adapted from: https://towardsdatascience.com/counterfactual-regret-minimization-ff4204bf4205
#[derive(Debug, Clone)]
struct CFRNode {
    info_set: String,
    regret_sum: Vec<f32>,
    strategy: Vec<f32>,
    strategy_sum: Vec<f32>,
}

impl CFRNode {
    /// Combine the positive regrets into a strategy.
    ///
    /// Defaults to a uniform action strategy if no regrets are present
    fn get_strategy(&mut self, realization_weight: f32, s: &dyn GameState) -> Vec<f32> {
        let actions = s.legal_actions();
        let num_actions = actions.len();
        let mut normalizing_sum = 0.0;

        for &a in &actions {
            self.strategy[a] = self.regret_sum[a].max(0.0);
            normalizing_sum += self.strategy[a];
        }

        for &a in &actions {
            if normalizing_sum > 0.0 {
                self.strategy[a] = self.strategy[a] / normalizing_sum;
            } else {
                self.strategy[a] = 1.0 / num_actions as f32;
            }
            self.strategy_sum[a] += realization_weight * self.strategy[a];
        }

        return self.strategy.clone();
    }

    fn get_average_strategy(&self) -> Vec<f32> {
        let mut avg_strat = vec![0.0; self.strategy.len()];
        let mut normalizing_sum = 0.0;
        for a in 0..self.strategy.len() {
            normalizing_sum += self.strategy_sum[a];
        }

        for a in 0..self.strategy.len() {
            if normalizing_sum > 0.0 {
                avg_strat[a] = self.strategy_sum[a] / normalizing_sum;
            } else {
                avg_strat[a] = 1.0 / self.strategy.len() as f32;
            }
        }

        return avg_strat;
    }
}

impl Agent for CFRAgent {
    /// Chooses a random action weighted by the policy for the current istate.
    ///
    /// If the I state has not be
    fn step(&mut self, s: &dyn GameState) -> Action {
        // Populate new istates with default value
        let istate = s.information_state_string(s.cur_player());
        if !self.policy.contains_key(&istate) {
            // populate an empty state
            warn!("new istate encountered during play: {}", istate);
            self.policy.insert(
                istate.clone(),
                vec![1.0 / self.game.max_actions as f32; self.game.max_actions],
            );
        }

        // Otherwise we choose the action with the highest value
        let p = self.policy.get(&istate).unwrap().clone();
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
        game::GameState,
        kuhn_poker::{KPAction, KuhnPoker},
    };

    #[test]
    fn cfragent_nash_test() {
        // Verify the nash equilibrium is reached. From https://en.wikipedia.org/wiki/Kuhn_poker
        let qa = CFRAgent::new(42, 10000);

        // The second player has a single equilibrium strategy:
        // Always betting or calling when having a King
        let w = qa.policy.get("2b").unwrap();
        check_floats(w[KPAction::Bet as usize], 1.0, 2);

        let w = qa.policy.get("2p").unwrap();
        check_floats(w[KPAction::Bet as usize], 1.0, 2);

        // when having a Queen, checking if possible, otherwise calling with the probability of 1/3
        let w = qa.policy.get("1p").unwrap();
        check_floats(w[KPAction::Pass as usize], 1.0, 2);
        let w = qa.policy.get("1b").unwrap();
        check_floats(w[KPAction::Bet as usize], 0.3333, 1);

        // when having a Jack, never calling and betting with the probability of 1/3.
        let w = qa.policy.get("0b").unwrap();
        check_floats(w[KPAction::Pass as usize], 1.0, 2);
        let w = qa.policy.get("0p").unwrap();
        check_floats(w[KPAction::Bet as usize], 0.3333, 1);

        // First player equilibrium
        // In one possible formulation, player one freely chooses the probability
        // {\displaystyle \alpha \in [0,1/3]}{\displaystyle \alpha \in [0,1/3]}
        // with which he will bet when having a Jack (otherwise he checks; if the
        //other player bets, he should always fold).
        let alpha = qa.policy.get("0").unwrap()[KPAction::Bet as usize];
        assert!(alpha < 0.4);

        let w = qa.policy.get("0pb").unwrap();
        check_floats(w[KPAction::Pass as usize], 1.0, 2);

        // When having a King, he should bet with the probability of {\displaystyle 3\alpha }3\alpha
        // (otherwise he checks; if the other player bets, he should always call)
        let w = qa.policy.get("2").unwrap();
        check_floats(w[KPAction::Bet as usize], 3.0 * alpha, 1);

        // He should always check when having a Queen, and if the other player bets after this check,
        // he should call with the probability of {\displaystyle \alpha +1/3}{\displaystyle \alpha +1/3}.
        let w = qa.policy.get("1").unwrap();
        check_floats(w[KPAction::Pass as usize], 1.0, 2);

        let w = qa.policy.get("1pb").unwrap();
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
        let mut qa = CFRAgent::new(42, 10000);
        let mut s = KuhnPoker::new();
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
