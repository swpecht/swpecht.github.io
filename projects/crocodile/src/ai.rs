use std::{
    cmp::Ordering,
    hash::{DefaultHasher, Hash, Hasher},
    sync::Arc,
};

use dashmap::DashMap;
use itertools::Itertools;

use crate::gamestate::{Action, SimState, Team};

const MAX_DEPTH: u8 = 5;

pub fn find_best_move(root: SimState) -> Option<Action> {
    // todo: switch to iterative deepending: https://www.chessprogramming.org/MTD(f)
    let cur_team = root.cur_team();

    let mut first_guess = 0;
    let mut action = None;
    let mut pv_cache = None;

    for d in 1..MAX_DEPTH {
        let mut cache = AlphaBetaCache::new();
        if let Some(c) = pv_cache {
            cache.pv_moves = c;
        }

        (first_guess, action) = mtd_search(root.clone(), cur_team, first_guess, d, &mut cache);
        pv_cache = Some(cache.pv_moves); // save the pv moves for the next run
    }

    action
}

/// Returns the value of a given state and optionally the best move
///
/// http://people.csail.mit.edu/plaat/mtdf.html#abmem
fn mtd_search(
    mut root: SimState,
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
            &mut root,
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
}
type TranspositionKey = (Team, u64);
/// Helper struct to speeding up alpha_beta search
#[derive(Clone)]
struct AlphaBetaCache {
    vec_pool: Pool<Vec<Action>>,
    sim_pool: Pool<SimState>,
    transposition_table: Arc<DashMap<TranspositionKey, AlphaBetaResult>>,
    pv_moves: [Option<Action>; MAX_DEPTH as usize],
}

impl AlphaBetaCache {
    fn new() -> Self {
        Self {
            vec_pool: Pool::new(|| Vec::with_capacity(8)),
            sim_pool: Pool::new(SimState::default),
            transposition_table: Arc::new(DashMap::new()),
            pv_moves: [None; MAX_DEPTH as usize],
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

    pub fn insert(&self, gs: &SimState, v: AlphaBetaResult, maximizing_team: Team, _depth: u8) {
        // Check if the game wants to store this state
        let k = self.get_game_key(gs);
        if let Some(k) = k {
            self.transposition_table.insert((maximizing_team, k), v);
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
    if let Some(v) = cache.get(gs, maximizing_team) {
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

    let mut children = child_nodes(gs, maximizing_team, cache, depth);

    if team == maximizing_team {
        let mut value = f64::NEG_INFINITY;
        for (ngs, a) in children.iter_mut() {
            let new_depth = if *a == Action::EndTurn {
                depth + 1
            } else {
                depth
            };
            let (child_value, _) = alpha_beta(
                ngs,
                maximizing_team,
                alpha,
                beta,
                new_depth,
                max_depth,
                cache,
            );
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

        for (ngs, a) in children.iter_mut() {
            let new_depth = if *a == Action::EndTurn {
                depth + 1
            } else {
                depth
            };
            let (child_value, _) = alpha_beta(
                ngs,
                maximizing_team,
                alpha,
                beta,
                new_depth,
                max_depth,
                cache,
            );
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

    children
        .into_iter()
        .for_each(|(s, _)| cache.sim_pool.attach(s));
    cache.insert(gs, cache_value, maximizing_team, depth);
    cache.pv_moves[depth as usize] = result.1;

    result
}

/// Return all chilren nodes, sorted by value
fn child_nodes(
    gs: &SimState,
    maximizing_team: Team,
    cache: &mut AlphaBetaCache,
    depth: u8,
) -> Vec<(SimState, Action)> {
    let mut actions = cache.vec_pool.detach();
    gs.legal_actions(&mut actions);

    let mut result = actions
        .iter()
        .map(|a| {
            let mut ngs = cache.sim_pool.detach();
            ngs.clone_from(gs);
            ngs.apply(*a);
            (ngs, *a)
        })
        .collect_vec();

    // use the pv moves from last time if available
    let pv_action = cache.pv_moves[depth as usize];

    // TODO: this will try the pv move first at this depth even if we're not on the PV chain
    // (e.g. earlier moves were different). TBD if this is desired
    result.sort_by(|(s1, a1), (s2, a2)| {
        if Some(a1) == pv_action.as_ref() {
            return Ordering::Less;
        } else if Some(a2) == pv_action.as_ref() {
            return Ordering::Greater;
        }

        // we're doing a reverse sort so s2.cmp(s1)
        s2.evaluate(maximizing_team)
            .partial_cmp(&s1.evaluate(maximizing_team))
            .unwrap()
    });

    // ensure we're trying the pv move first
    if let Some(pva) = pv_action
        && actions.contains(&pva)
    {
        assert_eq!(result[0].1, pva);
    }

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

#[cfg(test)]
mod tests {

    extern crate test;

    use test::Bencher;

    use crate::gamestate::{sc, Ability, Character, SimState};

    use super::find_best_move;

    #[bench]
    fn find_best_move_bench(b: &mut Bencher) {
        b.iter(|| {
            let mut state = SimState::new();

            state.insert_entity(Character::Orc, vec![Ability::MeleeAttack], sc(5, 10));
            // state.insert_entity(Character::Orc, vec![Ability::MeleeAttack], sc(6, 10));
            state.insert_entity(Character::Orc, vec![Ability::MeleeAttack], sc(4, 10));

            state.insert_entity(
                Character::Knight,
                vec![Ability::MeleeAttack, Ability::BowAttack { range: 20 }],
                sc(0, 9),
            );

            find_best_move(state);
        });
    }
}
