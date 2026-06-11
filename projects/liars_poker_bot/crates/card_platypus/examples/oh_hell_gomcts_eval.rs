//! Evaluate an Oh Hell GO-MCTS transformer checkpoint per trick count.
//!
//! The subject (transformer, raw policy — no search, per the Euchre
//! finding that search subtracts) rotates through every seat; the other
//! seats are filled by independent copies of the opponent agent. Since
//! `evaluate(p)` is mean-centred score, a mean > 0 against PIMCTS
//! opponents literally means "scores above the table average when the
//! rest of the table is PIMCTS" — i.e. beating it.
//!
//! Knobs:
//!   OH_WEIGHTS           safetensors path (default /home/steven/card_platypus/gomcts/oh_hell/bootstrap.safetensors)
//!   OH_CONFIG            smoke|medium|paper (default paper; must match training)
//!   OH_GAMES             games per trick count   (default 600)
//!   OH_OPPONENT          random | pimcts         (default pimcts)
//!   OH_PIMCTS_ROLLOUTS   opponent PIMCTS budget  (default 50)
//!   OH_INFER             lm | gated | argmax     (default lm)
//!   OH_TEMP              policy softmax temp     (default 0.05 ≈ greedy)
//!   OH_LAMBDA            λ gate for gated mode   (default 0.05)
//!   OH_PLAYERS           players                 (default 3)
//!   OH_MIN_TRICKS / OH_MAX_TRICKS                (default 1 / 10)
//!   OH_SUBJECT           model | pimcts (default model). pimcts =
//!                        calibration run: the subject seat is also
//!                        PIMCTS-50, giving the reference score for
//!                        each opponent/trick combination.
//!   OH_SEED              base RNG seed           (default 0)

use card_platypus::{
    agents::Agent,
    algorithms::{
        gomcts_transformer::{
            forward_histories_batch_tch, masked_policy, oh_hell::OhHellTokenizer,
            parse_env as parse, parse_env_path, GoMctsTransformerTch, InferenceMode, Tokenizer,
            TransformerConfig,
        },
        open_hand_solver::OpenHandSolver,
        pimcts::PIMCTSBot,
    },
};
use games::{
    gamestates::oh_hell::{OhHell, OhHellGameState},
    istate::IStateKey,
    Action, GameState,
};
use rand::{rngs::StdRng, seq::IndexedRandom, RngExt, SeedableRng};
use std::time::Instant;

struct InlineModel {
    net: GoMctsTransformerTch,
    tokenizer: OhHellTokenizer,
    mode: InferenceMode,
    lambda: f64,
    temp: f64,
}

impl InlineModel {
    fn policy(&self, history: &IStateKey, legal: &[Action]) -> Vec<f64> {
        masked_policy(
            self.mode,
            self.lambda,
            self.temp,
            |a| self.tokenizer.action_token(a),
            history,
            legal,
            |histories| forward_histories_batch_tch(&self.net, &self.tokenizer, &histories).ok(),
        )
    }

    fn act(&self, gs: &OhHellGameState, rng: &mut StdRng) -> Action {
        let mut legal = Vec::new();
        gs.legal_actions(&mut legal);
        if legal.len() == 1 {
            return legal[0];
        }
        let h = gs.istate_key(gs.cur_player());
        let probs = self.policy(&h, &legal);
        let mut r: f64 = rng.random::<f64>();
        for (i, p) in probs.iter().enumerate() {
            r -= *p;
            if r <= 0.0 {
                return legal[i];
            }
        }
        *legal.choose(rng).expect("non-empty legal")
    }
}

enum Opponent {
    Random,
    Pimcts(PIMCTSBot<OhHellGameState, OpenHandSolver<OhHellGameState>>),
}

impl Opponent {
    fn act(&mut self, gs: &OhHellGameState, rng: &mut StdRng) -> Action {
        match self {
            Opponent::Random => {
                let mut legal = Vec::new();
                gs.legal_actions(&mut legal);
                *legal.choose(rng).expect("non-empty legal")
            }
            Opponent::Pimcts(bot) => bot.step(gs),
        }
    }
}

fn main() {
    let weights = parse_env_path(
        "OH_WEIGHTS",
        "/home/steven/card_platypus/gomcts/oh_hell/bootstrap.safetensors",
    );
    let n_games: usize = parse("OH_GAMES", 600);
    let opponent_kind = std::env::var("OH_OPPONENT").unwrap_or_else(|_| "pimcts".into());
    let rollouts: usize = parse("OH_PIMCTS_ROLLOUTS", 50);
    let temp: f64 = parse("OH_TEMP", 0.05);
    let lambda: f64 = parse("OH_LAMBDA", 0.05);
    let num_players: usize = parse("OH_PLAYERS", 3);
    let min_tricks: usize = parse("OH_MIN_TRICKS", 1);
    let max_tricks: usize = parse("OH_MAX_TRICKS", 10);
    let base_seed: u64 = parse("OH_SEED", 0);
    let subject_kind = std::env::var("OH_SUBJECT").unwrap_or_else(|_| "model".into());
    let (mode, lambda) = match std::env::var("OH_INFER").as_deref() {
        Ok("gated") => (InferenceMode::ArgmaxVal, lambda),
        Ok("argmax") => (InferenceMode::ArgmaxVal, 0.0),
        _ => (InferenceMode::LmSoftmax, 0.0),
    };
    assert!(weights.exists(), "weights not found at {}", weights.display());

    let cfg = TransformerConfig::from_env(
        "OH_CONFIG",
        "paper",
        OhHellTokenizer::VOCAB_SIZE,
        OhHellTokenizer::MAX_CONTEXT,
    );
    let mut net =
        GoMctsTransformerTch::new(cfg, tch::Device::cuda_if_available()).expect("build");
    net.load_safetensors(&weights).expect("load weights");
    let model = InlineModel { net, tokenizer: OhHellTokenizer, mode, lambda, temp };

    println!(
        "OhHell gomcts eval: subject={subject_kind}, weights={}, opponent={}, games/trick={}, infer={:?} λ={} t={}, \
         players={}, tricks {}..={}",
        weights.display(),
        opponent_kind,
        n_games,
        mode,
        lambda,
        temp,
        num_players,
        min_tricks,
        max_tricks,
    );
    println!(
        "{:>7}  {:>10}  {:>8}  {:>8}  {:>8}",
        "tricks", "mean", "se", "win%", "secs"
    );

    let mut grand_sum = 0.0;
    let mut grand_n = 0usize;
    for n_tricks in min_tricks..=max_tricks {
        let t0 = Instant::now();
        let mut scores: Vec<f64> = Vec::with_capacity(n_games);
        let mut wins = 0usize;
        for game_idx in 0..n_games {
            let seed = base_seed
                .wrapping_add(n_tricks as u64 * 1_000_000)
                .wrapping_add(game_idx as u64);
            let mut rng: StdRng = SeedableRng::seed_from_u64(seed);
            let subject_seat = game_idx % num_players;
            let mut opponents: Vec<Opponent> = (0..num_players)
                .map(|i| {
                    if opponent_kind == "random" {
                        Opponent::Random
                    } else {
                        Opponent::Pimcts(PIMCTSBot::new(
                            rollouts,
                            OpenHandSolver::new_oh_hell(),
                            StdRng::seed_from_u64(seed.wrapping_add(50 + i as u64)),
                        ))
                    }
                })
                .collect();
            let mut gs = OhHell::new_state(num_players, n_tricks);
            let mut buf = Vec::new();
            while gs.is_chance_node() {
                buf.clear();
                gs.legal_actions(&mut buf);
                let a = *buf.choose(&mut rng).expect("chance");
                gs.apply_action(a);
            }
            let mut subject_pimcts = if subject_kind == "pimcts" {
                Some(PIMCTSBot::new(
                    rollouts,
                    OpenHandSolver::new_oh_hell(),
                    StdRng::seed_from_u64(seed.wrapping_add(99)),
                ))
            } else {
                None
            };
            while !gs.is_terminal() {
                let p = gs.cur_player();
                let a = if p == subject_seat {
                    match subject_pimcts.as_mut() {
                        Some(bot) => bot.step(&gs),
                        None => model.act(&gs, &mut rng),
                    }
                } else {
                    opponents[p].act(&gs, &mut rng)
                };
                gs.apply_action(a);
            }
            let s = gs.evaluate(subject_seat);
            if s > 0.0 {
                wins += 1;
            }
            scores.push(s);
        }
        let n = scores.len() as f64;
        let mean = scores.iter().sum::<f64>() / n;
        let var = scores.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n;
        let se = (var / n).sqrt();
        let secs = t0.elapsed().as_secs_f64();
        grand_sum += scores.iter().sum::<f64>();
        grand_n += scores.len();
        println!(
            "{:>7}  {:>+10.4}  {:>8.4}  {:>7.1}%  {:>8.1}",
            n_tricks,
            mean,
            se,
            100.0 * wins as f64 / n,
            secs
        );
        println!(
            "kestrel: step={} opponent={} mean={:.6} se={:.6} win_rate={:.6} n_games={} secs={:.2}",
            n_tricks,
            opponent_kind,
            mean,
            se,
            wins as f64 / n,
            n_games,
            secs
        );
    }
    println!(
        "\noverall mean (all trick counts pooled): {:+.4} over {} games",
        grand_sum / grand_n as f64,
        grand_n
    );
}
