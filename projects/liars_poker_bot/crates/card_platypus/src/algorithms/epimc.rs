//! EPIMC: Perfect Information Monte Carlo with Postponing Reasoning.
//!
//! Reference: Arjonilla, Saffidine, Cazenave. CoG 2024. arXiv:2408.02380.
//!
//! Generalises [`PIMCTSBot`](super::pimcts::PIMCTSBot) with a `depth`
//! hyperparameter. After applying the candidate root action to a sampled
//! world, EPIMC plays `depth - 1` additional uniform-random actions on that
//! world before handing it to the perfect-information leaf evaluator. The
//! intent is to delay the moment the evaluator commits to a perfect-info
//! strategy, reducing strategy-fusion error on games with private hidden
//! information (Euchre, Bridge, Skat).
//!
//! At `depth == 1` this bot is value-equivalent to `PIMCTSBot` for the same
//! seed, rollouts, and evaluator (no extra RNG draws happen in that path).
//!
//! See `plans/epimc-gomcts-implementation.md` for the design rationale,
//! handoff notes, and the planned v2 (subgame + ImperfectAlgo).

use std::marker::PhantomData;

use games::{actions, resample::ResampleFromInfoState, Action, GameState, Player};
use rand::{rngs::StdRng, seq::IndexedRandom, Rng, SeedableRng};
use rayon::prelude::*;

use crate::{
    agents::{Agent, Seedable},
    collections::actionvec::ActionVec,
    policy::Policy,
};

use super::{ismcts::Evaluator, pimcts::get_worlds};

/// EPIMC (Extended PIMC) — PIMC generalised with a `depth` parameter.
///
/// `depth == 1` reproduces standard PIMC. Larger depths play extra uniform-
/// random actions before the leaf evaluator runs.
pub struct EPIMCBot<G, E> {
    n_rollouts: usize,
    depth: usize,
    rng: StdRng,
    solver: E,
    eval_count: usize,
    _phantom: PhantomData<G>,
}

impl<G, E: Clone> Clone for EPIMCBot<G, E> {
    fn clone(&self) -> Self {
        Self {
            n_rollouts: self.n_rollouts,
            depth: self.depth,
            rng: StdRng::from_rng(&mut rand::rng()),
            solver: self.solver.clone(),
            eval_count: self.eval_count,
            _phantom: PhantomData,
        }
    }
}

impl<G: GameState + ResampleFromInfoState + Send, E: Evaluator<G> + Clone + Sync> EPIMCBot<G, E> {
    /// `depth >= 1`. `depth == 1` is standard PIMC.
    pub fn new(n_rollouts: usize, depth: usize, solver: E, rng: StdRng) -> Self {
        assert!(depth >= 1, "EPIMC depth must be at least 1");
        Self {
            n_rollouts,
            depth,
            rng,
            solver,
            eval_count: 0,
            _phantom: PhantomData,
        }
    }

    pub fn reset(&mut self) {
        self.solver.reset();
    }

    /// Sum-and-average the leaf evaluator's value across `worlds`, playing
    /// `depth - 1` uniform-random actions on each world first. At depth=1
    /// this is structurally identical to PIMCTSBot's evaluate path.
    fn evaluate_with_worlds(&mut self, maximizing_player: Player, worlds: Vec<G>) -> f64 {
        self.eval_count += 1;
        if self.eval_count % 1_000 == 0 {
            self.reset();
        }

        let extra_steps = self.depth - 1;
        // Pull one base seed per call when we actually need it. At depth=1
        // this branch is skipped so self.rng evolves identically to
        // PIMCTSBot's, preserving the depth-1 equivalence test.
        let base_seed = if extra_steps > 0 {
            self.rng.next_u64()
        } else {
            0
        };

        let sum: f64 = worlds
            .into_par_iter()
            .enumerate()
            .map(|(idx, mut w)| {
                if extra_steps > 0 {
                    let mut sub_rng: StdRng = SeedableRng::seed_from_u64(
                        base_seed.wrapping_add(splitmix64(idx as u64)),
                    );
                    let mut buf: Vec<Action> = Vec::new();
                    for _ in 0..extra_steps {
                        if w.is_terminal() {
                            break;
                        }
                        w.legal_actions(&mut buf);
                        let a = *buf.choose(&mut sub_rng).expect("non-empty legal actions");
                        w.apply_action(a);
                        buf.clear();
                    }
                }
                let mut solver = self.solver.clone();
                solver.evaluate_player(&w, maximizing_player)
            })
            .sum();

        sum / self.n_rollouts as f64
    }
}

/// A cheap full-avalanche mix so different per-rollout indices yield well-
/// separated sub-seeds even when `base_seed` is close to zero.
fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

impl<G: GameState + ResampleFromInfoState + Send, E: Evaluator<G> + Clone + Sync> Evaluator<G>
    for EPIMCBot<G, E>
{
    fn evaluate_player(&mut self, gs: &G, maximizing_player: Player) -> f64 {
        let worlds = get_worlds(gs, self.n_rollouts, &mut self.rng);
        self.evaluate_with_worlds(maximizing_player, worlds)
    }

    fn evaluate(&mut self, gs: &G) -> Vec<f64> {
        let mut result = vec![0.0; gs.num_players()];
        let worlds = get_worlds(gs, self.n_rollouts, &mut self.rng);
        for (i, r) in result.iter_mut().enumerate().take(2) {
            *r = self.evaluate_with_worlds(i, worlds.clone());
        }
        for i in 2..result.len() {
            result[i] = result[i % 2];
        }
        result
    }

    fn prior(&mut self, gs: &G) -> ActionVec<f64> {
        self.action_probabilities(gs)
    }
}

impl<G: GameState + ResampleFromInfoState + Send, E: Evaluator<G> + Clone + Sync> Policy<G>
    for EPIMCBot<G, E>
{
    fn action_probabilities(&mut self, gs: &G) -> ActionVec<f64> {
        let mut values = Vec::new();
        let actions = actions!(gs);
        let player = gs.cur_player();

        // Share one set of resampled worlds across all candidate actions, the
        // same variance-reduction trick PIMCTSBot uses.
        let mut worlds = get_worlds(gs, self.n_rollouts, &mut self.rng);

        for &a in &actions {
            worlds.iter_mut().for_each(|w| w.apply_action(a));
            let v = self.evaluate_with_worlds(player, worlds.clone());
            values.push(v);
            worlds.iter_mut().for_each(|w| w.undo());
        }

        let mut probs = ActionVec::new(&actions);
        // Reverse first so ties resolve to the lowest index, matching
        // PIMCTSBot's tie-breaking.
        let index_of_max = values
            .iter()
            .enumerate()
            .rev()
            .max_by(|(_, a), (_, b)| a.total_cmp(b))
            .map(|(index, _)| index)
            .unwrap();
        probs[actions[index_of_max]] = 1.0;
        probs
    }
}

impl<G, E> Seedable for EPIMCBot<G, E> {
    fn set_seed(&mut self, seed: u64) {
        self.rng = SeedableRng::seed_from_u64(seed);
    }
}

impl<G: GameState + ResampleFromInfoState + Send, E: Evaluator<G> + Clone + Sync> Agent<G>
    for EPIMCBot<G, E>
{
    fn step(&mut self, s: &G) -> Action {
        let action_weights = self.action_probabilities(s).to_vec();
        action_weights
            .choose_weighted(&mut self.rng, |item| item.1)
            .unwrap()
            .0
    }
}

#[cfg(test)]
mod tests {
    use games::{
        gamestates::{
            euchre::EuchreGameState,
            kuhn_poker::{KPAction, KuhnPoker},
            oh_hell::OhHell,
        },
        GameState,
    };
    use rand::{rngs::StdRng, seq::IndexedRandom, SeedableRng};

    use crate::{
        agents::Agent,
        algorithms::{
            epimc::EPIMCBot, ismcts::Evaluator, open_hand_solver::OpenHandSolver,
            pimcts::PIMCTSBot,
        },
        policy::Policy,
    };

    /// At depth=1, EPIMC must be value-equivalent to PIMCTSBot when seeded
    /// identically. This is the central correctness invariant — it pins
    /// EPIMC's depth=1 behaviour to PIMC and prevents future refactors from
    /// silently diverging.
    #[test]
    fn epimc_matches_pimc_at_depth_1_kuhn() {
        let seed = 109;
        let rollouts = 64;
        let gs = KuhnPoker::from_actions(&[KPAction::Jack, KPAction::Queen]);

        let mut pimc = PIMCTSBot::new(
            rollouts,
            OpenHandSolver::default(),
            SeedableRng::seed_from_u64(seed),
        );
        let mut epimc = EPIMCBot::new(
            rollouts,
            1,
            OpenHandSolver::default(),
            SeedableRng::seed_from_u64(seed),
        );

        let p = pimc.action_probabilities(&gs);
        let e = epimc.action_probabilities(&gs);
        assert_eq!(p, e);
    }

    #[test]
    fn epimc_matches_pimc_at_depth_1_euchre() {
        let gs = EuchreGameState::from(
            "JsQs9hKhAh|TcQcKcThAd|9cJc9sAsQh|KsJh9dJdQd|Kd|PPPT|Ks|P|JsThAsJh|JdQsAd9c|Qd",
        );
        let seed = 42;
        let rollouts = 8;
        let mut pimc = PIMCTSBot::new(
            rollouts,
            OpenHandSolver::default(),
            SeedableRng::seed_from_u64(seed),
        );
        let mut epimc = EPIMCBot::new(
            rollouts,
            1,
            OpenHandSolver::default(),
            SeedableRng::seed_from_u64(seed),
        );
        assert_eq!(
            pimc.action_probabilities(&gs),
            epimc.action_probabilities(&gs)
        );
    }

    /// At depth>1 the random playout substitutes for the opponent's
    /// in-tree response, so EPIMC's payoff on Kuhn is allowed to drift away
    /// from the perfect-info answer. We just check sign (the worse hand
    /// still loses in expectation) and a reasonable bounded range.
    #[test]
    fn epimc_kuhn_smoke_depth_2() {
        let mut agent = EPIMCBot::new(
            100,
            2,
            OpenHandSolver::default(),
            SeedableRng::seed_from_u64(109),
        );
        let gs = KuhnPoker::from_actions(&[KPAction::Jack, KPAction::Queen]);
        let v = agent.evaluate(&gs);
        assert!(v[0] < 0.0 && v[1] > 0.0, "expected Jack to lose vs Queen, got {:?}", v);
        for x in &v {
            assert!(x.abs() <= 2.0, "Kuhn payoff out of [-2, 2]: {:?}", v);
        }
    }

    /// Smoke test: depth=2 EPIMC drives a full Oh Hell game to terminal.
    #[test]
    fn epimc_oh_hell_full_game() {
        use games::actions;
        let mut rng: StdRng = SeedableRng::seed_from_u64(7);
        let mut agent = EPIMCBot::new(
            8,
            2,
            OpenHandSolver::default(),
            SeedableRng::seed_from_u64(7),
        );
        let mut gs = OhHell::new_state(3, 1);
        while gs.is_chance_node() {
            let a = actions!(gs);
            let chosen = *a.choose(&mut rng).unwrap();
            gs.apply_action(chosen);
        }
        while !gs.is_terminal() {
            let action = agent.step(&gs);
            gs.apply_action(action);
        }
        assert!(gs.is_terminal());
    }

    /// Same Euchre fixture as `pimcts_consistency`: identical seed → identical
    /// policy on 100 fresh bots.
    #[test]
    fn epimc_consistency() {
        let gs = EuchreGameState::from(
            "JsQs9hKhAh|TcQcKcThAd|9cJc9sAsQh|KsJh9dJdQd|Kd|PPPT|Ks|P|JsThAsJh|JdQsAd9c|Qd",
        );

        let mut policy = EPIMCBot::new(
            10,
            2,
            OpenHandSolver::default(),
            SeedableRng::seed_from_u64(42),
        );
        let result = policy.action_probabilities(&gs);

        for _ in 0..50 {
            let mut policy = EPIMCBot::new(
                10,
                2,
                OpenHandSolver::default(),
                SeedableRng::seed_from_u64(42),
            );
            assert_eq!(policy.action_probabilities(&gs), result);
        }
    }
}
