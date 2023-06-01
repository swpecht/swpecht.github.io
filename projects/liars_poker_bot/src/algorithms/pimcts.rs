use std::marker::PhantomData;

use rand::rngs::StdRng;

use crate::{
    actions,
    cfragent::cfrnode::ActionVec,
    game::{GameState, Player},
    policy::Policy,
};
use rayon::prelude::*;

use super::{
    ismcts::{Evaluator, ResampleFromInfoState},
    open_hand_solver::OpenHandSolver,
};

pub struct PIMCTSBot<G> {
    n_rollouts: usize,
    rng: StdRng,
    solver: OpenHandSolver,
    _phantom: PhantomData<G>,
}

impl<G: GameState + ResampleFromInfoState + Send> PIMCTSBot<G> {
    pub fn new(n_rollouts: usize, rng: StdRng) -> Self {
        Self {
            n_rollouts,
            rng,
            solver: OpenHandSolver::new(),
            _phantom: PhantomData,
        }
    }

    pub fn evaluate_player(&mut self, gs: &G, maximizing_player: Player) -> f64 {
        let worlds = self.get_worlds(gs);
        self.evaluate_with_worlds(maximizing_player, worlds)
    }

    fn evaluate_with_worlds(&mut self, maximizing_player: Player, worlds: Vec<G>) -> f64 {
        // clear the transposition table since it was generated with a different set of worlds
        // this can be removed if we can iterate over all possible worlds for a given state
        // self.cache.transposition_table.clear();

        let sum: f64 = worlds
            // .into_iter()
            .into_par_iter()
            .map(|w| evaluate_with_solver(w, self.solver.clone(), maximizing_player))
            .sum();

        sum / self.n_rollouts as f64
    }

    fn get_worlds(&mut self, gs: &G) -> Vec<G> {
        let mut worlds = Vec::with_capacity(self.n_rollouts);
        for _ in 0..self.n_rollouts {
            worlds.push(gs.resample_from_istate(gs.cur_player(), &mut self.rng));
        }
        worlds
    }
}

fn evaluate_with_solver<G: GameState + Send>(
    w: G,
    mut solver: OpenHandSolver,
    maximizing_player: Player,
) -> f64 {
    solver.evaluate_player(&w, maximizing_player)
}

impl<G: GameState + ResampleFromInfoState + Send> Evaluator<G> for PIMCTSBot<G> {
    fn evaluate(&mut self, gs: &G) -> Vec<f64> {
        let mut result = vec![0.0; gs.num_players()];

        let worlds = self.get_worlds(gs);

        for (i, r) in result.iter_mut().enumerate().take(2) {
            *r = self.evaluate_with_worlds(i, worlds.clone());
        }
        // Only support evaluating for 2 teams, so we can copy over the results
        for i in 2..result.len() {
            result[i] = result[i % 2];
        }

        result
    }

    fn prior(&mut self, gs: &G) -> ActionVec<f64> {
        self.action_probabilities(gs)
    }
}

impl<G: GameState + ResampleFromInfoState + Send> Policy<G> for PIMCTSBot<G> {
    fn action_probabilities(&mut self, gs: &G) -> ActionVec<f64> {
        let mut values = Vec::new();
        let actions = actions!(gs);
        let mut gs = gs.clone();
        let player = gs.cur_player();

        for a in actions.clone() {
            gs.apply_action(a);
            let v = self.evaluate_player(&gs, player);
            values.push(v);
            gs.undo();
        }

        let mut probs = ActionVec::new(&actions);

        let index_of_max = values
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.total_cmp(b))
            .map(|(index, _)| index)
            .unwrap();

        probs[actions[index_of_max]] = 1.0;

        probs
    }
}

#[cfg(test)]
mod tests {

    use rand::SeedableRng;

    use crate::{
        algorithms::{ismcts::Evaluator, pimcts::PIMCTSBot},
        game::kuhn_poker::{KPAction, KuhnPoker},
    };

    #[test]
    fn test_open_hand_solver_kuhn() {
        let mut evaluator = PIMCTSBot::new(100, SeedableRng::seed_from_u64(109));
        let gs = KuhnPoker::from_actions(&[KPAction::Jack, KPAction::Queen]);
        assert_eq!(evaluator.evaluate(&gs), vec![-1.0, 1.0]);

        let mut evaluator = PIMCTSBot::new(100, SeedableRng::seed_from_u64(109));
        let gs = KuhnPoker::from_actions(&[KPAction::Queen, KPAction::Jack]);
        assert_eq!(evaluator.evaluate(&gs), vec![0.0, 0.0]);

        let mut evaluator = PIMCTSBot::new(100, SeedableRng::seed_from_u64(109));
        let gs = KuhnPoker::from_actions(&[KPAction::King, KPAction::Jack]);
        assert_eq!(evaluator.evaluate(&gs), vec![1.0, -1.0]);
    }
}
