use rand::rngs::StdRng;
use rayon::prelude::*;

use crate::{
    actions,
    alloc::Pool,
    cfragent::cfrnode::ActionVec,
    game::{Action, GameState, Player},
    policy::Policy,
};

use super::{
    alphamu::Team,
    ismcts::{Evaluator, ResampleFromInfoState},
};

/// Rollout solver that assumes perfect information by playing against open hands
///
/// This is an adaption of a double dummy solver for bridge
/// http://privat.bahnhof.se/wb758135/bridge/Alg-dds_x.pdf
pub struct OpenHandSolver {
    n_rollouts: usize,
    rng: StdRng,
}

impl OpenHandSolver {
    pub fn new(n_rollouts: usize, rng: StdRng) -> Self {
        Self { rng, n_rollouts }
    }

    pub fn set_rollout(&mut self, n_rollouts: usize) {
        self.n_rollouts = n_rollouts;
    }
}

impl<G: GameState + ResampleFromInfoState> Evaluator<G> for OpenHandSolver {
    fn evaluate(&mut self, gs: &G) -> Vec<f64> {
        let mut result = vec![0.0; gs.num_players()];

        let mut worlds = Vec::with_capacity(self.n_rollouts);
        for _ in 0..self.n_rollouts {
            worlds.push(gs.resample_from_istate(gs.cur_player(), &mut self.rng));
        }

        // let r: f64 = (0..2).into_par_iter().map(|x| 2.0 * x as f64).sum();

        worlds.iter().for_each(|world| {
            for (i, r) in result.iter_mut().enumerate().take(2) {
                let (v, _) = alpha_beta_search(world.clone(), i);
                *r += v;
            }

            // Only support evaluating for 2 teams, so we can copy over the results
            for i in 2..result.len() {
                result[i] = result[i % 2];
            }
        });

        for r in result.iter_mut() {
            *r /= self.n_rollouts as f64;
        }
        result
    }

    fn prior(&mut self, gs: &G) -> ActionVec<f64> {
        self.action_probabilities(gs)
    }
}

impl<G: GameState + ResampleFromInfoState> Policy<G> for OpenHandSolver {
    fn action_probabilities(&mut self, gs: &G) -> ActionVec<f64> {
        let maximizing_player = gs.cur_player();
        let actions = actions!(gs);
        let mut policy = ActionVec::new(&actions);
        let mut gs = gs.clone();
        for a in &actions {
            gs.apply_action(*a);
            policy[*a] = self.evaluate(&gs)[maximizing_player];
            gs.undo()
        }

        let mut total = 0.0;
        for &a in &actions {
            total += policy[a];
        }

        for &a in &actions {
            policy[a] /= total;
        }
        policy
    }
}

pub fn alpha_beta_search<G: GameState>(
    mut gs: G,
    maximizing_player: Player,
) -> (f64, Option<Action>) {
    let maximizing_team = Team::from(maximizing_player);
    alpha_beta(
        &mut gs,
        maximizing_team,
        f64::NEG_INFINITY,
        f64::INFINITY,
        &mut AlphaBetaCache::default(),
    )
}

/// Helper struct to speeding up alpha_beta search
struct AlphaBetaCache {
    vec_pool: Pool<Vec<Action>>,
}

impl Default for AlphaBetaCache {
    fn default() -> Self {
        Self {
            vec_pool: Pool::new(|| Vec::with_capacity(5)),
        }
    }
}

/// An alpha-beta algorithm.
/// Implements a min-max algorithm with alpha-beta pruning.
/// See for example https://en.wikipedia.org/wiki/Alpha-beta_pruning
///
/// Adapted from openspiel:
///     https://github.com/deepmind/open_spiel/blob/master/open_spiel/python/algorithms/minimax.py
fn alpha_beta<G: GameState>(
    gs: &mut G,
    maximizing_team: Team,
    mut alpha: f64,
    mut beta: f64,
    cache: &mut AlphaBetaCache,
) -> (f64, Option<Action>) {
    if gs.is_terminal() {
        let v = gs.evaluate(maximizing_team as usize);
        return (v, None);
    }

    let mut actions = cache.vec_pool.detach();
    if gs.is_chance_node() {
        todo!("add support for chance nodes")
    }

    let player = gs.cur_player();
    let mut best_action = None;
    let team: Team = player.into();

    if team == maximizing_team {
        let mut value = f64::NEG_INFINITY;
        gs.legal_actions(&mut actions);
        for a in &actions {
            gs.apply_action(*a);
            let (child_value, _) = alpha_beta(gs, maximizing_team, alpha, beta, cache);
            gs.undo();
            if child_value > value {
                value = child_value;
                best_action = Some(*a);
            }
            alpha = alpha.max(value);
            if alpha >= beta {
                break; // Beta cut-off
            }
        }

        actions.clear();
        cache.vec_pool.attach(actions);
        (value, best_action)
    } else {
        let mut value = f64::INFINITY;
        gs.legal_actions(&mut actions);
        for a in &actions {
            gs.apply_action(*a);
            let (child_value, _) = alpha_beta(gs, maximizing_team, alpha, beta, cache);
            gs.undo();
            if child_value < value {
                value = child_value;
                best_action = Some(*a);
            }
            beta = beta.min(value);
            if alpha >= beta {
                break;
            }
        }

        actions.clear();
        cache.vec_pool.attach(actions);
        (value, best_action)
    }
}
#[cfg(test)]
mod tests {
    use rand::SeedableRng;

    use crate::{
        algorithms::{ismcts::Evaluator, open_hand_solver::OpenHandSolver},
        game::{
            bluff::{Bluff, BluffActions, Dice},
            kuhn_poker::{KPAction, KuhnPoker},
            GameState,
        },
    };

    use super::alpha_beta_search;

    #[test]
    fn test_min_max_kuhn_poker() {
        let gs = KuhnPoker::from_actions(&[KPAction::Jack, KPAction::Queen]);
        let (v, a) = alpha_beta_search(gs, 0);
        assert_eq!(v, -1.0);
        assert_eq!(a.unwrap(), KPAction::Pass.into());

        let gs = KuhnPoker::from_actions(&[KPAction::King, KPAction::Queen]);
        let (v, a) = alpha_beta_search(gs, 0);
        assert_eq!(v, 1.0);
        assert_eq!(a.unwrap(), KPAction::Bet.into());

        let gs = KuhnPoker::from_actions(&[
            KPAction::King,
            KPAction::Queen,
            KPAction::Pass,
            KPAction::Bet,
        ]);
        let (v, a) = alpha_beta_search(gs, 0);
        assert_eq!(v, 2.0);
        assert_eq!(a.unwrap(), KPAction::Bet.into());
    }

    #[test]
    fn test_min_max_bluff_2_2() {
        let mut gs = Bluff::new_state(2, 2);
        gs.apply_action(BluffActions::Roll(Dice::Two).into());
        gs.apply_action(BluffActions::Roll(Dice::Three).into());
        gs.apply_action(BluffActions::Roll(Dice::Two).into());
        gs.apply_action(BluffActions::Roll(Dice::Three).into());

        let (v, a) = alpha_beta_search(gs, 0);
        assert_eq!(v, 1.0);
        assert_eq!(
            BluffActions::from(a.unwrap()),
            BluffActions::Bid(2, Dice::Three)
        );

        let mut gs = Bluff::new_state(2, 2);
        gs.apply_action(BluffActions::Roll(Dice::Two).into());
        gs.apply_action(BluffActions::Roll(Dice::Wild).into());
        gs.apply_action(BluffActions::Roll(Dice::Three).into());
        gs.apply_action(BluffActions::Roll(Dice::Three).into());

        let (v, a) = alpha_beta_search(gs, 0);
        assert_eq!(v, 1.0);

        assert_eq!(
            BluffActions::from(a.unwrap()),
            BluffActions::Bid(3, Dice::Three)
        );
    }

    #[test]
    fn test_open_hand_solver_kuhn() {
        let mut evaluator = OpenHandSolver::new(100, SeedableRng::seed_from_u64(109));
        let gs = KuhnPoker::from_actions(&[KPAction::Jack, KPAction::Queen]);
        assert_eq!(evaluator.evaluate(&gs), vec![-1.0, 1.0]);

        let mut evaluator = OpenHandSolver::new(100, SeedableRng::seed_from_u64(109));
        let gs = KuhnPoker::from_actions(&[KPAction::Queen, KPAction::Jack]);
        assert_eq!(evaluator.evaluate(&gs), vec![0.0, 0.0]);

        let mut evaluator = OpenHandSolver::new(100, SeedableRng::seed_from_u64(109));
        let gs = KuhnPoker::from_actions(&[KPAction::King, KPAction::Jack]);
        assert_eq!(evaluator.evaluate(&gs), vec![1.0, -1.0]);
    }
}
