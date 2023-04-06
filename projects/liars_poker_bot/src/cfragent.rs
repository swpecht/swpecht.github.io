pub mod bestresponse;
pub mod cfr;
pub mod cfrcs;

use std::{fmt::Debug, iter::zip, marker::PhantomData};

use itertools::Itertools;
use log::{debug, info, trace};
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};
use serde::{Deserialize, Serialize};

use crate::{
    agents::Agent,
    database::{file_backend::FileBackend, NodeStore, Storage},
    game::{Action, Game, GameState},
    istate::IStateKey,
};

#[derive(Clone)]
pub struct CFRAgent<T: GameState> {
    game: Game<T>,
    rng: StdRng,
    store: NodeStore<FileBackend>,
    call_count: usize,
    _phantom: PhantomData<T>,
}

impl<T: GameState> CFRAgent<T> {
    pub fn new(game: Game<T>, seed: u64, iterations: usize, storage: Storage) -> Self {
        let mut agent = Self {
            game,
            rng: SeedableRng::seed_from_u64(seed),
            store: NodeStore::new(FileBackend::new(storage)),
            call_count: 0,
            _phantom: PhantomData,
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
            info!("Finished iteration {} for CFR", i);
        }

        // Save the trained policy
        debug!("finished training policy");

        return agent;
    }

    /// Recursive CFR implementation
    ///
    /// Adapted from: https://towardsdatascience.com/counterfactual-regret-minimization-ff4204bf4205
    fn cfr(&mut self, s: T, p0: f32, p1: f32) -> f32 {
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
            let mut new_s = s.clone();
            let a = s.legal_actions()[0];
            new_s.apply_action(a);
            return -self.cfr(new_s, p0, p1);
        }

        let cur_player = s.cur_player();

        // Get or create the node
        let info_set = s.istate_key(cur_player);
        trace!("cfr processing:\t{}", info_set.to_string());
        trace!("node:\t{}", s);

        if s.is_terminal() {
            return s.evaluate()[cur_player];
        }

        let mut node = self
            .get_node_mut(&info_set)
            .unwrap_or(CFRNode::new(info_set, &actions));

        let param = match cur_player {
            0 | 2 => p0,
            1 | 3 => p1,
            _ => panic!("invalid player"),
        };
        let strategy = node.get_move_prob(param);

        let mut util = [0.0; 5];

        let mut node_util = 0.0;
        for &a in &actions {
            let mut new_s = s.clone();
            new_s.apply_action(a);
            let idx = node.get_index(a);

            // the sign of the util received is the opposite of the one computed one layer below
            // because what is positive for one player, is neagtive for the other
            // if player == 0 is making the call, the reach probability of the node below depends on the strategy of player 0
            // so we pass reach probability = p0 * strategy[a], likewise if this is player == 1 then reach probability = p1 * strategy[a]
            // https://colab.research.google.com/drive/1SYNxGdR7UmoxbxY-NSpVsKywLX7YwQMN?usp=sharing#scrollTo=NamPieNiykz1
            util[idx] = match cur_player {
                0 | 2 => -self.cfr(new_s, p0 * strategy[idx], p1),
                _ => -self.cfr(new_s, p0, p1 * strategy[idx]),
            };
            node_util += strategy[idx] * util[idx];
        }

        // For each action, compute and accumulate counterfactual regret
        for a in actions {
            let idx = node.get_index(a);
            let regret = util[idx] - node_util;
            // for the regret of player 0 is multiplied by the reach p1 of player 1
            // because it is the action of player 1 at the layer above that made the current node reachable
            // conversly if this player 1, then the reach p0 is used.
            node.regret_sum[idx] += match cur_player {
                0 | 2 => p1,
                _ => p0,
            } * regret;
        }

        // Save the results
        self.insert_node(info_set, node);

        return node_util;
    }

    fn get_node_mut(&mut self, istate: &IStateKey) -> Option<CFRNode> {
        self.store.get_node_mut(istate)
    }

    fn insert_node(&mut self, istate: IStateKey, node: CFRNode) -> Option<CFRNode> {
        self.store.insert_node(istate, node)
    }

    fn get_policy(&mut self, istate: &IStateKey) -> Vec<f32> {
        let n = self.get_node_mut(istate).unwrap();
        let p = n.get_average_strategy();
        return p;
    }
}

/// Adapted from: https://towardsdatascience.com/counterfactual-regret-minimization-ff4204bf4205
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CFRNode {
    pub info_set: IStateKey,
    /// Stores what action each index represents.
    /// There are at most 5 actions (one for each card in hand)
    pub actions: [usize; 5],
    pub num_actions: usize,
    pub regret_sum: [f32; 5],
    pub move_prob: [f32; 5],
    pub total_move_prob: [f32; 5],
}

impl CFRNode {
    pub fn new(info_set: IStateKey, legal_moves: &Vec<Action>) -> Self {
        let num_actions = legal_moves.len();
        let mut actions = [0; 5];
        for i in 0..num_actions {
            actions[i] = legal_moves[i]
        }

        Self {
            info_set: info_set,
            actions: actions,
            num_actions: num_actions,
            regret_sum: [0.0; 5],
            move_prob: [0.0; 5],
            total_move_prob: [0.0; 5],
        }
    }

    /// Combine the positive regrets into a strategy.
    ///
    /// Defaults to a uniform action strategy if no regrets are present
    fn get_move_prob(&mut self, realization_weight: f32) -> [f32; 5] {
        let num_actions = self.num_actions;
        let mut normalizing_sum = 0.0;

        for i in 0..num_actions {
            self.move_prob[i] = self.regret_sum[i].max(0.0);
            normalizing_sum += self.move_prob[i];
        }

        for i in 0..num_actions {
            if normalizing_sum > 0.0 {
                self.move_prob[i] = self.move_prob[i] / normalizing_sum;
            } else {
                self.move_prob[i] = 1.0 / num_actions as f32;
            }
            self.total_move_prob[i] += realization_weight * self.move_prob[i];
        }

        return self.move_prob.clone();
    }

    fn get_average_strategy(&self) -> Vec<f32> {
        let mut avg_strat = vec![0.0; self.move_prob.len()];
        let mut normalizing_sum = 0.0;
        for i in 0..self.move_prob.len() {
            normalizing_sum += self.total_move_prob[i];
        }

        for i in 0..self.move_prob.len() {
            if normalizing_sum > 0.0 {
                avg_strat[i] = self.total_move_prob[i] / normalizing_sum;
            } else {
                avg_strat[i] = 1.0 / self.move_prob.len() as f32;
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

impl<T: GameState> Agent<T> for CFRAgent<T> {
    /// Chooses a random action weighted by the policy for the current istate.
    ///
    /// If the I state has not be
    fn step(&mut self, s: &T) -> Action {
        let istate = s.istate_key(s.cur_player());

        let p = self.get_policy(&istate);
        trace!("evaluating istate {} for {:?}", istate.to_string(), p);
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
        game::{Action, GameState},
        istate::IStateKey,
        kuhn_poker::{KPAction, KuhnPoker},
    };

    #[test]
    fn cfragent_nash_test() {
        let game = KuhnPoker::game();
        // Verify the nash equilibrium is reached. From https://en.wikipedia.org/wiki/Kuhn_poker
        let mut qa = CFRAgent::new(game, 42, 10000, Storage::Temp);

        // The second player has a single equilibrium strategy:
        // Always betting or calling when having a King
        // 2b
        let k = get_key(&[2, 0, KPAction::Bet as usize]);
        let w = qa.get_policy(&k);
        check_floats(w[KPAction::Bet as usize], 1.0, 2);

        // 2p
        let k = get_key(&[2, 0, KPAction::Pass as usize]);
        let w = qa.get_policy(&k);
        check_floats(w[KPAction::Bet as usize], 1.0, 2);

        // when having a Queen, checking if possible, otherwise calling with the probability of 1/3
        // 1p
        let k = get_key(&[1, 0, KPAction::Pass as usize]);
        let w = qa.get_policy(&k);
        check_floats(w[KPAction::Pass as usize], 1.0, 2);
        // 1b
        let k = get_key(&[1, 0, KPAction::Bet as usize]);
        let w = qa.get_policy(&k);
        check_floats(w[KPAction::Bet as usize], 0.3333, 2);

        // when having a Jack, never calling and betting with the probability of 1/3.
        // 0b
        let k = get_key(&[0, 1, KPAction::Bet as usize]);
        let w = qa.get_policy(&k);
        check_floats(w[KPAction::Pass as usize], 1.0, 2);
        // 0p
        let k = get_key(&[0, 1, KPAction::Pass as usize]);
        let w = qa.get_policy(&k);
        check_floats(w[KPAction::Bet as usize], 0.3333, 2);

        // First player equilibrium
        // In one possible formulation, player one freely chooses the probability
        // {\displaystyle \alpha \in [0,1/3]}{\displaystyle \alpha \in [0,1/3]}
        // with which he will bet when having a Jack (otherwise he checks; if the
        //other player bets, he should always fold).
        // 0
        let k = get_key(&[0]);
        let alpha = qa.get_policy(&k)[KPAction::Bet as usize];
        assert!(alpha < 0.4);

        // 0pb
        let k = get_key(&[0, 1, KPAction::Pass as usize, KPAction::Bet as usize]);
        let w = qa.get_policy(&k);
        check_floats(w[KPAction::Pass as usize], 1.0, 2);

        // When having a King, he should bet with the probability of {\displaystyle 3\alpha }3\alpha
        // (otherwise he checks; if the other player bets, he should always call)
        // 2
        let k = get_key(&[2]);
        let w = qa.get_policy(&k);
        check_floats(w[KPAction::Bet as usize], 3.0 * alpha, 2);

        // He should always check when having a Queen, and if the other player bets after this check,
        // he should call with the probability of {\displaystyle \alpha +1/3}{\displaystyle \alpha +1/3}.
        // 1
        let k = get_key(&[1]);
        let w = qa.get_policy(&k);
        check_floats(w[KPAction::Pass as usize], 1.0, 2);

        // 1pb
        let k = get_key(&[1, 0, KPAction::Pass as usize, KPAction::Bet as usize]);
        let w = qa.get_policy(&k);
        // We nudge the optimal weight here to save on iterations for convergence
        check_floats(w[KPAction::Bet as usize], alpha + 0.35, 2);
    }

    fn check_floats(x: f32, y: f32, i: i32) {
        let diff = (x * (10.0f32).powi(i)) - (y * (10.0f32).powi(i));

        if diff > 2.0 {
            panic!("expected: {} got: {}", x, y);
        }
    }

    /// Gets a key for player 0 of a new gamestate after applying the passed actions
    fn get_key(actions: &[Action]) -> IStateKey {
        let mut g = (KuhnPoker::game().new)();
        for &a in actions {
            g.apply_action(a);
        }

        return g.istate_key(0);
    }

    #[test]
    fn cfragent_sample_test() {
        let mut qa = CFRAgent::new(KuhnPoker::game(), 42, 10000, Storage::Temp);
        let mut s = KuhnPoker::new_state();
        s.apply_action(1);
        s.apply_action(0);
        s.apply_action(KPAction::Pass as usize);

        assert_eq!(s.istate_string(1), "0p");

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
