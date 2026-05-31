//! Cross-comparison of trained Oh Hell policies vs random and head-to-head.
//!
//! For each `(num_players, n_tricks)` config we evaluate three agent
//! types:
//!
//!   * **PIMCTS**       — `PIMCTSBot` with `OpenHandSolver`. No CFR
//!                        training; this is the agent CFRES delegates
//!                        to past the depth cutoff.
//!   * **CFR max=0**    — `CFRES::new_oh_hell_mmap(..., 0, ...)`, the
//!                        bidding-only trained policy.
//!   * **CFR max=1**    — `CFRES::new_oh_hell_mmap(..., 1, ...)`, the
//!                        bidding + first-play-decision trained policy.
//!
//! For 1-trick games `max=0` and `max=1` collapse to the same policy
//! (the only play decision is forced), so we report that combined.
//!
//! Each agent is benchmarked against three opponents in turn:
//!
//!   * vs random  — same setup as the training-time eval.
//!   * vs PIMCTS
//!   * vs CFR max=0
//!   * vs CFR max=1
//!
//! The focal agent rotates through every seat over the N games to
//! remove seat bias. Opponent agents fill the remaining seats with
//! independent copies (so e.g. for 3-player "vs PIMCTS" both
//! non-focal seats are run by their own PIMCTSBot).
//!
//! Defaults: `N_VS_RANDOM=400`, `N_HEAD_TO_HEAD=200`. Override with
//! `BENCH_RANDOM_GAMES` and `BENCH_HEAD_GAMES`.

use std::path::PathBuf;

use card_platypus::{
    agents::{Agent, RandomAgent},
    algorithms::{
        cfres::{self, CFRES, OH_MAX_ACTIONS},
        open_hand_solver::OpenHandSolver,
        pimcts::PIMCTSBot,
    },
};
use games::{
    gamestates::oh_hell::{OhHell, OhHellGameState},
    GameState,
};
use rand::{rngs::StdRng, seq::IndexedRandom, SeedableRng};

type OhCfres = CFRES<OhHellGameState, OH_MAX_ACTIONS>;

fn parse_env<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn pimcts_bot(seed: u64) -> PIMCTSBot<OhHellGameState, OpenHandSolver<OhHellGameState>> {
    PIMCTSBot::new(
        50,
        OpenHandSolver::default(),
        StdRng::seed_from_u64(seed),
    )
}

/// Try to load a CFR policy from disk for `(num_players, n_tricks)`
/// at the given `max_cards_played` depth. Returns `None` if the
/// checkpoint directory doesn't exist.
fn try_load_cfr(
    num_players: usize,
    n_tricks: usize,
    max_cards_played: usize,
) -> Option<OhCfres> {
    let cache = std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join("cache/oh_cfr"))?;
    let mmap_dir = cache.join(format!(
        "{}p_{}t_max{}",
        num_players, n_tricks, max_cards_played
    ));
    if !(mmap_dir.join("indexer").exists() && mmap_dir.join("mmap").exists()) {
        return None;
    }
    Some(CFRES::new_oh_hell(
        num_players,
        n_tricks,
        max_cards_played,
        Some(mmap_dir.as_path()),
    ))
}

#[derive(Clone, Copy, Debug)]
struct EvalSummary {
    avg_score: f64,
    win_rate: f64,
    tie_rate: f64,
    loss_rate: f64,
    n_games: usize,
}

/// Play `n_games` of OH between `focal` (rotating through every seat)
/// and `opponents` (independent agents filling the other seats — one
/// per opponent seat). Returns focal's score / win-rate / etc.
fn play_matchup(
    focal: &mut dyn Agent<OhHellGameState>,
    opponents: &mut [Box<dyn Agent<OhHellGameState>>],
    num_players: usize,
    n_tricks: usize,
    n_games: usize,
    seed_offset: u64,
) -> EvalSummary {
    debug_assert_eq!(opponents.len(), num_players - 1);

    let mut total_score = 0.0_f64;
    let mut wins = 0usize;
    let mut ties = 0usize;
    let mut losses = 0usize;
    let mut acts = Vec::new();

    for i in 0..n_games {
        let seed = seed_offset.wrapping_add((i as u64).wrapping_mul(0x9E3779B97F4A7C15));
        let mut chance_rng: StdRng = SeedableRng::seed_from_u64(seed);
        let focal_pos = i % num_players;

        let mut gs = OhHell::new_state(num_players, n_tricks);
        while !gs.is_terminal() {
            if gs.is_chance_node() {
                gs.legal_actions(&mut acts);
                gs.apply_action(*acts.choose(&mut chance_rng).unwrap());
                continue;
            }
            let cp = gs.cur_player();
            let action = if cp == focal_pos {
                focal.step(&gs)
            } else {
                let opp_idx = if cp < focal_pos { cp } else { cp - 1 };
                opponents[opp_idx].step(&gs)
            };
            gs.apply_action(action);
        }
        let focal_score = gs.evaluate(focal_pos);
        total_score += focal_score;
        let max_other = (0..num_players)
            .filter(|p| *p != focal_pos)
            .map(|p| gs.evaluate(p))
            .fold(f64::NEG_INFINITY, f64::max);
        if focal_score > max_other {
            wins += 1;
        } else if (focal_score - max_other).abs() < 1e-9 {
            ties += 1;
        } else {
            losses += 1;
        }
    }

    let g = n_games as f64;
    EvalSummary {
        avg_score: total_score / g,
        win_rate: wins as f64 / g,
        tie_rate: ties as f64 / g,
        loss_rate: losses as f64 / g,
        n_games,
    }
}

/// Make `num_opps` independent random agents.
fn random_opponents(num_opps: usize) -> Vec<Box<dyn Agent<OhHellGameState>>> {
    (0..num_opps)
        .map(|_| Box::new(RandomAgent::default()) as Box<dyn Agent<OhHellGameState>>)
        .collect()
}

/// Make `num_opps` independent PIMCTS opponents, seeded distinctly.
fn pimcts_opponents(
    num_opps: usize,
    seed: u64,
) -> Vec<Box<dyn Agent<OhHellGameState>>> {
    (0..num_opps)
        .map(|i| {
            Box::new(pimcts_bot(seed.wrapping_add(i as u64 + 1)))
                as Box<dyn Agent<OhHellGameState>>
        })
        .collect()
}

/// Make `num_opps` independent CFR opponents loaded from the same
/// checkpoint. Returns `None` if the checkpoint isn't available.
fn cfr_opponents(
    num_opps: usize,
    num_players: usize,
    n_tricks: usize,
    max_cards_played: usize,
) -> Option<Vec<Box<dyn Agent<OhHellGameState>>>> {
    let mut out = Vec::with_capacity(num_opps);
    for _ in 0..num_opps {
        out.push(Box::new(try_load_cfr(num_players, n_tricks, max_cards_played)?)
            as Box<dyn Agent<OhHellGameState>>);
    }
    Some(out)
}

#[derive(Clone, Copy, Debug)]
enum AgentKind {
    Pimcts,
    CfrMax0,
    CfrMax1,
}

impl AgentKind {
    fn label(&self) -> &'static str {
        match self {
            AgentKind::Pimcts => "PIMCTS",
            AgentKind::CfrMax0 => "CFR max=0",
            AgentKind::CfrMax1 => "CFR max=1",
        }
    }
}

/// Build the focal agent for a given kind. Returns `None` if the
/// underlying checkpoint isn't loadable.
fn build_focal(
    kind: AgentKind,
    num_players: usize,
    n_tricks: usize,
    seed: u64,
) -> Option<Box<dyn Agent<OhHellGameState>>> {
    match kind {
        AgentKind::Pimcts => Some(Box::new(pimcts_bot(seed))),
        AgentKind::CfrMax0 => Some(Box::new(try_load_cfr(num_players, n_tricks, 0)?)),
        AgentKind::CfrMax1 => Some(Box::new(try_load_cfr(num_players, n_tricks, 1)?)),
    }
}

fn fmt_summary(s: &EvalSummary) -> String {
    format!(
        "{:+5.2} W:{:>4.1}% T:{:>4.1}% L:{:>4.1}%",
        s.avg_score,
        100.0 * s.win_rate,
        100.0 * s.tie_rate,
        100.0 * s.loss_rate,
    )
}

fn benchmark_config(num_players: usize, n_tricks: usize, n_random: usize, n_head: usize) {
    println!();
    println!(
        "==== {} players × {} trick(s) ====",
        num_players, n_tricks
    );

    // Decide which agent kinds are available for this config.
    let mut kinds = vec![AgentKind::Pimcts, AgentKind::CfrMax0];
    // For 1-trick max=0 and max=1 produce identical policy (only play
    // decision is forced) — only include max=1 row when it's
    // *separately* trained.
    if n_tricks >= 2 && try_load_cfr(num_players, n_tricks, 1).is_some() {
        kinds.push(AgentKind::CfrMax1);
    }

    // Verify each kind's checkpoint is reachable.
    kinds.retain(|k| build_focal(*k, num_players, n_tricks, 0xC0FFEE).is_some());
    if kinds.is_empty() {
        println!("  (no checkpoints available)");
        return;
    }

    // Header.
    let col_w = 28;
    print!("  {:<12} | {:<col_w$}", "agent", "vs random", col_w = col_w);
    for k in &kinds {
        print!(" | {:<col_w$}", format!("vs {}", k.label()), col_w = col_w);
    }
    println!();
    println!(
        "  {}",
        "-".repeat(12 + 3 + (kinds.len() + 1) * (col_w + 3))
    );

    for (row_idx, focal_kind) in kinds.iter().enumerate() {
        let mut row = format!("  {:<12} |", focal_kind.label());

        // vs random.
        let mut focal = build_focal(*focal_kind, num_players, n_tricks, 0xC0FFEE)
            .expect("focal");
        let mut opps = random_opponents(num_players - 1);
        let summary = play_matchup(
            focal.as_mut(),
            &mut opps,
            num_players,
            n_tricks,
            n_random,
            0x1111 + row_idx as u64,
        );
        row.push_str(&format!(" {:<28} |", fmt_summary(&summary)));

        // vs each agent.
        for (col_idx, opp_kind) in kinds.iter().enumerate() {
            if std::mem::discriminant(focal_kind) == std::mem::discriminant(opp_kind) {
                row.push_str(&format!(" {:<28} |", "-"));
                continue;
            }
            let mut focal = build_focal(*focal_kind, num_players, n_tricks, 0xBEEF)
                .expect("focal rebuild");
            let mut opps: Vec<Box<dyn Agent<OhHellGameState>>> = match opp_kind {
                AgentKind::Pimcts => pimcts_opponents(num_players - 1, 0xCAFE),
                AgentKind::CfrMax0 => cfr_opponents(num_players - 1, num_players, n_tricks, 0)
                    .expect("opp CFR max=0 missing"),
                AgentKind::CfrMax1 => cfr_opponents(num_players - 1, num_players, n_tricks, 1)
                    .expect("opp CFR max=1 missing"),
            };
            let summary = play_matchup(
                focal.as_mut(),
                &mut opps,
                num_players,
                n_tricks,
                n_head,
                0x2222 + ((row_idx * 16 + col_idx) as u64),
            );
            row.push_str(&format!(" {:<28} |", fmt_summary(&summary)));
        }

        println!("{}", row);
    }
}

fn main() {
    cfres::feature::enable(cfres::feature::LinearCFR);
    cfres::feature::disable(cfres::feature::SingleThread);

    let n_random = parse_env("BENCH_RANDOM_GAMES", 400);
    let n_head = parse_env("BENCH_HEAD_GAMES", 200);

    println!(
        "Oh Hell agent benchmark — random eval: {} games; head-to-head: {} games per pair",
        n_random, n_head
    );
    println!("(focal agent rotates through every seat; opponent agents are independent instances)");

    let configs = [
        (2, 1),
        (2, 2),
        (2, 3),
        (3, 1),
        (3, 2),
        (3, 3),
    ];

    for (np, nt) in configs {
        benchmark_config(np, nt, n_random, n_head);
    }
}
