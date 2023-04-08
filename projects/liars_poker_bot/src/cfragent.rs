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
    cfragent::{cfr::Algorithm, cfrcs::CFRCS},
    database::{file_backend::FileBackend, NodeStore, Storage},
    game::{Action, Game, GameState},
    istate::IStateKey,
};

#[derive(Clone)]
pub struct CFRAgent<T: GameState> {
    game: Game<T>,
    rng: StdRng,
    store: NodeStore<FileBackend>,
    _phantom: PhantomData<T>,
}

impl<T: GameState> CFRAgent<T> {
    pub fn new(game: Game<T>, seed: u64, iterations: usize, storage: Storage) -> Self {
        let mut agent = Self {
            game,
            rng: SeedableRng::seed_from_u64(seed),
            store: NodeStore::new(FileBackend::new(storage)),
            _phantom: PhantomData,
        };

        // Use CFR to train the agent
        info!("Starting self play for CFR");
        let mut alg = CFRCS::new(seed);
        for i in 0..iterations {
            let gs = (agent.game.new)();

            for i in 0..agent.game.max_players {
                alg.run(&mut agent.store, &gs, i);
            }

            info!("Finished iteration {} for CFR", i);
        }

        // Save the trained policy
        debug!("finished training policy");

        return agent;
    }

    fn get_node_mut(&mut self, istate: &IStateKey) -> Option<CFRNode> {
        self.store.get_node_mut(istate)
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
        game::GameState,
        kuhn_poker::{KPAction, KuhnPoker},
    };

    #[test]
    fn cfragent_sample_test() {
        let mut qa = CFRAgent::new(KuhnPoker::game(), 42, 50000, Storage::Temp);
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
