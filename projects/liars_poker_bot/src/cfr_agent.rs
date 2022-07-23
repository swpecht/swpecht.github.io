use log::{debug, info};
use rand::{prelude::SliceRandom, thread_rng};

use crate::{
    agents::{Agent, RandomAgent},
    game::{RPSAction, RPSState, RPS},
};

const NUM_ACTIONS: usize = 3;

/// Regret matching agent.
///
/// Adapted from: http://modelai.gettysburg.edu/2013/cfr/cfr.pdf
pub struct CFRAgent {
    regret_sum: [f64; NUM_ACTIONS],
    strategy: [f64; NUM_ACTIONS],
    strategy_sum: [f64; NUM_ACTIONS],
}

impl CFRAgent {
    pub fn new() -> Self {
        debug!("training CFR agent...");

        let possible_moves = vec![RPSAction::Rock, RPSAction::Paper, RPSAction::Scissors];
        let mut cfr1 = Self {
            regret_sum: [0.0; NUM_ACTIONS],
            strategy: [0.0; NUM_ACTIONS],
            strategy_sum: [0.0; NUM_ACTIONS],
        };

        let mut cfr2 = Self {
            regret_sum: [0.0; NUM_ACTIONS],
            strategy: [0.0; NUM_ACTIONS],
            strategy_sum: [0.0; NUM_ACTIONS],
        };

        const NUM_ITER: usize = 10000;
        info!("training CFR for {} iterations", NUM_ITER);

        for _ in 0..NUM_ITER {
            cfr1.strategy = cfr1.get_strategy(); // update the strategy
            cfr2.strategy = cfr2.get_strategy();

            let g = RPSState::new();
            let my_a = cfr1.play(&g, &possible_moves);
            let op_a = cfr2.play(&g, &possible_moves);

            let action_utility = get_utilities(op_a);
            let a2_u = get_utilities(my_a);

            for i in 0..NUM_ACTIONS {
                cfr1.regret_sum[i] += action_utility[i] - action_utility[my_a as usize];
                cfr2.regret_sum[i] += a2_u[i] - a2_u[op_a as usize];
            }
        }

        // Lock in the average strategy
        cfr1.strategy = cfr1.get_avg_strategy();
        debug!("trained CFR to strategy: {:?}", cfr1.strategy);

        return cfr1;
    }

    /// Get current mixed strategy through regret-matching
    fn get_strategy(&mut self) -> [f64; NUM_ACTIONS] {
        let mut normalizing_sum = 0.0;

        for i in 0..NUM_ACTIONS {
            self.strategy[i] = self.regret_sum[i].max(0.0);
            normalizing_sum += self.strategy[i];
        }

        for i in 0..NUM_ACTIONS {
            if normalizing_sum > 0.0 {
                self.strategy[i] = self.strategy[i] / normalizing_sum;
            } else {
                self.strategy[i] = 1.0 / NUM_ACTIONS as f64;
            }
            self.strategy_sum[i] += self.strategy[i];
        }

        return self.strategy;
    }

    fn get_avg_strategy(&self) -> [f64; NUM_ACTIONS] {
        let mut avg_strategy = [0.0; NUM_ACTIONS];
        let mut normalizing_sum = 0.0;

        for i in 0..NUM_ACTIONS {
            normalizing_sum += self.strategy_sum[i];
        }

        for i in 0..NUM_ACTIONS {
            if normalizing_sum > 0.0 {
                avg_strategy[i] = self.strategy_sum[i] / normalizing_sum;
            } else {
                avg_strategy[i] = 1.0 / NUM_ACTIONS as f64;
            }
        }

        return avg_strategy;
    }
}

impl Agent<RPSState, RPSAction> for CFRAgent {
    fn name(&self) -> &str {
        return "CFRAgent";
    }

    fn play(&self, _: &RPSState, _: &Vec<RPSAction>) -> RPSAction {
        let weights = [
            (RPSAction::Rock, self.strategy[0]),
            (RPSAction::Paper, self.strategy[1]),
            (RPSAction::Scissors, self.strategy[2]),
        ];
        let a = weights.choose_weighted(&mut thread_rng(), |x| x.1);
        let a = a.unwrap().0;

        return a;
    }
}

fn get_utilities(op_a: RPSAction) -> [f64; NUM_ACTIONS] {
    let mut action_utility = [0.0; NUM_ACTIONS];

    action_utility[RPSAction::Rock as usize] = match op_a {
        RPSAction::Rock => 0.0,
        RPSAction::Paper => -1.0,
        RPSAction::Scissors => 2.0,
    };

    action_utility[RPSAction::Paper as usize] = match op_a {
        RPSAction::Rock => 1.0,
        RPSAction::Paper => 0.0,
        RPSAction::Scissors => -2.0,
    };

    action_utility[RPSAction::Scissors as usize] = match op_a {
        RPSAction::Rock => -2.0,
        RPSAction::Paper => 2.0,
        RPSAction::Scissors => 0.0,
    };

    return action_utility;
}
