//! Round-robin head-to-head tournament across every Euchre agent the
//! deployment supports plus the trained CFR depths and a uniform-random
//! baseline. Used to diagnose how much the CFR depths actually buy
//! against each other and against the no-training agents.
//!
//! Agents (each has its own metric series; metric name = `a_vs_b`):
//!   * random — uniform over legal actions; the zero-skill floor.
//!   * pimcts — PIMCTS w/ OpenHandSolver, 50 rollouts. No weights.
//!   * epimc2 — EPIMC w/ OpenHandSolver, 50 rollouts, postponing depth=2.
//!   * epimc3 — EPIMC w/ OpenHandSolver, 50 rollouts, postponing depth=3.
//!   * epimc4 — EPIMC w/ OpenHandSolver, 50 rollouts, postponing depth=4.
//!   * epimc5 — EPIMC w/ OpenHandSolver, 50 rollouts, postponing depth=5.
//!   * cfr0   — CFRES trained on bidding only (max_cards_played = 0).
//!              `euchre_server::bench`'s "medium" tier; also called
//!              "baseline" in older docs.
//!   * cfr1   — CFRES trained through 1 card played.
//!   * cfr2   — CFRES trained through 2 cards played.
//!   * cfr3   — CFRES trained through 3 cards played.
//!              `euchre_server::bench`'s "hard" tier.
//!   * gomcts — GO-MCTS over a trained Euchre transformer. Loads the
//!              safetensors checkpoint at $EUCHRE_GOMCTS_WEIGHTS
//!              (default `/tmp/euchre_gomcts/final.safetensors`).
//!              Config selected via $EUCHRE_GOMCTS_CONFIG (smoke /
//!              medium / paper).
//!
//! With 6 agents the tournament has C(6,2) = 15 pairings. We rotate
//! through the pairings in batches sized at `BENCH_BATCH_PCT` (default
//! 5%) of the per-pair game budget — every pair plays one batch, then
//! we cycle back. After each pair finishes its batch, we emit a kestrel
//! metric for that pair with `step` = cumulative games played for that
//! pair and value = A's cumulative match-win rate. So every pair has
//! data points at the same x-axis grid (50, 100, 150, …) regardless of
//! how slow some pairs are, and the metric chart fills in incrementally
//! across all 15 series as the tournament progresses.
//!
//! Weight paths come from env vars, falling back to the production layout:
//!   EUCHRE_CFR0_WEIGHTS    /home/steven/card_platypus/infostate.baseline
//!   EUCHRE_CFR1_WEIGHTS    /home/steven/card_platypus/infostate.one_card_played
//!   EUCHRE_CFR2_WEIGHTS    /home/steven/card_platypus/infostate.two_card_played
//!   EUCHRE_CFR3_WEIGHTS    /home/steven/card_platypus/infostate.three_card_played_f32
//!
//! Knobs:
//!   BENCH_MATCHES    matches per pairing (default 1000)
//!   BENCH_BATCH_PCT  batch size as % of matches per pair (default 5)
//!   BENCH_SEED       base RNG seed (default 0)
//!   BENCH_AGENTS     comma-separated subset (default
//!                    "random,pimcts,epimc2,epimc3,epimc4,epimc5,cfr0,cfr1,cfr2,cfr3")
//!
//! Run:
//!   cargo run -p card_platypus --release --example euchre_difficulty_benchmark

use std::{env, io::Write, path::PathBuf, time::Instant};

use card_platypus::{
    agents::Agent,
    algorithms::{
        cfres::EuchreCfres,
        epimc::EPIMCBot,
        gomcts::{GoMcts, GoMctsConfig},
        gomcts_transformer::{
            euchre::EuchreTokenizer, forward_histories_batch_tch, GoMctsTransformerTch,
            InferenceMode, Tokenizer, TransformerConfig,
        },
        open_hand_solver::OpenHandSolver,
        pimcts::PIMCTSBot,
    },
};
use games::{
    gamestates::euchre::{Euchre, EuchreGameState},
    Action, GameState,
};
use rand::{rngs::StdRng, seq::IndexedRandom, SeedableRng};

const WIN_SCORE: i32 = 10;

const DEFAULT_CFR0: &str = "/home/steven/card_platypus/infostate.baseline";
const DEFAULT_CFR1: &str = "/home/steven/card_platypus/infostate.one_card_played";
const DEFAULT_CFR2: &str = "/home/steven/card_platypus/infostate.two_card_played";
const DEFAULT_CFR3: &str = "/home/steven/card_platypus/infostate.three_card_played_f32";
const DEFAULT_GOMCTS: &str = "/home/steven/card_platypus/gomcts/bootstrap.safetensors";

type EuchreAgent = Box<dyn Agent<EuchreGameState>>;

/// Uniform random over legal actions. Uses `StdRng` (not `ThreadRng`) so
/// it's reproducible from a seed and is `Send`-friendly if we ever want
/// to parallelise.
struct RandomAgent {
    rng: StdRng,
    scratch: Vec<Action>,
}

impl RandomAgent {
    fn new(seed: u64) -> Self {
        Self { rng: StdRng::seed_from_u64(seed), scratch: Vec::new() }
    }
}

impl Agent<EuchreGameState> for RandomAgent {
    fn step(&mut self, s: &EuchreGameState) -> Action {
        self.scratch.clear();
        s.legal_actions(&mut self.scratch);
        *self.scratch.choose(&mut self.rng).expect("no legal actions")
    }
    fn get_name(&self) -> String {
        "random".to_string()
    }
}

fn parse_env<T: std::str::FromStr>(name: &str, default: T) -> T {
    env::var(name).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}

fn weights_path(env_var: &str, default: &str) -> PathBuf {
    env::var(env_var).map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(default))
}

fn load_cfr(env_var: &str, default: &str, max_cards_played: usize, seed: u64) -> EuchreAgent {
    let path = weights_path(env_var, default);
    assert!(
        path.exists(),
        "CFR weights not found at {} (set {} to override)",
        path.display(),
        env_var
    );
    let agent =
        EuchreCfres::new_euchre(StdRng::seed_from_u64(seed), max_cards_played, Some(&path));
    Box::new(agent)
}

/// Build a GO-MCTS agent backed by a trained transformer checkpoint.
///
/// Env knobs:
///   EUCHRE_GOMCTS_WEIGHTS       safetensors path (required)
///   EUCHRE_GOMCTS_CONFIG        smoke|medium|paper (must match training)
///   EUCHRE_GOMCTS_ITER          per-decision MCTS budget (default 32)
///   EUCHRE_GOMCTS_ROLLOUT_STEPS rollout phase length per leaf (default 0)
///   EUCHRE_GOMCTS_INFER         argmaxval|lm|gated (default argmaxval).
///                               `lm`: LM-head softmax (use for a
///                               supervised-only bootstrap whose value
///                               head hasn't seen counterfactual
///                               actions). `gated`: ArgmaxVal* over the
///                               actions whose LM prob ≥ λ — the
///                               paper's legality/plausibility gate.
///   EUCHRE_GOMCTS_LAMBDA        λ for gated mode (default 0.05)
///   EUCHRE_GOMCTS_TEMP          value-softmax temp (default 0.5;
///                               paper's deterministic argmax ≈ 0.05)
/// `GenerativeModel` impl that owns a tch transformer directly (no
/// service thread). Calls `forward_histories_batch_tch` per query.
/// Suitable for the round-robin tournament where each agent's `step`
/// runs sequentially on the main thread — there is no cross-game
/// batching to gain from the service architecture here.
struct TchInlineModel {
    net: std::sync::Arc<GoMctsTransformerTch>,
    tokenizer: EuchreTokenizer,
    mode: InferenceMode,
    lambda: f64,
    temp: f64,
}

impl card_platypus::algorithms::gomcts::GenerativeModel<EuchreGameState> for TchInlineModel {
    fn sample(
        &mut self,
        history: &games::istate::IStateKey,
        legal: &[Action],
        rng: &mut StdRng,
    ) -> Action {
        let probs = self.policy(history, legal);
        let mut r: f64 = rand::RngExt::random::<f64>(rng);
        for (i, p) in probs.iter().enumerate() {
            r -= *p;
            if r <= 0.0 {
                return legal[i];
            }
        }
        *legal.choose(rng).expect("non-empty legal")
    }

    fn value(&mut self, history: &games::istate::IStateKey) -> f64 {
        let (_, values) =
            forward_histories_batch_tch(&self.net, &self.tokenizer, &[*history]).expect("forward");
        values[0] as f64
    }

    fn policy(
        &mut self,
        history: &games::istate::IStateKey,
        legal: &[Action],
    ) -> Vec<f64> {
        let uniform = || vec![1.0 / legal.len() as f64; legal.len()];
        let needs_lm = self.mode == InferenceMode::LmSoftmax || self.lambda > 0.0;
        // Forward [h, h⊕a1, …, h⊕ak] in one batch when the LM head is
        // needed; logits[0] is the next-action distribution at h and
        // values[1..] are V(h⊕a). Without the LM we skip the h row.
        let mut histories: Vec<games::istate::IStateKey> = Vec::with_capacity(legal.len() + 1);
        if needs_lm {
            histories.push(*history);
        }
        histories.extend(legal.iter().map(|&a| {
            let mut h = *history;
            h.push(a);
            h
        }));
        let (logits, values) =
            match forward_histories_batch_tch(&self.net, &self.tokenizer, &histories) {
                Ok(x) => x,
                Err(_) => return uniform(),
            };
        let softmax = |vals: &[f64], mask: &[bool], temp: f64| -> Vec<f64> {
            let max = vals
                .iter()
                .zip(mask)
                .filter(|(_, &m)| m)
                .map(|(&v, _)| v)
                .fold(f64::NEG_INFINITY, f64::max);
            if !max.is_finite() {
                return uniform();
            }
            let exps: Vec<f64> = vals
                .iter()
                .zip(mask)
                .map(|(&v, &m)| if m { ((v - max) / temp).exp() } else { 0.0 })
                .collect();
            let total: f64 = exps.iter().sum();
            if total == 0.0 || !total.is_finite() {
                return uniform();
            }
            exps.into_iter().map(|e| e / total).collect()
        };
        let all_true = vec![true; legal.len()];
        if needs_lm {
            let lm_logits: Vec<f64> = legal
                .iter()
                .map(|&a| {
                    logits[0]
                        .get(self.tokenizer.action_token(a) as usize)
                        .copied()
                        .unwrap_or(f32::MIN) as f64
                })
                .collect();
            let p_lm = softmax(&lm_logits, &all_true, 1.0);
            match self.mode {
                InferenceMode::LmSoftmax => p_lm,
                InferenceMode::ArgmaxVal => {
                    let vals: Vec<f64> = values[1..].iter().map(|&v| v as f64).collect();
                    let mut gate: Vec<bool> =
                        p_lm.iter().map(|&p| p >= self.lambda).collect();
                    if !gate.iter().any(|&g| g) {
                        gate = all_true;
                    }
                    softmax(&vals, &gate, self.temp)
                }
            }
        } else {
            let vals: Vec<f64> = values.iter().map(|&v| v as f64).collect();
            softmax(&vals, &all_true, self.temp)
        }
    }

    fn batch_value(&mut self, histories: &[games::istate::IStateKey]) -> Vec<f64> {
        if histories.is_empty() {
            return Vec::new();
        }
        let (_, values) = match forward_histories_batch_tch(&self.net, &self.tokenizer, histories) {
            Ok(x) => x,
            Err(_) => return vec![0.0; histories.len()],
        };
        values.into_iter().map(|v| v as f64).collect()
    }
}

fn load_gomcts(seed: u64) -> EuchreAgent {
    let path = weights_path("EUCHRE_GOMCTS_WEIGHTS", DEFAULT_GOMCTS);
    assert!(
        path.exists(),
        "GO-MCTS weights not found at {} (set EUCHRE_GOMCTS_WEIGHTS to override)",
        path.display()
    );
    let cfg = match env::var("EUCHRE_GOMCTS_CONFIG").as_deref() {
        Ok("smoke") => {
            TransformerConfig::euchre_smoke(EuchreTokenizer::VOCAB_SIZE, EuchreTokenizer::MAX_CONTEXT)
        }
        Ok("paper") => TransformerConfig::paper_default(
            EuchreTokenizer::VOCAB_SIZE,
            EuchreTokenizer::MAX_CONTEXT,
        ),
        _ => TransformerConfig::euchre_medium(
            EuchreTokenizer::VOCAB_SIZE,
            EuchreTokenizer::MAX_CONTEXT,
        ),
    };
    let mut net =
        GoMctsTransformerTch::new(cfg, tch::Device::cuda_if_available()).expect("build transformer");
    net.load_safetensors(&path).expect("load gomcts checkpoint");
    let lambda: f64 = env::var("EUCHRE_GOMCTS_LAMBDA")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.05);
    let (mode, lambda) = match env::var("EUCHRE_GOMCTS_INFER").as_deref() {
        Ok("lm") => (InferenceMode::LmSoftmax, 0.0),
        Ok("gated") => (InferenceMode::ArgmaxVal, lambda),
        _ => (InferenceMode::ArgmaxVal, 0.0),
    };
    let temp: f64 = env::var("EUCHRE_GOMCTS_TEMP")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.5);
    let model = TchInlineModel {
        net: std::sync::Arc::new(net),
        tokenizer: EuchreTokenizer,
        mode,
        lambda,
        temp,
    };
    let mcts_iter: usize = env::var("EUCHRE_GOMCTS_ITER")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(32);
    let rollout_steps: usize = env::var("EUCHRE_GOMCTS_ROLLOUT_STEPS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let parallel_sims: usize = env::var("EUCHRE_GOMCTS_PARALLEL_SIMS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let search = GoMcts::new(
        GoMctsConfig {
            uct_c: 0.4,
            n_iterations: mcts_iter,
            mu: 0.01,
            n_rollout_steps: rollout_steps,
            rollout_to_terminal: false,
            n_parallel_sims: parallel_sims,
        },
        model,
        StdRng::seed_from_u64(seed),
    );
    Box::new(search)
}

fn load_agent(name: &str, seed: u64) -> EuchreAgent {
    match name {
        "random" => Box::new(RandomAgent::new(seed)),
        "pimcts" => Box::new(PIMCTSBot::new(
            50,
            OpenHandSolver::new_euchre(),
            StdRng::seed_from_u64(seed),
        )),
        // EPIMC matches pimcts on rollouts + evaluator; only `depth` differs,
        // so the head-to-head isolates the postponing-reasoning contribution.
        "epimc2" => Box::new(EPIMCBot::new(
            50,
            2,
            OpenHandSolver::new_euchre(),
            StdRng::seed_from_u64(seed),
        )),
        "epimc3" => Box::new(EPIMCBot::new(
            50,
            3,
            OpenHandSolver::new_euchre(),
            StdRng::seed_from_u64(seed),
        )),
        "epimc4" => Box::new(EPIMCBot::new(
            50,
            4,
            OpenHandSolver::new_euchre(),
            StdRng::seed_from_u64(seed),
        )),
        "epimc5" => Box::new(EPIMCBot::new(
            50,
            5,
            OpenHandSolver::new_euchre(),
            StdRng::seed_from_u64(seed),
        )),
        "cfr0" => load_cfr("EUCHRE_CFR0_WEIGHTS", DEFAULT_CFR0, 0, seed),
        "cfr1" => load_cfr("EUCHRE_CFR1_WEIGHTS", DEFAULT_CFR1, 1, seed),
        "cfr2" => load_cfr("EUCHRE_CFR2_WEIGHTS", DEFAULT_CFR2, 2, seed),
        "cfr3" => load_cfr("EUCHRE_CFR3_WEIGHTS", DEFAULT_CFR3, 3, seed),
        "gomcts" => load_gomcts(seed),
        _ => panic!(
            "unknown agent: {name} (valid: random, pimcts, epimc2, epimc3, \
             epimc4, epimc5, cfr0, cfr1, cfr2, cfr3, gomcts)"
        ),
    }
}

fn deal(rng: &mut StdRng) -> EuchreGameState {
    let mut gs = Euchre::new_state();
    let mut actions = Vec::new();
    while gs.is_chance_node() {
        gs.legal_actions(&mut actions);
        let a = *actions.choose(rng).unwrap();
        gs.apply_action(a);
    }
    gs
}

/// Play one match to WIN_SCORE between agents A (seats 0+2 if
/// `a_on_team0`, else seats 1+3) and B. Returns the per-side match
/// outcome: `(a_match_won, a_pts, b_pts, hands_played)`.
fn play_match(
    a: &mut dyn Agent<EuchreGameState>,
    b: &mut dyn Agent<EuchreGameState>,
    a_on_team0: bool,
    deal_rng: &mut StdRng,
) -> (bool, i32, i32, usize) {
    let mut a_pts: i32 = 0;
    let mut b_pts: i32 = 0;
    let mut hands = 0;
    while a_pts < WIN_SCORE && b_pts < WIN_SCORE {
        let mut gs = deal(deal_rng);
        while !gs.is_terminal() {
            let seat = gs.cur_player();
            let team0 = seat == 0 || seat == 2;
            let acts_as_a = team0 == a_on_team0;
            let action = if acts_as_a { a.step(&gs) } else { b.step(&gs) };
            gs.apply_action(action);
        }
        let score0 = gs.evaluate(0) as i32;
        let team0_pts = score0.max(0);
        let team1_pts = (-score0).max(0);
        if a_on_team0 {
            a_pts += team0_pts;
            b_pts += team1_pts;
        } else {
            a_pts += team1_pts;
            b_pts += team0_pts;
        }
        hands += 1;
    }
    (a_pts >= WIN_SCORE, a_pts, b_pts, hands)
}

/// Per-pair running aggregate. Updated after each batch.
#[derive(Default)]
struct PairState {
    a_wins: usize,
    b_wins: usize,
    a_points: i64,
    b_points: i64,
    hands: usize,
    matches_played: usize,
    deal_rng: Option<StdRng>,
    /// Which side gets seats 0+2 next match. Toggled each match so seat
    /// bias washes out within (and across) batches.
    next_a_on_team0: bool,
    elapsed_secs: f64,
}

impl PairState {
    fn new(deal_seed: u64) -> Self {
        Self {
            deal_rng: Some(StdRng::seed_from_u64(deal_seed)),
            next_a_on_team0: true,
            ..Default::default()
        }
    }
    fn a_match_win_rate(&self) -> f64 {
        let total = (self.a_wins + self.b_wins) as f64;
        if total > 0.0 { self.a_wins as f64 / total } else { 0.0 }
    }
}

fn main() {
    let matches_per_pair: usize = parse_env("BENCH_MATCHES", 1000);
    let batch_pct: usize = parse_env("BENCH_BATCH_PCT", 5);
    let base_seed: u64 = parse_env("BENCH_SEED", 0);

    // Round up so we never silently skip the tail of the run. With a
    // 1000 match budget and 5% batches that's batch_size=50.
    let batch_size = ((matches_per_pair * batch_pct + 99) / 100).max(1);
    let n_batches = (matches_per_pair + batch_size - 1) / batch_size;

    let agents_csv = env::var("BENCH_AGENTS").unwrap_or_else(|_| {
        "random,pimcts,epimc2,epimc3,epimc4,epimc5,cfr0,cfr1,cfr2,cfr3".to_string()
    });
    let agent_names: Vec<String> = agents_csv
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    assert!(agent_names.len() >= 2, "need at least 2 agents to run a pairing");

    // Distinct unordered pairs, in the order the user reads top-to-bottom.
    // Pair k = (agent_names[i], agent_names[j]) for i<j.
    let mut pairings: Vec<(usize, usize)> = Vec::new();
    for i in 0..agent_names.len() {
        for j in i + 1..agent_names.len() {
            pairings.push((i, j));
        }
    }

    // Load each agent ONCE — hard's mmap+PHF load is ~10s and we'd pay
    // it per pairing otherwise. We then split-borrow the Vec on each pair
    // to get two simultaneous &mut to disjoint indices.
    println!(
        "Euchre tournament: {} agents, {} pairings, {} matches/pair, batch={} ({}%), to-{} pts",
        agent_names.len(),
        pairings.len(),
        matches_per_pair,
        batch_size,
        batch_pct,
        WIN_SCORE,
    );
    println!("Agents: {}", agent_names.join(", "));
    println!(
        "Rotation: {} batches of {} matches per pair, all pairings round-robin per batch",
        n_batches, batch_size,
    );
    println!();

    let mut agents: Vec<EuchreAgent> = agent_names
        .iter()
        .enumerate()
        .map(|(i, name)| load_agent(name, base_seed.wrapping_add(i as u64)))
        .collect();

    // One deal-RNG per pair so the deal stream for "random_vs_cfr0" is
    // independent of "pimcts_vs_cfr3". This matters because pairs run
    // interleaved — sharing a single RNG would otherwise let one pair's
    // batch eat deals the next pair was about to play.
    let mut states: Vec<PairState> = (0..pairings.len())
        .map(|k| PairState::new(base_seed.wrapping_add(1_000 + k as u64)))
        .collect();

    let tournament_start = Instant::now();

    for batch_idx in 0..n_batches {
        for (pair_k, &(i, j)) in pairings.iter().enumerate() {
            let a_name = &agent_names[i];
            let b_name = &agent_names[j];
            let metric = format!("{}_vs_{}", a_name, b_name);

            // Cap batch_size so the final batch doesn't overshoot the
            // per-pair match budget.
            let target = (batch_idx + 1) * batch_size;
            let target = target.min(matches_per_pair);
            let to_play = target - states[pair_k].matches_played;
            if to_play == 0 {
                continue;
            }

            // split_at_mut to get two disjoint &mut from the agents Vec.
            // i < j by construction.
            let (left, right) = agents.split_at_mut(j);
            let a = left[i].as_mut();
            let b = right[0].as_mut();

            let state = &mut states[pair_k];
            let mut deal_rng = state.deal_rng.take().expect("deal_rng missing");
            let batch_start = Instant::now();

            for _ in 0..to_play {
                let a_on_team0 = state.next_a_on_team0;
                state.next_a_on_team0 = !state.next_a_on_team0;
                let (a_won, a_pts, b_pts, h) =
                    play_match(a, b, a_on_team0, &mut deal_rng);
                if a_won {
                    state.a_wins += 1;
                } else {
                    state.b_wins += 1;
                }
                state.a_points += a_pts as i64;
                state.b_points += b_pts as i64;
                state.hands += h;
                state.matches_played += 1;
            }
            state.elapsed_secs += batch_start.elapsed().as_secs_f64();
            state.deal_rng = Some(deal_rng);

            // step = cumulative matches played for this pair. Metric name
            // is `{a}_vs_{b}` so kestrel shows one line per pairing.
            println!(
                "kestrel: step={} {}={:.6}",
                state.matches_played,
                metric,
                state.a_match_win_rate(),
            );
            // Human-readable progress line alongside the kestrel metric.
            println!(
                "  batch {:>2}/{} pair {:>2}/{} [{:>14}]  W-L {:>4}-{:<4}  pts {:>5}-{:<5}  hands={:<5}  +{} matches in {:.1}s (cum {:.1}s)",
                batch_idx + 1,
                n_batches,
                pair_k + 1,
                pairings.len(),
                metric,
                state.a_wins,
                state.b_wins,
                state.a_points,
                state.b_points,
                state.hands,
                to_play,
                batch_start.elapsed().as_secs_f64(),
                state.elapsed_secs,
            );
            // Flush stdout so the kestrel-tail process downstream sees
            // each metric line as it lands. Without this, stdout is
            // block-buffered when piped (8 KB default) and kestrel
            // wouldn't see anything until the buffer happens to fill —
            // which on a multi-hour run can be most of the run.
            let _ = std::io::stdout().flush();
        }
        println!(
            "--- end of batch {}/{} (tournament elapsed: {:.1}s) ---",
            batch_idx + 1,
            n_batches,
            tournament_start.elapsed().as_secs_f64(),
        );
    }

    println!();
    println!("=== final ===");
    println!(
        "{:>16}  {:>10}  {:>12}  {:>14}  {:>14}  {:>6}  {:>8}",
        "pair", "matches", "A match win%", "A points", "B points", "hands", "secs",
    );
    println!("{}", "-".repeat(96));
    for (pair_k, &(i, j)) in pairings.iter().enumerate() {
        let s = &states[pair_k];
        let total = (s.a_wins + s.b_wins) as f64;
        let a_win_pct = if total > 0.0 { 100.0 * s.a_wins as f64 / total } else { 0.0 };
        let total_pts = (s.a_points + s.b_points) as f64;
        let a_pt_pct = if total_pts > 0.0 {
            100.0 * s.a_points as f64 / total_pts
        } else {
            0.0
        };
        println!(
            "{:>16}  {:>4}-{:<5}  {:>11.1}%  {:>9} ({:>4.1}%)  {:>9} ({:>4.1}%)  {:>6}  {:>8.2}",
            format!("{}_vs_{}", agent_names[i], agent_names[j]),
            s.a_wins,
            s.b_wins,
            a_win_pct,
            s.a_points,
            a_pt_pct,
            s.b_points,
            100.0 - a_pt_pct,
            s.hands,
            s.elapsed_secs,
        );
    }
    println!(
        "Tournament total elapsed: {:.1}s",
        tournament_start.elapsed().as_secs_f64()
    );
}
