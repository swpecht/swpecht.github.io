use std::{
    hash::{DefaultHasher, Hash, Hasher},
    sync::Arc,
};

use dashmap::DashMap;

use crate::gamestate::{Action, SimState, Team};

const MAX_DEPTH: u8 = 50;

pub fn find_best_move(root: SimState) -> Option<Action> {
    let cur_team = root.cur_team();
    mtd_search(root, cur_team, 0, AlphaBetaCache::new()).1
}

/// Returns the value of a given state and optionally the best move
///
/// http://people.csail.mit.edu/plaat/mtdf.html#abmem
fn mtd_search(
    mut root: SimState,
    maximizing_player: Team,
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
            maximizing_player,
            (beta - 1) as f64,
            beta as f64,
            0,
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
}

impl AlphaBetaCache {
    fn new() -> Self {
        Self {
            vec_pool: Pool::new(|| Vec::with_capacity(5)),
            transposition_table: Arc::new(DashMap::new()),
        }
    }
}

impl AlphaBetaCache {
    pub fn get(&self, gs: &SimState, maximizing_team: Team) -> Option<AlphaBetaResult> {
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

    pub fn insert(&self, gs: &SimState, v: AlphaBetaResult, maximizing_team: Team, depth: u8) {
        // Check if the game wants to store this state
        let k = self.get_game_key(gs);
        if let Some(k) = k {
            self.transposition_table.insert((maximizing_team, k), v);
        }
    }

    fn get_game_key(&self, gs: &SimState) -> Option<u64> {
        let mut hasher = DefaultHasher::default();
        gs.hash(&mut hasher);
        Some(hasher.finish())
    }
}

impl AlphaBetaCache {
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
fn alpha_beta(
    gs: &mut SimState,
    maximizing_team: Team,
    mut alpha: f64,
    mut beta: f64,
    depth: u8,
    cache: &mut AlphaBetaCache,
) -> (f64, Option<Action>) {
    if gs.is_terminal() || depth > MAX_DEPTH {
        let v = gs.evaluate(maximizing_team) as f64;
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

    let mut best_action = None;
    let team = gs.cur_team();
    let result;

    if team == maximizing_team {
        let mut value = f64::NEG_INFINITY;
        for a in &actions {
            // gs.apply(*a);
            let mut ngs = gs.clone();
            ngs.apply(*a);
            let (child_value, _) =
                alpha_beta(&mut ngs, maximizing_team, alpha, beta, depth + 1, cache);
            // gs.undo();
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
            // gs.apply(*a);
            let mut ngs = gs.clone();
            ngs.apply(*a);
            let (child_value, _) =
                alpha_beta(&mut ngs, maximizing_team, alpha, beta, depth + 1, cache);
            // gs.undo();
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

#[derive(Clone)]
pub struct Pool<T> {
    pool: Vec<T>,
    generator: fn() -> T,
}

impl<T> Pool<T> {
    pub fn new(generator: fn() -> T) -> Self {
        Self {
            pool: Vec::new(),
            generator,
        }
    }

    pub fn detach(&mut self) -> T {
        if self.pool.is_empty() {
            return (self.generator)();
        }

        self.pool.pop().unwrap()
    }

    pub fn attach(&mut self, obj: T) {
        self.pool.push(obj);
    }
}
