use std::{marker::PhantomData, sync::Arc};

use dashmap::DashMap;
use rand::rngs::StdRng;
use rayon::prelude::*;

use crate::{
    actions,
    alloc::Pool,
    cfragent::cfrnode::ActionVec,
    game::{Action, GameState, Player},
    istate::IsomorphicHash,
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
pub struct OpenHandSolver<G> {
    n_rollouts: usize,
    rng: StdRng,
    cache: AlphaBetaCache,
    _phantom: PhantomData<G>,
}

impl<G: GameState + ResampleFromInfoState + Send> OpenHandSolver<G> {
    pub fn new(n_rollouts: usize, rng: StdRng) -> Self {
        Self {
            n_rollouts,
            rng,
            cache: AlphaBetaCache::default(),
            _phantom: PhantomData::default(),
        }
    }

    pub fn new_without_cache(n_rollouts: usize, rng: StdRng) -> Self {
        Self {
            n_rollouts,
            rng,
            cache: AlphaBetaCache::new(false),
            _phantom: PhantomData::default(),
        }
    }

    pub fn set_rollout(&mut self, n_rollouts: usize) {
        // clears all cached data for different world counts
        self.reset();
        self.n_rollouts = n_rollouts;
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
            .map(|w| alpha_beta_search_cached(w, maximizing_player, self.cache.clone()).0)
            .sum();

        sum / self.n_rollouts as f64
    }

    fn get_worlds(&mut self, gs: &G) -> Vec<G> {
        // since we're generating new worlds, we reset the cache
        // self.reset();

        let mut worlds = Vec::with_capacity(self.n_rollouts);
        for _ in 0..self.n_rollouts {
            worlds.push(gs.resample_from_istate(gs.cur_player(), &mut self.rng));
        }
        worlds
    }

    pub fn reset(&mut self) {
        self.cache.transposition_table.clear();
    }
}

impl<G: GameState + ResampleFromInfoState + Send> Evaluator<G> for OpenHandSolver<G> {
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

    fn prior(&mut self, _gs: &G) -> ActionVec<f64> {
        todo!()
    }
}

impl<G: GameState + ResampleFromInfoState + Send> Policy<G> for OpenHandSolver<G> {
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

fn alpha_beta_search_cached<G: GameState>(
    mut gs: G,
    maximizing_player: Player,
    mut cache: AlphaBetaCache,
) -> (f64, Option<Action>) {
    let maximizing_team = Team::from(maximizing_player);
    alpha_beta(
        &mut gs,
        maximizing_team,
        f64::NEG_INFINITY,
        f64::INFINITY,
        &mut cache,
    )
}

/// Helper struct to speeding up alpha_beta search
#[derive(Clone)]
struct AlphaBetaCache {
    vec_pool: Pool<Vec<Action>>,
    transposition_table: Arc<DashMap<(Team, IsomorphicHash), (f64, Option<Action>)>>,
    use_tt: bool,
}

impl AlphaBetaCache {
    fn new(use_tt: bool) -> Self {
        Self {
            vec_pool: Pool::new(|| Vec::with_capacity(5)),
            transposition_table: Arc::new(DashMap::new()),
            use_tt,
        }
    }
}

impl Default for AlphaBetaCache {
    fn default() -> Self {
        Self::new(true)
    }
}

impl AlphaBetaCache {
    pub fn get<G: GameState>(
        &self,
        gs: &G,
        maximizing_team: Team,
    ) -> Option<(f64, Option<Action>)> {
        if !self.use_tt {
            return None;
        }

        let k = gs.transposition_table_hash();
        if let Some(k) = k {
            self.transposition_table
                .get(&(maximizing_team, k))
                .as_deref()
                .copied()
        } else {
            None
        }
    }

    pub fn insert<G: GameState>(&self, gs: &G, v: (f64, Option<Action>), maximizing_team: Team) {
        if !self.use_tt {
            return;
        }

        // Check if the game wants to store this state
        let k = gs.transposition_table_hash();
        if let Some(k) = k {
            self.transposition_table.insert((maximizing_team, k), v);
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

    if let Some(v) = cache.get(gs, maximizing_team) {
        return v;
    }

    let mut actions = cache.vec_pool.detach();
    gs.legal_actions(&mut actions);
    if gs.is_chance_node() {
        todo!("add support for chance nodes")
    }

    let player = gs.cur_player();
    let mut best_action = None;
    let team: Team = player.into();
    let result;

    if team == maximizing_team {
        let mut value = f64::NEG_INFINITY;
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
        result = (value, best_action);
    } else {
        let mut value = f64::INFINITY;
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
        result = (value, best_action);
    }

    cache.insert(gs, result, maximizing_team);
    actions.clear();
    cache.vec_pool.attach(actions);
    result
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
