use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::Arc,
};

use dashmap::DashMap;
use rustc_hash::FxBuildHasher;
use games::{
    actions,
    gamestates::{
        euchre::{
            processors::{euchre_early_terminate, process_euchre_actions},
            EuchreGameState,
        },
        oh_hell::{
            processors::{oh_hell_early_terminate, process_oh_hell_actions},
            OhHellGameState,
        },
    },
    Action, GameState, Player, Team,
};
use log::trace;

use crate::{alloc::Pool, collections::actionvec::ActionVec};

use super::ismcts::Evaluator;

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

impl Optimizations<OhHellGameState> {
    pub fn new_oh_hell() -> Self {
        Optimizations {
            use_transposition_table: true,
            isometric_transposition: true,
            max_depth_for_tt: DEFAULT_MAX_TT_DEPTH,
            action_processor: process_oh_hell_actions,
            can_early_terminate: oh_hell_early_terminate,
        }
    }

    /// Just early termination + cheap TT hash (no action processing).
    /// Useful for measuring the marginal contribution of action filtering.
    pub fn new_oh_hell_minimal() -> Self {
        Optimizations {
            use_transposition_table: true,
            isometric_transposition: true,
            max_depth_for_tt: DEFAULT_MAX_TT_DEPTH,
            action_processor: |_, _| {},
            can_early_terminate: oh_hell_early_terminate,
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

impl OpenHandSolver<OhHellGameState> {
    pub fn new_oh_hell() -> Self {
        let optimizations = Optimizations::new_oh_hell();
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

impl<G: GameState> OpenHandSolver<G> {
    /// Like the trait `evaluate_player` but takes `&mut G` and relies on `apply_action` /
    /// `undo` inside `alpha_beta` to leave the state untouched on return. Avoids the per-
    /// rollout `gs.clone()` (which heap-allocates `play_order` for Euchre). CFRES holds a
    /// concrete `OpenHandSolver` and calls this directly via the inherent method.
    pub fn evaluate_player_mut(&mut self, gs: &mut G, maximizing_player: Player) -> f64 {
        mtd_search(
            gs,
            maximizing_player,
            0,
            self.cache.clone(),
            &self.optimizations,
        )
        .0
    }
}

impl<G: GameState> Evaluator<G> for OpenHandSolver<G> {
    /// Evaluates the gamestate for a maximizing player using alpha-beta search
    fn evaluate_player(&mut self, gs: &G, maximizing_player: Player) -> f64 {
        let mut owned = gs.clone();
        mtd_search(
            &mut owned,
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

    fn reset(&mut self) {
        self.cache.reset();
    }
}

/// Returns the value of a given state and optionally the best move
///
/// http://people.csail.mit.edu/plaat/mtdf.html#abmem
fn mtd_search<G: GameState>(
    root: &mut G,
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
            root,
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
/// Helper struct to speeding up alpha_beta search.
///
/// Uses `FxBuildHasher` (rustc-hash) instead of DashMap's default `RandomState` (SipHasher).
/// The cache key is already a `(Team, u64)` where the `u64` is itself the output of an
/// isomorphic canonicalization hash, so running it through a cryptographic-strength hasher
/// like SipHasher is pure overhead. FxHasher is a simple multiply-xor-shift that runs in a
/// handful of instructions on a single u64.
#[derive(Clone)]
struct AlphaBetaCache<G> {
    vec_pool: Pool<Vec<Action>>,
    transposition_table: Arc<DashMap<TranspositionKey, AlphaBetaResult, FxBuildHasher>>,
    optimizations: Optimizations<G>,
}

impl<G> AlphaBetaCache<G> {
    fn new(optimizations: Optimizations<G>) -> Self {
        Self {
            vec_pool: Pool::new(|| Vec::with_capacity(5)),
            transposition_table: Arc::new(DashMap::with_hasher(FxBuildHasher)),
            optimizations,
        }
    }
}

impl<G: GameState> AlphaBetaCache<G> {
    /// Look up a cached result by a precomputed game key. The caller computes the key
    /// once per alpha_beta frame via `get_game_key` and passes it to both `get_by_key`
    /// and `insert_by_key`, avoiding the iso-deck + hash cost on the insert side.
    pub fn get_by_key(&self, k: u64, maximizing_team: Team) -> Option<AlphaBetaResult> {
        self.transposition_table
            .get(&(maximizing_team, k))
            .as_deref()
            .copied()
    }

    /// Insert with a precomputed key. See `get_by_key`.
    pub fn insert_by_key(&self, k: u64, v: AlphaBetaResult, maximizing_team: Team) {
        self.transposition_table.insert((maximizing_team, k), v);
    }

    pub(crate) fn get_game_key(&self, gs: &G) -> Option<u64> {
        let k = gs.transposition_table_hash();

        // Continue to use the game specific logic of when to put things in the table
        if k.is_none() {
            return k;
        }

        match self.optimizations.isometric_transposition {
            true => k,
            false => {
                let mut hasher = DefaultHasher::default();
                gs.key().hash(&mut hasher);
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

        trace!("early termination found for: {}, evaluation: {}", gs, v);
        return (v, None);
    }

    let alpha_orig = alpha;
    let beta_orig = beta;
    // Compute the cache key once at function entry. The state is unchanged across the
    // get/insert pair (apply_action/undo pairs leave gs identical by the end of the
    // recursion), so we can reuse the key for both lookups instead of running
    // transposition_table_hash (and iso_deck) twice per frame.
    let cache_key = if cache.optimizations.use_transposition_table {
        cache.get_game_key(gs)
    } else {
        None
    };
    // We can only return the value if we have the right bound
    // http://people.csail.mit.edu/plaat/mtdf.html#abmem
    if let Some(k) = cache_key {
        if let Some(v) = cache.get_by_key(k, maximizing_team) {
            if v.lower_bound >= beta {
                return (v.lower_bound, v.action);
            } else if v.upper_bound <= alpha {
                return (v.upper_bound, v.action);
            }
            alpha = alpha.max(v.lower_bound);
            beta = beta.min(v.upper_bound);
        }
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

    if let Some(k) = cache_key {
        if depth <= cache.optimizations.max_depth_for_tt {
            cache.insert_by_key(k, cache_value, maximizing_team);
        }
    }
    actions.clear();
    cache.vec_pool.attach(actions);
    result
}

#[cfg(test)]
mod tests {

    use games::{
        actions,
        gamestates::{
            bluff::{Bluff, BluffActions, Dice},
            kuhn_poker::{KPAction, KuhnPoker},
            oh_hell::{processors::oh_hell_early_terminate, OhHell, NUM_PLAYERS},
        },
        GameState,
    };
    use rand::{rngs::StdRng, seq::IndexedRandom, SeedableRng};

    use crate::algorithms::{
        ismcts::Evaluator,
        open_hand_solver::{mtd_search, AlphaBetaCache, Optimizations},
    };

    use super::OpenHandSolver;

    #[test]
    fn test_mtd_kuhn_poker() {
        let mut gs = KuhnPoker::from_actions(&[KPAction::Jack, KPAction::Queen]);
        let (v, a) = mtd_search(
            &mut gs,
            0,
            0,
            AlphaBetaCache::new(Optimizations::default()),
            &Optimizations::default(),
        );
        assert_eq!(v, -1.0);
        assert_eq!(a.unwrap(), KPAction::Pass.into());

        let mut gs = KuhnPoker::from_actions(&[KPAction::King, KPAction::Queen]);
        let (v, a) = mtd_search(
            &mut gs,
            0,
            0,
            AlphaBetaCache::new(Optimizations::default()),
            &Optimizations::default(),
        );
        assert_eq!(v, 1.0);
        assert_eq!(a.unwrap(), KPAction::Bet.into());

        let mut gs = KuhnPoker::from_actions(&[
            KPAction::King,
            KPAction::Queen,
            KPAction::Pass,
            KPAction::Bet,
        ]);
        let (v, a) = mtd_search(
            &mut gs,
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
            &mut gs,
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
            &mut gs,
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

    // ============================================================
    // Oh Hell correctness: optimized solver must produce the same
    // values as the unoptimized one on every state.
    // ============================================================

    /// Drive `gs` randomly through the chance + bidding phases so we reach
    /// a fully-determined play-phase state suitable for the open-hand
    /// solver.
    fn drive_to_play(gs: &mut games::gamestates::oh_hell::OhHellGameState, rng: &mut StdRng) {
        use games::gamestates::oh_hell::OHPhase;
        while !gs.is_terminal() && gs.phase() != OHPhase::Play {
            let acts = actions!(gs);
            let a = *acts.choose(rng).unwrap();
            gs.apply_action(a);
        }
    }

    /// For many random Oh Hell play-phase states (at various n_tricks and
    /// various depths into the play), check that the OH-specific
    /// optimizations produce *exactly* the same evaluated value as the
    /// default optimizations.
    #[test]
    fn oh_hell_optimized_matches_unoptimized() {
        let mut rng: StdRng = SeedableRng::seed_from_u64(0xC0DE);
        for n_tricks in 1..=3 {
            for trial in 0..30 {
                let mut gs = OhHell::new_state(n_tricks);
                drive_to_play(&mut gs, &mut rng);

                // Sample a few depths into the play phase so we test both
                // start-of-trick and intermediate states.
                let max_depth = (3 * n_tricks).saturating_sub(1);
                let depth = if max_depth == 0 { 0 } else { trial % max_depth };
                for _ in 0..depth {
                    if gs.is_terminal() {
                        break;
                    }
                    let acts = actions!(gs);
                    let a = *acts.choose(&mut rng).unwrap();
                    gs.apply_action(a);
                }
                if gs.is_terminal() {
                    continue;
                }

                for p in 0..NUM_PLAYERS {
                    let mut baseline = OpenHandSolver::default();
                    let mut tuned = OpenHandSolver::new_oh_hell();
                    let v_default = baseline.evaluate_player(&gs, p);
                    let v_tuned = tuned.evaluate_player(&gs, p);
                    assert_eq!(
                        v_default, v_tuned,
                        "value mismatch for player {} on state {} (n_tricks={}, depth={})",
                        p, gs, n_tricks, depth
                    );
                }
            }
        }
    }

    /// The early-termination heuristic should kick in when every player is
    /// already locked at score 0 — and it should produce the same value as
    /// the un-tuned solver doing a full search.
    #[test]
    fn oh_hell_early_termination_matches_full_search() {
        // 2-trick state where all three players are guaranteed to score 0
        // after trick 1:
        //   P0 bid 0 but won trick 1 (busted, locked at 0).
        //   P1 bid 2 but won 0 with 1 trick left (can't make, locked at 0).
        //   P2 bid 2 but won 0 with 1 trick left (can't make, locked at 0).
        use games::gamestates::oh_hell::actions::{OHAction, OHCard};
        let mut gs = OhHell::new_state(2);
        // Deals: P0=9s,Ts ; P1=9c,Tc ; P2=9h,Th  (order: P0,P1,P2 x2)
        let deals = [
            OHCard::NS, OHCard::NC, OHCard::NH,
            OHCard::TS, OHCard::TC, OHCard::TH,
        ];
        for c in deals {
            gs.apply_action(OHAction::Card(c).into());
        }
        // Face up: TD → trump = Diamonds (no one holds a diamond).
        gs.apply_action(OHAction::Card(OHCard::TD).into());
        // Bids: P0=0 (sandbag), P1=2 (greedy), P2=2 (greedy).
        gs.apply_action(OHAction::Bid(0).into());
        gs.apply_action(OHAction::Bid(2).into());
        gs.apply_action(OHAction::Bid(2).into());

        // Play trick 1: P0 leads 9s, P1/P2 play non-spade non-trump → P0
        // wins on the lead-suit rule.
        gs.apply_action(OHAction::Card(OHCard::NS).into());
        gs.apply_action(OHAction::Card(OHCard::NC).into());
        gs.apply_action(OHAction::Card(OHCard::NH).into());

        // Mid-game but every final score is now locked.
        assert!(!gs.is_terminal());
        assert!(oh_hell_early_terminate(&gs));

        for p in 0..NUM_PLAYERS {
            let mut baseline = OpenHandSolver::default();
            let mut tuned = OpenHandSolver::new_oh_hell();
            let v_default = baseline.evaluate_player(&gs, p);
            let v_tuned = tuned.evaluate_player(&gs, p);
            assert_eq!(v_default, v_tuned);
            // Everyone scores 0 → evaluate = 0 - 0 = 0.
            assert_eq!(v_tuned, 0.0, "all-locked scenario should evaluate to 0");
        }
    }

    /// Sanity: the solver is deterministic for Oh Hell, like Bluff.
    #[test]
    fn oh_hell_solver_deterministic() {
        let mut rng: StdRng = SeedableRng::seed_from_u64(7);
        let mut gs = OhHell::new_state(2);
        drive_to_play(&mut gs, &mut rng);

        let mut solver = OpenHandSolver::new_oh_hell();
        let baseline = solver.evaluate_player(&gs, 0);
        for _ in 0..50 {
            assert_eq!(solver.evaluate_player(&gs, 0), baseline);
        }
    }
}
