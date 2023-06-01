use std::sync::Arc;

use dashmap::DashMap;

use crate::{
    actions,
    alloc::Pool,
    cfragent::cfrnode::ActionVec,
    game::{Action, GameState, Player},
    istate::IsomorphicHash,
};

use super::{alphamu::Team, ismcts::Evaluator};

/// Rollout solver that assumes perfect information by playing against open hands
///
/// This is an adaption of a double dummy solver for bridge
/// http://privat.bahnhof.se/wb758135/bridge/Alg-dds_x.pdf
#[derive(Clone)]
pub struct OpenHandSolver {
    cache: AlphaBetaCache,
}

impl OpenHandSolver {
    pub fn new() -> Self {
        Self {
            cache: AlphaBetaCache::default(),
        }
    }

    pub fn new_without_cache() -> Self {
        Self {
            cache: AlphaBetaCache::new(false),
        }
    }

    /// Evaluates the gamestate for a maximizing player using alpha-beta search
    pub fn evaluate_player<G: GameState>(&mut self, gs: &G, maximizing_player: Player) -> f64 {
        alpha_beta_search_cached(gs.clone(), maximizing_player, self.cache.clone()).0
    }
}

impl Default for OpenHandSolver {
    fn default() -> Self {
        Self::new()
    }
}

impl<G: GameState> Evaluator<G> for OpenHandSolver {
    fn evaluate(&mut self, gs: &G) -> Vec<f64> {
        let mut result = vec![0.0; gs.num_players()];

        for (p, r) in result.iter_mut().enumerate().take(2) {
            *r = self.evaluate_player(gs, p);
        }

        // Only support evaluating for 2 teams, so we can copy over the results
        for i in 2..result.len() {
            result[i] = result[i % 2];
        }

        result
    }

    fn prior(&mut self, gs: &G) -> ActionVec<f64> {
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

    use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

    use crate::{
        algorithms::{ismcts::Evaluator, open_hand_solver::OpenHandSolver},
        game::{
            bluff::{Bluff, BluffActions, Dice},
            euchre::Euchre,
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
    fn test_alg_open_hand_solver_euchre() {
        let mut rng: StdRng = SeedableRng::seed_from_u64(51);
        let mut actions = Vec::new();

        let mut cached = OpenHandSolver::new();
        let mut no_cache = OpenHandSolver::new_without_cache();

        for _ in 0..10 {
            let mut gs = Euchre::new_state();
            while gs.is_chance_node() {
                gs.legal_actions(&mut actions);
                let a = actions.choose(&mut rng).unwrap();
                gs.apply_action(*a);
            }

            println!("{}", gs);
            let c = cached.evaluate(&gs);
            let no_c = no_cache.evaluate(&gs);
            assert_eq!(c[0], no_c[0]);
            assert_eq!(c[1], no_c[1]);
        }
    }
}
