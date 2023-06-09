use std::marker::PhantomData;

use itertools::Itertools;
use rand::rngs::StdRng;

use crate::{
    actions,
    cfragent::cfrnode::ActionVec,
    game::{GameState, Player},
    policy::Policy,
};
use rayon::prelude::*;

use super::ismcts::{Evaluator, ResampleFromInfoState};

pub struct PIMCTSBot<G, E> {
    n_rollouts: usize,
    rng: StdRng,
    solver: E,
    _phantom: PhantomData<G>,
}

impl<G: GameState + ResampleFromInfoState + Send, E: Evaluator<G> + Clone + Sync> PIMCTSBot<G, E> {
    pub fn new(n_rollouts: usize, solver: E, rng: StdRng) -> Self {
        Self {
            n_rollouts,
            rng,
            solver,
            _phantom: PhantomData,
        }
    }

    fn evaluate_with_worlds(&mut self, maximizing_player: Player, worlds: Vec<G>) -> f64 {
        let sum: f64 = worlds
            // .into_iter()
            .into_par_iter()
            .map(|w| evaluate_with_solver(w, self.solver.clone(), maximizing_player))
            .sum();

        sum / self.n_rollouts as f64
    }
}

fn evaluate_with_solver<G: GameState + Send, E: Evaluator<G>>(
    w: G,
    mut solver: E,
    maximizing_player: Player,
) -> f64 {
    solver.evaluate_player(&w, maximizing_player)
}

impl<G: GameState + ResampleFromInfoState + Send, E: Evaluator<G> + Clone + Sync> Evaluator<G>
    for PIMCTSBot<G, E>
{
    fn evaluate_player(&mut self, gs: &G, maximizing_player: Player) -> f64 {
        let worlds = get_worlds(gs, self.n_rollouts, &mut self.rng);
        self.evaluate_with_worlds(maximizing_player, worlds)
    }

    fn evaluate(&mut self, gs: &G) -> Vec<f64> {
        let mut result = vec![0.0; gs.num_players()];

        let worlds = get_worlds(gs, self.n_rollouts, &mut self.rng);

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

impl<G: GameState + ResampleFromInfoState + Send, E: Evaluator<G> + Clone + Sync> Policy<G>
    for PIMCTSBot<G, E>
{
    fn action_probabilities(&mut self, gs: &G) -> ActionVec<f64> {
        let mut values = Vec::new();
        let actions = actions!(gs);
        let player = gs.cur_player();

        // Use the same set of worlds for all evaluations
        let mut worlds = get_worlds(gs, self.n_rollouts, &mut self.rng);

        for a in actions.clone() {
            worlds.iter_mut().map(|w| w.apply_action(a)).collect_vec();
            let v = self.evaluate_with_worlds(player, worlds.clone());
            values.push(v);
            worlds.iter_mut().map(|w| w.undo()).collect_vec();
        }

        let mut probs = ActionVec::new(&actions);
        let index_of_max = values
            .iter()
            .enumerate()
            // since our other algorithms take the first max element, we reverse the order so max by
            // also returns the first element
            .rev()
            .max_by(|(_, a), (_, b)| a.total_cmp(b))
            .map(|(index, _)| index)
            .unwrap();

        probs[actions[index_of_max]] = 1.0;

        probs
    }
}

pub(super) fn get_worlds<G: GameState + ResampleFromInfoState>(
    gs: &G,
    n: usize,
    rng: &mut StdRng,
) -> Vec<G> {
    let mut worlds = Vec::with_capacity(n);
    for _ in 0..n {
        worlds.push(gs.resample_from_istate(gs.cur_player(), rng));
    }
    worlds
}

#[cfg(test)]
mod tests {

    use rand::SeedableRng;

    use crate::{
        algorithms::{ismcts::Evaluator, open_hand_solver::OpenHandSolver, pimcts::PIMCTSBot},
        game::{
            euchre::EuchreGameState,
            kuhn_poker::{KPAction, KuhnPoker},
        },
        policy::Policy,
    };

    #[test]
    fn test_pimcts_kuhn() {
        let mut agent = PIMCTSBot::new(100, OpenHandSolver::new(), SeedableRng::seed_from_u64(109));
        let gs = KuhnPoker::from_actions(&[KPAction::Jack, KPAction::Queen]);
        assert_eq!(agent.evaluate(&gs), vec![-1.0, 1.0]);

        let mut agent = PIMCTSBot::new(100, OpenHandSolver::new(), SeedableRng::seed_from_u64(109));
        let gs = KuhnPoker::from_actions(&[KPAction::Queen, KPAction::Jack]);
        assert_eq!(agent.evaluate(&gs), vec![0.0, 0.0]);

        let mut agent = PIMCTSBot::new(100, OpenHandSolver::new(), SeedableRng::seed_from_u64(109));
        let gs = KuhnPoker::from_actions(&[KPAction::King, KPAction::Jack]);
        assert_eq!(agent.evaluate(&gs), vec![1.0, -1.0]);
    }

    #[test]
    fn pimcts_consistency() {
        let gs = EuchreGameState::from(
            "JsQs9hKhAh|TcQcKcThAd|9cJc9sAsQh|KsJh9dJdQd|Kd|PPPT|Ks|JsThAsJh|JdQsAd9c|Qd",
        );

        let mut policy = PIMCTSBot::new(10, OpenHandSolver::new(), SeedableRng::seed_from_u64(42));
        let result = policy.action_probabilities(&gs);

        for _ in 0..100 {
            let mut policy =
                PIMCTSBot::new(10, OpenHandSolver::new(), SeedableRng::seed_from_u64(42));
            assert_eq!(policy.action_probabilities(&gs), result);
        }
    }
}
