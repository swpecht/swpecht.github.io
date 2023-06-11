use std::sync::Arc;

use dashmap::DashMap;

use crate::{
    actions,
    alloc::Pool,
    cfragent::cfrnode::ActionVec,
    game::{Action, GameState, Player},
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

    pub fn reset(&mut self) {
        self.cache.reset();
    }
}

impl Default for OpenHandSolver {
    fn default() -> Self {
        Self::new()
    }
}

impl<G: GameState> Evaluator<G> for OpenHandSolver {
    /// Evaluates the gamestate for a maximizing player using alpha-beta search
    fn evaluate_player(&mut self, gs: &G, maximizing_player: Player) -> f64 {
        // We reset the cache to avoid search instability
        self.reset();
        mtd_search(gs.clone(), maximizing_player, 0, self.cache.clone()).0
        // alpha_beta_search_cached(gs.clone(), maximizing_player, self.cache.clone()).0
    }

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

/// Returns the value of a given state and optionally the best move
///
/// http://people.csail.mit.edu/plaat/mtdf.html#abmem
fn mtd_search<G: GameState>(
    mut root: G,
    maximizing_player: Player,
    first_guess: i8,
    mut cache: AlphaBetaCache,
) -> (f64, Option<Action>) {
    let mut g = first_guess;
    let mut best_action;
    let mut upperbound = i8::MAX;
    let mut lowerbound = i8::MIN;

    loop {
        let beta = if g == lowerbound { g + 1 } else { g };
        let result = alpha_beta(
            &mut root,
            Team::from(maximizing_player),
            (beta - 1) as f64,
            beta as f64,
            &mut cache,
        );
        g = result.0 as i8;
        best_action = result.1;
        if g < beta {
            upperbound = g;
        } else {
            lowerbound = g;
        }

        if lowerbound >= upperbound {
            break;
        }
    }

    (g as f64, best_action)
}

#[derive(Clone, Copy)]
struct AlphaBetaResult {
    lower_bound: f64,
    upper_bound: f64,
    action: Option<Action>,
}
type TranspositionKey = (Team, u64);
/// Helper struct to speeding up alpha_beta search
#[derive(Clone)]
struct AlphaBetaCache {
    vec_pool: Pool<Vec<Action>>,
    transposition_table: Arc<DashMap<TranspositionKey, AlphaBetaResult>>,
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
    pub fn get<G: GameState>(&self, gs: &G, maximizing_team: Team) -> Option<AlphaBetaResult> {
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

    pub fn insert<G: GameState>(&self, gs: &G, v: AlphaBetaResult, maximizing_team: Team) {
        if !self.use_tt {
            return;
        }

        // Check if the game wants to store this state
        let k = gs.transposition_table_hash();
        if let Some(k) = k {
            self.transposition_table.insert((maximizing_team, k), v);
        }
    }

    pub fn reset(&mut self) {
        self.transposition_table.clear();
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

    let alpha_orig = alpha;
    let beta_orig = beta;
    // We can only return the value if we have the right bound
    // http://people.csail.mit.edu/plaat/mtdf.html#abmem
    if let Some(v) = cache.get(gs, maximizing_team) {
        if v.lower_bound >= beta {
            return (v.lower_bound, v.action);
        } else if v.upper_bound <= alpha {
            return (v.upper_bound, v.action);
        }
        alpha = alpha.max(v.lower_bound);
        beta = beta.min(v.upper_bound);
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
            if value >= beta {
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
            if value <= alpha {
                break;
            }
        }
        result = (value, best_action);
    }

    // Store the bounds in the transposition table
    // http://people.csail.mit.edu/plaat/mtdf.html#abmem
    let mut cache_value = AlphaBetaResult {
        lower_bound: f64::NEG_INFINITY,
        upper_bound: f64::INFINITY,
        action: result.1,
    };

    // maybe should compare to alpha orig
    // https://en.wikipedia.org/wiki/Negamax#cite_note-Breuker-1

    if result.0 <= alpha_orig {
        cache_value.upper_bound = result.0;
    } else if result.0 > alpha_orig && result.0 < beta_orig {
        cache_value.upper_bound = result.0;
        cache_value.lower_bound = result.0;
    } else if result.0 >= beta_orig {
        cache_value.lower_bound = result.0;
    }

    cache.insert(gs, cache_value, maximizing_team);
    actions.clear();
    cache.vec_pool.attach(actions);
    result
}

#[cfg(test)]
mod tests {

    use crate::{
        algorithms::ismcts::Evaluator,
        game::{
            bluff::{Bluff, BluffActions, Dice},
            euchre::EuchreGameState,
            kuhn_poker::{KPAction, KuhnPoker},
            GameState,
        },
    };

    use super::{alpha_beta_search, OpenHandSolver};

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
    fn spot_test_euchre_solutions() {
        let games: Vec<&str> = vec![
            "TsJhAhJdQd|KcJsQsKsAd|JcQcAcKhTd|9cTc9sTh9d|Kd|",
            "TsJhAhJdQd|KcJsQsKsAd|JcQcAcKhTd|9cTc9sTh9d|Kd|P",
            "TsJhAhJdQd|KcJsQsKsAd|JcQcAcKhTd|9cTc9sTh9d|Kd|PP",
            "TsJhAhJdQd|KcJsQsKsAd|JcQcAcKhTd|9cTc9sTh9d|Kd|PPT|",
        ];
        let mut cache = OpenHandSolver::new();

        for s in games {
            let gs = EuchreGameState::from(s);
            println!("Evaluated {}: {}", gs, cache.evaluate_player(&gs, 0));
        }

        let gs = EuchreGameState::from("TsJhAhJdQd|KcJsQsKsAd|JcQcAcKhTd|9cTc9sTh9d|Kd|PPT|Th|");
        let key = gs.key();
        let child_hash = gs.transposition_table_hash();

        let cached = cache.evaluate_player(&gs, 0);
        // cache.reset();
        // let no_cached = cache.evaluate_player(&gs, 0);
        // assert_eq!(no_cached, 2.0);
        assert_eq!(cached, 2.0);
    }

    #[test]
    fn open_hand_solver_deterministic() {
        let mut gs = Bluff::new_state(1, 1);
        gs.apply_action(BluffActions::Roll(Dice::Wild).into());
        gs.apply_action(BluffActions::Roll(Dice::One).into());
        gs.apply_action(BluffActions::Bid(1, Dice::Five).into());

        let mut cache = OpenHandSolver::new();
        let first = cache.evaluate_player(&gs, 0);

        for _ in 0..1000 {
            assert_eq!(cache.evaluate_player(&gs, 0), first);
        }
    }
}
