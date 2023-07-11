use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::Arc,
};

use dashmap::DashMap;

use crate::{
    actions,
    alloc::Pool,
    cfragent::cfrnode::ActionVec,
    game::{
        euchre::{
            processors::{euchre_early_terminate, process_euchre_actions},
            EuchreGameState,
        },
        Action, GameState, Player,
    },
};

use super::{alphamu::Team, ismcts::Evaluator};

pub const DEFAULT_MAX_TT_DEPTH: u8 = 255;

#[derive(Copy, Clone)]
pub struct Optimizations<G> {
    pub use_transposition_table: bool,
    pub isometric_transposition: bool,
    pub max_depth_for_tt: u8,
    /// Function that can filter or re-order moves for evaluation.
    ///
    /// For example, it could filter down to a single move, or it could
    /// remove all but 1 move.
    pub action_processor: fn(gs: &G, actions: &mut Vec<Action>),
    /// Determines if a game is already decided and can be finished by randomly
    /// playing move with no impact to the outcome
    pub can_early_terminate: fn(gs: &G) -> bool,
}

impl<G> Default for Optimizations<G> {
    fn default() -> Self {
        Self {
            use_transposition_table: true,
            isometric_transposition: true,
            max_depth_for_tt: DEFAULT_MAX_TT_DEPTH,
            action_processor: |_: &G, _: &mut Vec<Action>| {},
            can_early_terminate: |_: &G| false,
        }
    }
}

impl Optimizations<EuchreGameState> {
    pub fn new_euchre() -> Self {
        Optimizations {
            use_transposition_table: true,
            isometric_transposition: true,
            max_depth_for_tt: DEFAULT_MAX_TT_DEPTH,
            action_processor: process_euchre_actions,
            can_early_terminate: euchre_early_terminate,
        }
    }
}

/// Rollout solver that assumes perfect information by playing against open hands
///
/// This is an adaption of a double dummy solver for bridge
/// http://privat.bahnhof.se/wb758135/bridge/Alg-dds_x.pdf
#[derive(Clone)]
pub struct OpenHandSolver<G> {
    cache: AlphaBetaCache<G>,
    optimizations: Optimizations<G>,
}

impl<G: Clone> OpenHandSolver<G> {
    pub fn new(optimizations: Optimizations<G>) -> Self {
        Self {
            cache: AlphaBetaCache::new(optimizations.clone()),
            optimizations,
        }
    }

    pub fn new_without_cache() -> Self {
        let optimizations = Optimizations {
            use_transposition_table: false,
            ..Default::default()
        };

        Self {
            cache: AlphaBetaCache::new(optimizations.clone()),
            optimizations,
        }
    }

    pub fn reset(&mut self) {
        self.cache.reset();
    }
}

impl OpenHandSolver<EuchreGameState> {
    pub fn new_euchre() -> Self {
        let optimizations = Optimizations::new_euchre();

        Self {
            cache: AlphaBetaCache::new(optimizations.clone()),
            optimizations,
        }
    }
}

impl<G: Clone> Default for OpenHandSolver<G> {
    fn default() -> Self {
        Self::new(Optimizations::default())
    }
}

impl<G: GameState> Evaluator<G> for OpenHandSolver<G> {
    /// Evaluates the gamestate for a maximizing player using alpha-beta search
    fn evaluate_player(&mut self, gs: &G, maximizing_player: Player) -> f64 {
        mtd_search(
            gs.clone(),
            maximizing_player,
            0,
            self.cache.clone(),
            &self.optimizations,
        )
        .0
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

/// Returns the value of a given state and optionally the best move
///
/// http://people.csail.mit.edu/plaat/mtdf.html#abmem
fn mtd_search<G: GameState>(
    mut root: G,
    maximizing_player: Player,
    first_guess: i8,
    mut cache: AlphaBetaCache<G>,
    optimizations: &Optimizations<G>,
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
            0,
            &mut cache,
            optimizations,
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
struct AlphaBetaCache<G> {
    vec_pool: Pool<Vec<Action>>,
    transposition_table: Arc<DashMap<TranspositionKey, AlphaBetaResult>>,
    optimizations: Optimizations<G>,
}

impl<G> AlphaBetaCache<G> {
    fn new(optimizations: Optimizations<G>) -> Self {
        Self {
            vec_pool: Pool::new(|| Vec::with_capacity(5)),
            transposition_table: Arc::new(DashMap::new()),
            optimizations,
        }
    }
}

impl<G: GameState> AlphaBetaCache<G> {
    pub fn get(&self, gs: &G, maximizing_team: Team) -> Option<AlphaBetaResult> {
        if !self.optimizations.use_transposition_table {
            return None;
        }

        let k = self.get_game_key(gs);
        if let Some(k) = k {
            self.transposition_table
                .get(&(maximizing_team, k))
                .as_deref()
                .copied()
        } else {
            None
        }
    }

    pub fn insert(&self, gs: &G, v: AlphaBetaResult, maximizing_team: Team, depth: u8) {
        if !self.optimizations.use_transposition_table
            || depth > self.optimizations.max_depth_for_tt
        {
            return;
        }

        // Check if the game wants to store this state
        let k = self.get_game_key(gs);
        if let Some(k) = k {
            self.transposition_table.insert((maximizing_team, k), v);
        }
    }

    fn get_game_key(&self, gs: &G) -> Option<u64> {
        let k = gs.transposition_table_hash();

        // Continue to use the game specific logic of when to put things in the table
        if k.is_none() {
            return k;
        }

        match self.optimizations.isometric_transposition {
            true => k,
            false => {
                let mut hasher = DefaultHasher::default();
                gs.istate_key(gs.cur_player()).hash(&mut hasher);
                Some(hasher.finish())
            }
        }
    }
}

impl<G> AlphaBetaCache<G> {
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
    depth: u8,
    cache: &mut AlphaBetaCache<G>,
    optimizations: &Optimizations<G>,
) -> (f64, Option<Action>) {
    if gs.is_terminal() {
        let v = gs.evaluate(maximizing_team as usize);
        return (v, None);
    }

    // if the game is decided, just play the first action until the game
    // is actually terminal, get the value, get the score, and then undo the actions
    if (optimizations.can_early_terminate)(gs) {
        let mut actions = cache.vec_pool.detach();

        let mut actions_applied = 0;

        while !gs.is_terminal() {
            gs.legal_actions(&mut actions);
            gs.apply_action(actions[0]);
            actions_applied += 1;
        }

        let v = gs.evaluate(maximizing_team as usize);

        for _ in 0..actions_applied {
            gs.undo();
        }

        actions.clear();
        cache.vec_pool.attach(actions);
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
    (optimizations.action_processor)(gs, &mut actions);
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
            let (child_value, _) = alpha_beta(
                gs,
                maximizing_team,
                alpha,
                beta,
                depth + 1,
                cache,
                optimizations,
            );
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
            let (child_value, _) = alpha_beta(
                gs,
                maximizing_team,
                alpha,
                beta,
                depth + 1,
                cache,
                optimizations,
            );
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

    // transposition table scoring agains the original alpha and beta
    if result.0 <= alpha_orig {
        cache_value.upper_bound = result.0;
    } else if result.0 > alpha_orig && result.0 < beta_orig {
        cache_value.upper_bound = result.0;
        cache_value.lower_bound = result.0;
    } else if result.0 >= beta_orig {
        cache_value.lower_bound = result.0;
    }

    cache.insert(gs, cache_value, maximizing_team, depth);
    actions.clear();
    cache.vec_pool.attach(actions);
    result
}

#[cfg(test)]
mod tests {

    use crate::{
        algorithms::{
            ismcts::Evaluator,
            open_hand_solver::{mtd_search, AlphaBetaCache, Optimizations},
        },
        game::{
            bluff::{Bluff, BluffActions, Dice},
            kuhn_poker::{KPAction, KuhnPoker},
            GameState,
        },
    };

    use super::OpenHandSolver;

    #[test]
    fn test_mtd_kuhn_poker() {
        let gs = KuhnPoker::from_actions(&[KPAction::Jack, KPAction::Queen]);
        let (v, a) = mtd_search(
            gs,
            0,
            0,
            AlphaBetaCache::new(Optimizations::default()),
            &Optimizations::default(),
        );
        assert_eq!(v, -1.0);
        assert_eq!(a.unwrap(), KPAction::Pass.into());

        let gs = KuhnPoker::from_actions(&[KPAction::King, KPAction::Queen]);
        let (v, a) = mtd_search(
            gs,
            0,
            0,
            AlphaBetaCache::new(Optimizations::default()),
            &Optimizations::default(),
        );
        assert_eq!(v, 1.0);
        assert_eq!(a.unwrap(), KPAction::Bet.into());

        let gs = KuhnPoker::from_actions(&[
            KPAction::King,
            KPAction::Queen,
            KPAction::Pass,
            KPAction::Bet,
        ]);
        let (v, a) = mtd_search(
            gs,
            0,
            0,
            AlphaBetaCache::new(Optimizations::default()),
            &Optimizations::default(),
        );
        assert_eq!(v, 2.0);
        assert_eq!(a.unwrap(), KPAction::Bet.into());
    }

    #[test]
    fn test_mtd_bluff_2_2() {
        let mut gs = Bluff::new_state(2, 2);
        gs.apply_action(BluffActions::Roll(Dice::Two).into());
        gs.apply_action(BluffActions::Roll(Dice::Three).into());
        gs.apply_action(BluffActions::Roll(Dice::Two).into());
        gs.apply_action(BluffActions::Roll(Dice::Three).into());

        let (v, a) = mtd_search(
            gs,
            0,
            0,
            AlphaBetaCache::new(Optimizations::default()),
            &Optimizations::default(),
        );
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

        let (v, a) = mtd_search(
            gs,
            0,
            0,
            AlphaBetaCache::new(Optimizations::default()),
            &Optimizations::default(),
        );
        assert_eq!(v, 1.0);

        assert_eq!(
            BluffActions::from(a.unwrap()),
            BluffActions::Bid(3, Dice::Three)
        );
    }

    #[test]
    fn open_hand_solver_deterministic() {
        let mut gs = Bluff::new_state(1, 1);
        gs.apply_action(BluffActions::Roll(Dice::Wild).into());
        gs.apply_action(BluffActions::Roll(Dice::One).into());
        gs.apply_action(BluffActions::Bid(1, Dice::Five).into());

        let mut cache = OpenHandSolver::default();
        let first = cache.evaluate_player(&gs, 0);

        for _ in 0..1000 {
            assert_eq!(cache.evaluate_player(&gs, 0), first);
        }
    }
}
