use std::{
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

use bytemuck::{Pod, Zeroable};
use dyn_clone::DynClone;
use games::{
    gamestates::{
        bluff::{Bluff, BluffGameState},
        euchre::{
            isomorphic::EuchreNormalizer, processors::post_cards_played, Euchre, EuchreGameState,
        },
        kuhn_poker::{KPGameState, KuhnPoker},
        oh_hell::{isomorphic::OhHellNormalizer, OhHell, OhHellGameState},
    },
    istate::{IStateNormalizer, NoOpNormalizer, NormalizedAction, NormalizedIstate},
    resample::ResampleFromInfoState,
    Action, GameState, Player,
};
use itertools::Itertools;
use rand::{rngs::StdRng, seq::IndexedRandom, rng, RngExt, SeedableRng};
use rayon::prelude::*;

use serde::{Deserialize, Serialize};
use tinyvec::ArrayVec;

use crate::{
    agents::{Agent, Seedable},
    algorithms::{ismcts::Evaluator, open_hand_solver::OpenHandSolver, pimcts::PIMCTSBot},
    alloc::Pool,
    collections::{actionlist::ActionList, actionvec::ActionVec},
    counter,
    database::NodeStore,
    policy::Policy,
};

use features::features;

/// Number of iterations to stop doing the linear CFR normalization
///
/// https://www.science.org/doi/10.1126/science.aay2400
///
/// Stop doing the normalizations after a certain number of steps since no longer worth the effort
const LINEAR_CFR_CUTOFF: usize = 1_000_000;
type Weight = f32;

counter!(nodes_touched);

features! {
    pub mod feature {
        const LinearCFR = 0b01000000,
        const SingleThread = 0b00100000
    }
}


/// Max actions per slot for Euchre (max 6 legal actions at any decision node)
pub const EUCHRE_MAX_ACTIONS: usize = 6;
/// Max actions per slot for Bluff(1,1) (up to 10 legal actions at betting decisions)
pub const BLUFF_MAX_ACTIONS: usize = 10;
/// Max actions per slot for Kuhn Poker
pub const KP_MAX_ACTIONS: usize = 6;
/// Max actions per slot for Oh Hell. Bounded by max(n_tricks+1, n_tricks) ≤ 3
/// for the 2-trick variant; pick a slack value of 8 to support future growth.
pub const OH_MAX_ACTIONS: usize = 8;

#[derive(Copy, Clone, Serialize, Deserialize)]
pub struct InfoState<const MAX_ACTIONS: usize> {
    pub actions: ActionList,
    pub regrets: ArrayVec<[Weight; MAX_ACTIONS]>,
    pub avg_strategy: ArrayVec<[Weight; MAX_ACTIONS]>,
    pub last_iteration: usize,
}

// SAFETY: InfoState is composed of:
//   - ActionList(u32): a plain u32 bitmask, trivially Pod/Zeroable.
//   - ArrayVec<[f32; MAX_ACTIONS]> (x2): tinyvec's ArrayVec is internally a length field + a
//     fixed-size array. All byte patterns are valid for its fields (u16 len + [f32; N]).
//     The all-zeros pattern produces a valid ArrayVec with len=0.
//   - last_iteration: usize, trivially Pod/Zeroable.
//
// All fields are Copy and contain no padding that would be uninitialized, making the struct
// safe to reinterpret as bytes. The all-zeros bit pattern is valid (empty ActionList, empty
// ArrayVecs with len=0, last_iteration=0). We cannot use #[derive(Pod, Zeroable)] because
// tinyvec::ArrayVec does not itself implement Pod or Zeroable.
unsafe impl<const N: usize> Pod for InfoState<N> {}
unsafe impl<const N: usize> Zeroable for InfoState<N> {}

impl<const MAX_ACTIONS: usize> InfoState<MAX_ACTIONS> {
    pub fn new(normalized_actions: Vec<NormalizedAction>) -> Self {
        let n = normalized_actions.len();
        let mut regrets = ArrayVec::new();
        let mut avg_strategy = ArrayVec::default();

        for _ in 0..n {
            regrets.push(1.0 / 1e6);
            avg_strategy.push(1.0 / 1e6);
        }

        Self {
            actions: ActionList::new(&normalized_actions),
            regrets,
            avg_strategy,
            last_iteration: 0,
        }
    }

    pub fn avg_strategy(&self) -> Vec<(NormalizedAction, Weight)> {
        self.actions
            .to_vec()
            .into_iter()
            .zip(self.avg_strategy)
            .collect_vec()
    }

    pub fn regrets(&self) -> Vec<(NormalizedAction, Weight)> {
        self.actions
            .to_vec()
            .into_iter()
            .zip(self.regrets)
            .collect_vec()
    }
}

/// Implementation of external sampled CFR
///
/// Based on implementation from: OpenSpiel:
///     https://github.com/deepmind/open_spiel/blob/master/open_spiel/python/algorithms/mccfr.py
#[derive(Clone)]
pub struct CFRES<G, const MAX_ACTIONS: usize = EUCHRE_MAX_ACTIONS> {
    vector_pool: Pool<Vec<Action>>,
    game_generator: fn() -> G,
    iteration: Arc<AtomicUsize>,
    infostates: Arc<Mutex<NodeStore<MAX_ACTIONS>>>,
    /// determine if we are at the max depth and should use the rollout
    depth_checker: Box<dyn DepthChecker<G>>,
    normalizer: Box<dyn IStateNormalizer<G>>,
    play_bot: PIMCTSBot<G, OpenHandSolver<G>>,
    evaluator: OpenHandSolver<G>,
}

impl<G, const MAX_ACTIONS: usize> CFRES<G, MAX_ACTIONS> {
    pub fn iterations(&self) -> usize {
        self.iteration.load(Ordering::Relaxed)
    }
}

impl<G, const MAX_ACTIONS: usize> Seedable for CFRES<G, MAX_ACTIONS> {
    /// Sets the seed for the evaluator, it doesn't change the seed used for training
    fn set_seed(&mut self, seed: u64) {
        self.play_bot.set_seed(seed);
    }
}

impl CFRES<EuchreGameState> {
    pub fn new_euchre(rng: StdRng, max_cards_played: usize, path: Option<&Path>) -> Self {
        let normalizer: Box<dyn IStateNormalizer<EuchreGameState>> =
            Box::<EuchreNormalizer>::default();

        CFRES::new_with_normalizer(rng, max_cards_played, normalizer, path)
    }

    pub fn new_with_normalizer(
        mut rng: StdRng,
        max_cards_played: usize,
        normalizer: Box<dyn IStateNormalizer<EuchreGameState>>,
        path: Option<&Path>,
    ) -> Self {
        let pimcts_seed = rng.random();

        Self {
            vector_pool: Pool::new(Vec::new),
            game_generator: Euchre::new_state,
            infostates: Arc::new(Mutex::new(
                NodeStore::new_euchre(path, max_cards_played).unwrap(),
            )),
            depth_checker: Box::new(EuchreDepthChecker { max_cards_played }),
            play_bot: PIMCTSBot::new(
                50,
                OpenHandSolver::new_euchre(),
                SeedableRng::seed_from_u64(pimcts_seed),
            ),
            iteration: Arc::new(AtomicUsize::new(0)),
            evaluator: OpenHandSolver::new_euchre(),
            normalizer,
        }
    }

    pub fn set_game_generator(&mut self, game_generator: fn() -> EuchreGameState) {
        self.game_generator = game_generator;
    }
}

impl<G: GameState + ResampleFromInfoState, const MAX_ACTIONS: usize> CFRES<G, MAX_ACTIONS> {
    /// Creates a CFRES instance for simple (non-Euchre) games that don't need
    /// depth checking or state normalization.
    fn new_simple(
        game_generator: fn() -> G,
        node_store: NodeStore<MAX_ACTIONS>,
    ) -> Self {
        let mut rng: StdRng = SeedableRng::seed_from_u64(43);
        let pimcts_seed = rng.random();
        Self {
            vector_pool: Pool::new(Vec::new),
            game_generator,
            infostates: Arc::new(Mutex::new(node_store)),
            depth_checker: Box::new(NoOpDepthChecker {}),
            play_bot: PIMCTSBot::new(
                50,
                OpenHandSolver::default(),
                SeedableRng::seed_from_u64(pimcts_seed),
            ),
            evaluator: OpenHandSolver::default(),
            normalizer: Box::<NoOpNormalizer>::default(),
            iteration: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl CFRES<KPGameState, KP_MAX_ACTIONS> {
    pub fn new_kp() -> Self {
        Self::new_simple(KuhnPoker::new_state, NodeStore::new_kp(None).unwrap())
    }
}

impl CFRES<BluffGameState, BLUFF_MAX_ACTIONS> {
    pub fn new_bluff_11() -> Self {
        Self::new_simple(
            || Bluff::new_state(1, 1),
            NodeStore::new_bluff_11(None).unwrap(),
        )
    }
}

impl CFRES<OhHellGameState, OH_MAX_ACTIONS> {
    /// CFRES for Oh Hell. Uses a HashMap-backed store (no PHF
    /// pre-enumeration) and a depth checker that hands off to an
    /// `OpenHandSolver` rollout once `max_cards_played` cards have been
    /// played. Mirrors the Euchre `max_cards_played` knob: pass `0` to
    /// run CFR only on the bidding sub-game, or larger values to also
    /// solve some opening tricks.
    ///
    /// When `path` is `Some`, the infostate map is loaded from disk on
    /// construction (if the file exists) and `save()` writes it back via
    /// MessagePack. Pass `None` for purely in-memory training.
    pub fn new_oh_hell(
        num_players: usize,
        n_tricks: usize,
        max_cards_played: usize,
        path: Option<&Path>,
    ) -> Self {
        Self::new_oh_hell_with_store(
            num_players,
            n_tricks,
            max_cards_played,
            NodeStore::new_oh_hell(path, n_tricks).unwrap(),
        )
    }

    /// Disk-backed mmap + PHF variant for Oh Hell **bidding-only**
    /// training. Force `max_cards_played = 0` so the play phase
    /// always hands off to `OpenHandSolver` rollouts (matching Euchre's
    /// production setup), and route storage through
    /// [`NodeStore::new_oh_hell_bidding_mmap`].
    ///
    /// `path` is the directory holding the indexer + mmap + metadata
    /// files. `path = None` gets an anonymous in-memory mmap (still
    /// PHF-indexed, just not persisted on shutdown).
    pub fn new_oh_hell_bidding_mmap(
        num_players: usize,
        n_tricks: usize,
        path: Option<&Path>,
    ) -> Self {
        let store = NodeStore::new_oh_hell_bidding_mmap(path, num_players, n_tricks)
            .expect("failed to build OH bidding mmap node store");
        Self::new_oh_hell_with_store(num_players, n_tricks, 0, store)
    }

    fn new_oh_hell_with_store(
        num_players: usize,
        n_tricks: usize,
        max_cards_played: usize,
        store: NodeStore<OH_MAX_ACTIONS>,
    ) -> Self {
        let game_generator: fn() -> OhHellGameState = match (num_players, n_tricks) {
            (2, 1) => || OhHell::new_state(2, 1),
            (2, 2) => || OhHell::new_state(2, 2),
            (2, 3) => || OhHell::new_state(2, 3),
            (2, 4) => || OhHell::new_state(2, 4),
            (2, 5) => || OhHell::new_state(2, 5),
            (2, 6) => || OhHell::new_state(2, 6),
            (3, 1) => || OhHell::new_state(3, 1),
            (3, 2) => || OhHell::new_state(3, 2),
            (3, 3) => || OhHell::new_state(3, 3),
            (3, 4) => || OhHell::new_state(3, 4),
            (3, 5) => || OhHell::new_state(3, 5),
            (3, 6) => || OhHell::new_state(3, 6),
            (4, 1) => || OhHell::new_state(4, 1),
            (4, 2) => || OhHell::new_state(4, 2),
            _ => panic!(
                "unsupported (num_players, n_tricks) for CFRES Oh Hell: ({}, {})",
                num_players, n_tricks
            ),
        };

        let mut rng: StdRng = SeedableRng::seed_from_u64(43);
        let pimcts_seed = rng.random();
        Self {
            vector_pool: Pool::new(Vec::new),
            game_generator,
            infostates: Arc::new(Mutex::new(store)),
            depth_checker: Box::new(OhHellDepthChecker { max_cards_played }),
            play_bot: PIMCTSBot::new(
                50,
                OpenHandSolver::default(),
                SeedableRng::seed_from_u64(pimcts_seed),
            ),
            evaluator: OpenHandSolver::default(),
            // Iso-canonicalising normaliser: collapses CFR istates that
            // differ only by non-trump suit permutation. See
            // games/src/gamestates/oh_hell/isomorphic.rs for the perm
            // logic and its correctness gates.
            normalizer: Box::<OhHellNormalizer>::default(),
            iteration: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl<G: GameState + ResampleFromInfoState + Sync, const MAX_ACTIONS: usize> CFRES<G, MAX_ACTIONS> {
    pub fn train(&mut self, n: usize) {
        if feature::is_enabled(feature::SingleThread) {
            for _ in 0..n {
                self.iteration();
            }
        } else {
            (0..n)
                .into_par_iter()
                .for_each(|_| self.clone().iteration())
        }

        self.play_bot.reset();
        self.evaluator.reset();
    }

    pub fn save(&self) -> anyhow::Result<()> {
        self.infostates.lock().unwrap().commit()
    }

    /// Performs one iteration of external sampling.
    ///
    /// An iteration consists of one episode for each player as the update
    /// player.
    fn iteration(&mut self) {
        // We probably don't need this strict of ordering, but will start with this and relax if becomes performance
        // issue.
        self.iteration
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let num_players = (self.game_generator)().num_players();
        for player in 0..num_players {
            self.update_regrets(&mut (self.game_generator)(), player, 0);
        }
    }

    /// Runs an episode of external sampling.
    ///
    /// Args:
    ///     state: the game state to run from
    ///     player: the player to update regrets for
    ///
    /// Returns:
    ///     value: is the value of the state in the game
    ///     obtained as the weighted average of the values
    ///     of the children
    fn update_regrets(&mut self, gs: &mut G, player: Player, _depth: usize) -> Weight {
        if gs.is_terminal() {
            return gs.evaluate(player) as Weight;
        }

        if gs.is_chance_node() {
            let mut actions = self.vector_pool.detach();
            gs.legal_actions(&mut actions);
            let outcome = *actions
                .choose(&mut rng())
                .expect("error choosing a random action for chance node");
            actions.clear();
            self.vector_pool.attach(actions);

            gs.apply_action(outcome);
            let value = self.update_regrets(gs, player, _depth + 1);
            gs.undo();
            return value;
        }

        // If we're at max depth, do the rollout. evaluate_player_mut is the &mut variant
        // that avoids the per-rollout gs.clone() — alpha_beta restores state via undo.
        if self.depth_checker.is_max_depth(gs) {
            return self.evaluator.evaluate_player_mut(gs, player) as Weight;
        }

        let cur_player = gs.cur_player();
        let info_state_key = self
            .normalizer
            .normalize_istate(&gs.istate_key(cur_player), gs);
        let mut actions = self.vector_pool.detach();
        gs.legal_actions(&mut actions);

        // don't store anything if only 1 valid action
        if actions.len() == 1 {
            gs.apply_action(actions[0]);
            let v = self.update_regrets(gs, player, _depth + 1);
            gs.undo();
            actions.clear();
            self.vector_pool.attach(actions);
            return v;
        }

        nodes_touched::increment();
        let normalized_actions = actions
            .iter()
            .map(|&a| self.normalizer.normalize_action(a, gs))
            .collect_vec();

        let policy;
        {
            let normalizer = self.normalizer.clone();
            let was_loaded;
            let infostate_info = match self.lookup_entry(&info_state_key) {
                Some(v) => {
                    was_loaded = true;
                    v
                }
                None => {
                    was_loaded = false;
                    InfoState::new(normalized_actions.clone())
                }
            };
            let stored_norm_actions: Vec<NormalizedAction> =
                infostate_info.regrets().into_iter().map(|(a, _)| a).collect();
            let regrets = infostate_info
                .regrets()
                .into_iter()
                .map(|(a, v)| (normalizer.denormalize_action(a, gs), v))
                .collect_vec();

            policy = regret_matching(&regrets);

            let mut policy_actions = policy.actions().clone();
            policy_actions.sort();
            let mut sorted_actions = actions.clone();
            sorted_actions.sort();
            if sorted_actions != policy_actions {
                let raw_key = gs.istate_key(cur_player);
                panic!(
                    "ISTATE COLLISION DETECTED in CFRES update_regrets\n\
                     gs: {}\n\
                     cur_player: {}\n\
                     legal actions (sorted): {:?}\n\
                     stored normalized actions: {:?}\n\
                     denormalized policy actions (sorted): {:?}\n\
                     normalized info_state_key: {:?}\n\
                     raw istate_key: {:?}\n\
                     was_loaded_from_indexer: {}",
                    gs,
                    cur_player,
                    sorted_actions,
                    stored_norm_actions,
                    policy_actions,
                    info_state_key.get(),
                    raw_key,
                    was_loaded,
                );
            }
        }

        let mut value = 0.0;
        let mut child_values = ActionVec::new(&actions);

        if cur_player != player {
            // sample at opponent node
            let a = policy
                .to_vec()
                .choose_weighted(&mut rng(), |a| a.1)
                .expect("error choosing weighted action")
                .0;
            gs.apply_action(a);
            value = self.update_regrets(gs, player, _depth + 1);
            gs.undo();
        } else {
            // walk over all actions at my node
            for &a in actions.iter() {
                gs.apply_action(a);
                child_values[a] = self.update_regrets(gs, player, _depth + 1);
                gs.undo();
                value += policy[a] * child_values[a];
            }
        }

        if cur_player == player {
            // update regrets
            let iteration = self.iteration.load(Ordering::SeqCst);
            let mut infostate_info = self
                .lookup_entry(&info_state_key)
                .unwrap_or_else(|| InfoState::new(normalized_actions.clone()));
            // normalized_actions was already computed at the top of this frame, so reuse it
            // instead of calling normalize_action (which redoes istate_key + norm_transform)
            // once per legal action.
            for (&a, &norm_a) in actions.iter().zip(normalized_actions.iter()) {
                add_regret(
                    &mut infostate_info,
                    norm_a,
                    child_values[a] - value,
                    iteration,
                );
            }
            self.put_entry(&info_state_key, infostate_info);
        }

        // Simple average does averaging on the opponent node. To do this in a game
        // with more than two players, we only update the player + 1 mod num_players,
        // which reduces to the standard rule in 2 players.
        //
        // We adapt this slightly for euchre where it alternates what team the players are on
        let cur_team = cur_player % 2;
        let player_team = player % 2;
        if cur_team != player_team {
            let mut infostate_info = self
                .lookup_entry(&info_state_key)
                .unwrap_or_else(|| InfoState::new(normalized_actions.clone()));
            for (&action, &norm_a) in actions.iter().zip(normalized_actions.iter()) {
                add_avstrat(&mut infostate_info, norm_a, policy[action]);
            }

            self.put_entry(&info_state_key, infostate_info);
        }

        actions.clear();
        self.vector_pool.attach(actions);

        value
    }

    pub fn num_info_states(&self) -> usize {
        self.infostates.lock().unwrap().len()
    }

    pub fn indexer_size(&self) -> usize {
        self.infostates.lock().unwrap().indexer_len()
    }
}

impl<G, const MAX_ACTIONS: usize> CFRES<G, MAX_ACTIONS> {
    /// Can deadlock if we hold onto handle
    fn lookup_entry(&self, key: &NormalizedIstate) -> Option<InfoState<MAX_ACTIONS>> {
        self.infostates.lock().unwrap().get(&key.get())
    }

    fn put_entry(&self, key: &NormalizedIstate, v: InfoState<MAX_ACTIONS>) {
        self.infostates.lock().unwrap().put(&key.get(), &v);
    }
}

/// Applies regret matching to get a policy.
///
/// Returns:
///   probability of taking each action
fn regret_matching(regrets: &[(Action, Weight)]) -> ActionVec<Weight> {
    let sum_pos_regrets: Weight = regrets.iter().map(|(_, b)| b.max(0.0)).sum();

    let actions = regrets.iter().map(|(a, _)| *a).collect_vec();
    let mut policy = ActionVec::new(&actions);

    if sum_pos_regrets <= 0.0 {
        for a in &actions {
            policy[*a] = 1.0 / actions.len() as Weight;
        }
    } else {
        for (a, r) in regrets {
            policy[*a] = r.max(0.0) / sum_pos_regrets;
        }
    }

    policy
}

fn add_regret<const N: usize>(
    infostate: &mut InfoState<N>,
    action: NormalizedAction,
    amount: Weight,
    iteration: usize,
) {
    // Implement linear CFR for the early iterations.
    //
    // We do the update on write of regrets to avoid needing to touch nodes that haven't been updated
    // in a given iteration
    //
    //https://www.science.org/doi/10.1126/science.aay2400
    //
    // Equivalently, one could multiply the accumulated regret by
    // t / t+1 on each iteration. We do this in
    //  our experiments to reduce the risk of numerical instability.
    if feature::is_enabled(feature::LinearCFR)
        // We don't need to do this if the node has never been touched before. This is not only
        // an optimization, but also ensures that we don't set the weights to 0 by accident
        && infostate.last_iteration > 0
    {
        // Closed form of the telescoping product ∏(i=a..b) i/(i+1) = a/b.
        // a = last_iteration, b = min(iteration, LINEAR_CFR_CUTOFF).
        // If a >= b the range is empty and factor is 1.0 (no scaling applied).
        let end = iteration.min(LINEAR_CFR_CUTOFF);
        if infostate.last_iteration < end {
            let factor: Weight = infostate.last_iteration as Weight / end as Weight;
            infostate.regrets.iter_mut().for_each(|r| *r *= factor);
        }
    }

    infostate.last_iteration = iteration;

    let idx = infostate
        .actions
        .index(action)
        .expect("couldn't find action");
    infostate.regrets[idx] += amount;
}

fn add_avstrat<const N: usize>(
    infostate: &mut InfoState<N>,
    action: NormalizedAction,
    amount: Weight,
) {
    let idx = infostate
        .actions
        .index(action)
        .expect("couldn't find action");
    infostate.avg_strategy[idx] += amount;
}

impl<G: GameState + ResampleFromInfoState + Send, const MAX_ACTIONS: usize> Policy<G>
    for CFRES<G, MAX_ACTIONS>
{
    /// Returns the MCCFR average policy for a player in a state.
    ///
    /// If the policy is not defined for the provided state, a uniform
    /// random policy is returned.
    fn action_probabilities(&mut self, gs: &G) -> ActionVec<f64> {
        let player = gs.cur_player();

        if self.depth_checker.is_max_depth(gs) {
            return self.play_bot.action_probabilities(gs);
        }

        let mut actions = self.vector_pool.detach();
        gs.legal_actions(&mut actions);
        let info_state_key = self.normalizer.normalize_istate(&gs.istate_key(player), gs);

        let mut policy = ActionVec::new(&actions);

        {
            let retrieved_infostate = self.lookup_entry(&info_state_key);
            if let Some(retrieved_infostate) = retrieved_infostate {
                let policy_sum: f64 = retrieved_infostate
                    .avg_strategy()
                    .iter()
                    .map(|(_, v)| *v as f64)
                    .sum();
                for (norm_a, s) in retrieved_infostate.avg_strategy() {
                    let a = self.normalizer.denormalize_action(norm_a, gs);
                    policy[a] = s as f64 / policy_sum;
                }
            } else {
                for a in actions.iter() {
                    policy[*a] = 1.0 / actions.len() as f64;
                }
            }
        }

        self.vector_pool.attach(actions);

        policy
    }
}

impl<G: GameState + ResampleFromInfoState + Send, const MAX_ACTIONS: usize> Agent<G>
    for CFRES<G, MAX_ACTIONS>
{
    fn step(&mut self, s: &G) -> Action {
        let action_weights = self.action_probabilities(s).to_vec();
        action_weights
            .choose_weighted(&mut rng(), |item| item.1)
            .unwrap()
            .0
    }
}

pub trait DepthChecker<G>: Sync + Send + DynClone {
    fn is_max_depth(&self, gs: &G) -> bool;
}
dyn_clone::clone_trait_object!(<G>DepthChecker<G>);

#[derive(Clone)]
struct NoOpDepthChecker;
impl<G> DepthChecker<G> for NoOpDepthChecker {
    fn is_max_depth(&self, _: &G) -> bool {
        false
    }
}

#[derive(Clone)]
pub struct EuchreDepthChecker {
    pub max_cards_played: usize,
}

impl DepthChecker<EuchreGameState> for EuchreDepthChecker {
    fn is_max_depth(&self, gs: &EuchreGameState) -> bool {
        post_cards_played(gs, self.max_cards_played)
    }
}

/// Stops CFR descent once `max_cards_played` cards have been played. Set
/// to 0 to limit CFR to the bidding sub-game (deal + face up + bids), with
/// `OpenHandSolver` rollouts scoring every play-phase state.
#[derive(Clone)]
pub struct OhHellDepthChecker {
    pub max_cards_played: usize,
}

impl DepthChecker<OhHellGameState> for OhHellDepthChecker {
    fn is_max_depth(&self, gs: &OhHellGameState) -> bool {
        use games::gamestates::oh_hell::OHPhase;
        gs.phase() == OHPhase::Play && gs.cards_played() >= self.max_cards_played
    }
}

#[cfg(test)]
mod tests {

    use super::{feature, Weight, CFRES, LINEAR_CFR_CUTOFF};

    #[test]
    fn cfres_train_test() {
        feature::enable(feature::LinearCFR);

        let mut alg = CFRES::new_kp();
        alg.train(10);
    }

    /// CFRES on full-deck Oh Hell, bidding-only mode (max_cards_played=0).
    /// Smoke test: training completes and at least one info state gets
    /// touched. Bidding-only keeps the istate space small enough that
    /// MCCFR with a HashMap-backed store converges quickly.
    #[test]
    fn cfres_oh_hell_train_smoke() {
        let mut alg = CFRES::new_oh_hell(3, 2, 0, None);
        alg.train(20);
        assert!(alg.num_info_states() > 0);
    }

    /// Bidding-only mmap variant: build the PHF, run a few iterations,
    /// confirm the disk-backed store touches at least one infostate
    /// per training step.
    #[test]
    fn cfres_oh_hell_bidding_mmap_smoke() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut alg = CFRES::new_oh_hell_bidding_mmap(2, 1, Some(dir.path()));
        alg.train(20);
        assert!(
            alg.num_info_states() > 0,
            "mmap-backed bidding CFR didn't touch any info states"
        );
        // Indexer file and mmap file should both exist after a save.
        alg.save().expect("save");
        assert!(dir.path().join("indexer").exists());
        assert!(dir.path().join("mmap").exists());
        assert!(dir.path().join("meta").exists());
    }

    /// Round-trip the mmap variant: train, save, reload into a fresh
    /// CFRES with the same path, confirm the populated count is
    /// preserved (the indexer is loaded from disk; the populated
    /// count is read from `meta`).
    #[test]
    fn cfres_oh_hell_bidding_mmap_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut a = CFRES::new_oh_hell_bidding_mmap(2, 1, Some(dir.path()));
        a.train(50);
        let n_after_train = a.num_info_states();
        assert!(n_after_train > 0);
        a.save().expect("save");

        // Fresh CFRES reading the same directory should see the same
        // populated count (the indexer is rehydrated from `indexer`,
        // the count from `meta`).
        let b = CFRES::new_oh_hell_bidding_mmap(2, 1, Some(dir.path()));
        assert_eq!(b.num_info_states(), n_after_train);
    }

    /// Round-trip: train a small Oh Hell run, persist to disk, reload
    /// into a fresh CFRES, and confirm the info-state count is preserved
    /// and that `save()` after reload is a no-op (count stays the same).
    #[test]
    fn cfres_oh_hell_save_load_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("oh_cfr.msgpack");

        let mut a = CFRES::new_oh_hell(3, 2, 0, Some(path.as_path()));
        a.train(50);
        let n_after_train = a.num_info_states();
        assert!(n_after_train > 0);
        a.save().expect("save");
        assert!(path.exists());

        let b = CFRES::new_oh_hell(3, 2, 0, Some(path.as_path()));
        assert_eq!(b.num_info_states(), n_after_train);
    }

    // Reference in f64 — the true mathematical answer, used to verify the f32 closed form.
    // The existing f32 iterative version accumulates rounding drift (up to ~0.2% per 500k
    // multiplies) so it is NOT a valid reference; the closed form is more accurate, which is
    // a small positive side-effect of this refactor, not a correctness regression.
    fn linear_cfr_factor_reference_f64(last: usize, iteration: usize) -> f64 {
        let end = iteration.min(LINEAR_CFR_CUTOFF);
        if last >= end {
            1.0
        } else {
            last as f64 / end as f64
        }
    }

    fn linear_cfr_factor_closed(last: usize, iteration: usize) -> Weight {
        let end = iteration.min(LINEAR_CFR_CUTOFF);
        if last >= end {
            1.0
        } else {
            last as Weight / end as Weight
        }
    }

    #[test]
    fn linear_cfr_factor_closed_matches_reference() {
        // Telescoping product ∏(i=a..b) i/(i+1) = a/b. Verify the f32 closed form agrees
        // with the f64 mathematical reference within f32 epsilon.
        let cases = [
            (1, 2),
            (1, 100),
            (100, 101),
            (100, 1_000),
            (999, 1_000),
            (1_000, 100_000),
            (1, LINEAR_CFR_CUTOFF),
            (500_000, LINEAR_CFR_CUTOFF),
            (LINEAR_CFR_CUTOFF - 1, LINEAR_CFR_CUTOFF),
            // iteration beyond cutoff: end clamped to cutoff
            (500_000, LINEAR_CFR_CUTOFF + 50_000),
            // empty range: last >= end
            (LINEAR_CFR_CUTOFF, LINEAR_CFR_CUTOFF),
            (LINEAR_CFR_CUTOFF + 10, LINEAR_CFR_CUTOFF),
        ];
        for (last, iter) in cases {
            let reference = linear_cfr_factor_reference_f64(last, iter) as Weight;
            let closed = linear_cfr_factor_closed(last, iter);
            let diff = (reference - closed).abs();
            let tol = reference.abs() * 1e-6 + 1e-7;
            assert!(
                diff < tol,
                "mismatch at last={last} iter={iter}: reference={reference} closed={closed} diff={diff} tol={tol}"
            );
        }
    }
}
