use std::{
    cell::RefCell,
    hash::{DefaultHasher, Hash, Hasher},
    rc::Rc,
    sync::Arc,
};

use dashmap::DashMap;
use simulation::gamestate::{Action, SimState, Team};

const MAX_DEPTH: u8 = 6;

pub fn find_best_move(root: SimState) -> Option<Action> {
    // todo: switch to iterative deepending: https://www.chessprogramming.org/MTD(f)
    let cur_team = root.cur_team();

    let mut first_guess = 0;
    let mut action = None;

    let mut cache = AlphaBetaCache::new();

    for d in 1..MAX_DEPTH {
        (first_guess, action) = mtd_search(&mut root.clone(), cur_team, first_guess, d, &mut cache);
    }

    action
}

/// Returns the value of a given state and optionally the best move
///
/// http://people.csail.mit.edu/plaat/mtdf.html#abmem
fn mtd_search(
    root: &mut SimState,
    maximizing_player: Team,
    first_guess: i8,
    max_depth: u8,
    cache: &mut AlphaBetaCache,
) -> (i8, Option<Action>) {
    let mut g = first_guess;
    let mut best_action;
    let mut upperbound = i8::MAX;
    let mut lowerbound = i8::MIN;

    loop {
        let beta = if g == lowerbound { g + 1 } else { g };
        let result = alpha_beta(
            root,
            maximizing_player,
            (beta - 1) as f64,
            beta as f64,
            0,
            max_depth,
            cache,
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

    (g, best_action)
}

#[derive(Clone, Copy)]
struct AlphaBetaResult {
    lower_bound: f64,
    upper_bound: f64,
    action: Option<Action>,
    remaining_depth: u8,
}
type TranspositionKey = (Team, u64);
/// Helper struct to speeding up alpha_beta search
#[derive(Clone)]
struct AlphaBetaCache {
    action_vec: Vec<Action>,
    child_nodes: Vec<Rc<RefCell<Vec<(i32, Action)>>>>,
    transposition_table: Arc<DashMap<TranspositionKey, AlphaBetaResult>>,
    pv_moves: [Option<Action>; MAX_DEPTH as usize],
}

impl AlphaBetaCache {
    fn new() -> Self {
        Self {
            action_vec: Vec::with_capacity(32),
            transposition_table: Arc::new(DashMap::new()),
            pv_moves: [None; MAX_DEPTH as usize],
            child_nodes: {
                let mut v = Vec::with_capacity(MAX_DEPTH as usize);
                (0..MAX_DEPTH as usize)
                    .for_each(|_| v.push(Rc::new(RefCell::new(Vec::with_capacity(32)))));
                v
            },
        }
    }
}

impl AlphaBetaCache {
    pub fn get(
        &self,
        gs: &SimState,
        maximizing_team: Team,
        reamining_depth: u8,
    ) -> Option<AlphaBetaResult> {
        let k = self.get_game_key(gs);
        if let Some(k) = k {
            let r = self
                .transposition_table
                .get(&(maximizing_team, k))
                .as_deref()
                .copied()?;
            if r.remaining_depth >= reamining_depth {
                Some(r)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn insert(&self, gs: &SimState, v: AlphaBetaResult, maximizing_team: Team) {
        // Check if the game wants to store this state
        let k = self.get_game_key(gs);
        if let Some(k) = k {
            // only insert the value if the remaining depth of the new value is greater
            // than the one currently stored. This means the result is more accurate as it's from a
            // deeper search
            if let Some(cur_val) = self
                .transposition_table
                .get(&(maximizing_team, k))
                .as_deref()
                && cur_val.remaining_depth < v.remaining_depth
            {
                self.transposition_table.insert((maximizing_team, k), v);
            }
        }
    }

    fn get_game_key(&self, gs: &SimState) -> Option<u64> {
        if !gs.is_start_of_turn() {
            return None;
        }

        let mut hasher = DefaultHasher::default();
        gs.hash(&mut hasher);
        Some(hasher.finish())
    }
}

impl AlphaBetaCache {
    pub fn _reset(&mut self) {
        self.transposition_table.clear();
        self.pv_moves = [None; MAX_DEPTH as usize];
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
    max_depth: u8,
    cache: &mut AlphaBetaCache,
) -> (f64, Option<Action>) {
    if gs.is_terminal() || depth >= max_depth {
        let v = gs.evaluate(maximizing_team) as f64;
        return (v, None);
    }

    let alpha_orig = alpha;
    let beta_orig = beta;
    // We can only return the value if we have the right bound
    // http://people.csail.mit.edu/plaat/mtdf.html#abmem
    if let Some(v) = cache.get(gs, maximizing_team, MAX_DEPTH - depth) {
        if v.lower_bound >= beta {
            return (v.lower_bound, v.action);
        } else if v.upper_bound <= alpha {
            return (v.upper_bound, v.action);
        }
        alpha = alpha.max(v.lower_bound);
        beta = beta.min(v.upper_bound);
    }

    if gs.is_chance_node() {
        todo!("add support for chance nodes")
    }

    let mut best_action = None;
    let team = gs.cur_team();
    let result;

    let children = child_nodes(gs, maximizing_team, cache, depth);
    let mut children = children
        .try_borrow_mut()
        .unwrap_or_else(|_| panic!("failed to borrow children at depth: {}", depth));

    if team == maximizing_team {
        let mut value = f64::NEG_INFINITY;
        for (_, a) in children.iter() {
            gs.apply(*a);
            let (child_value, _) = alpha_beta(
                gs,
                maximizing_team,
                alpha,
                beta,
                depth + 1,
                max_depth,
                cache,
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
        children.reverse();

        for (_, a) in children.iter() {
            gs.apply(*a);
            let (child_value, _) = alpha_beta(
                gs,
                maximizing_team,
                alpha,
                beta,
                depth + 1,
                max_depth,
                cache,
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
        remaining_depth: MAX_DEPTH - depth,
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

    cache.insert(gs, cache_value, maximizing_team);
    children.clear();
    cache.pv_moves[depth as usize] = result.1;

    result
}

/// Return all chilren nodes, sorted by value
fn child_nodes(
    gs: &mut SimState,
    maximizing_team: Team,
    cache: &mut AlphaBetaCache,
    depth: u8,
) -> Rc<RefCell<Vec<(i32, Action)>>> {
    // We're re-using the child node arrays to avoid allocations
    // need the Rc and RefCell for interior mutability
    let mut result = cache.child_nodes[depth as usize]
        .try_borrow_mut()
        .unwrap_or_else(|_| panic!("failed to borrow children at depth: {}", depth));
    result.clear();
    gs.legal_actions(&mut cache.action_vec);

    cache.action_vec.iter().for_each(|a| {
        gs.apply(*a);
        let score = gs.evaluate(maximizing_team);
        gs.undo();
        result.push((score, *a));
    });

    // use the pv moves from last time if available
    let pv_action = cache.pv_moves[depth as usize];

    // This will try the pv move first at this depth even if we're not on the PV chain
    // (e.g. earlier moves were different). TBD if this is desired
    //
    // We use cached key because evaluate can be expensive to calculate
    result.sort_by_key(|(s, a)| {
        if Some(a) == pv_action.as_ref() {
            return i32::MAX;
        }
        *s
    });
    result.reverse();

    // ensure we're trying the pv move first
    if let Some(pva) = pv_action
        && cache.action_vec.contains(&pva)
    {
        assert_eq!(result[0].1, pva);
    }

    cache.action_vec.clear();
    Rc::clone(&cache.child_nodes[depth as usize])
}

#[cfg(test)]
mod tests {

    extern crate test;

    // use crate::gamestate::{sc, SimState, Team};

    // use super::find_best_move;

    // #[bench]
    // fn find_best_move_bench(b: &mut Bencher) {
    //     b.iter(|| {
    //         // let mut state = SimState::new();
    //         // // state.insert_prebuilt(PreBuiltCharacter::Skeleton, sc(4, 10), Team::NPCs);
    //         // // state.insert_prebuilt(PreBuiltCharacter::Skeleton, sc(5, 10), Team::NPCs);
    //         // // state.insert_prebuilt(PreBuiltCharacter::Knight, sc(0, 9), Team::Players);

    //         // find_best_move(state);
    //         todo!()
    //     });
    // }
}
