//! Head-to-head benchmark of the three Euchre difficulty tiers used in
//! the online deployment (euchre_server):
//!
//!   * easy    — PIMCTS (OpenHandSolver, 50 rollouts), no trained weights
//!   * medium  — CFRES trained on bidding only (max_cards_played = 0)
//!   * hard    — CFRES trained through 3 cards played (max_cards_played = 3)
//!
//! Each pairing plays matches to WIN_SCORE (10) points, alternating which
//! agent controls seats (0, 2) vs (1, 3) every match to remove seat bias.
//! Reports per-pair match win rate, point win rate, and total hands.
//!
//! Weight paths match euchre_server (env vars override the defaults):
//!   EUCHRE_MEDIUM_WEIGHTS_PATH  default: /home/steven/card_platypus/infostate.baseline
//!   EUCHRE_HARD_WEIGHTS_PATH    default: /home/steven/card_platypus/infostate.three_card_played_f32
//!
//! Knobs:
//!   BENCH_MATCHES  matches per pairing (default 20)
//!   BENCH_SEED     base RNG seed (default 0)
//!
//! Run:
//!   cargo run -p card_platypus --release --example euchre_difficulty_benchmark

use std::{env, path::PathBuf, time::Instant};

use card_platypus::{
    agents::Agent,
    algorithms::{cfres::CFRES, open_hand_solver::OpenHandSolver, pimcts::PIMCTSBot},
};
use games::{
    gamestates::euchre::{Euchre, EuchreGameState},
    GameState,
};
use rand::{rngs::StdRng, seq::IndexedRandom, SeedableRng};

const WIN_SCORE: i32 = 10;
const MEDIUM_WEIGHT_PATH: &str = "/home/steven/card_platypus/infostate.baseline";
const HARD_WEIGHT_PATH: &str = "/home/steven/card_platypus/infostate.three_card_played_f32";

type EuchreAgent = Box<dyn Agent<EuchreGameState>>;

fn parse_env<T: std::str::FromStr>(name: &str, default: T) -> T {
    env::var(name).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}

fn load_agent(name: &str, seed: u64) -> EuchreAgent {
    match name {
        "easy" => Box::new(PIMCTSBot::new(
            50,
            OpenHandSolver::new_euchre(),
            StdRng::seed_from_u64(seed),
        )),
        "medium" => {
            let path: PathBuf = env::var("EUCHRE_MEDIUM_WEIGHTS_PATH")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from(MEDIUM_WEIGHT_PATH));
            assert!(
                path.exists(),
                "medium weights not found at {} (set EUCHRE_MEDIUM_WEIGHTS_PATH)",
                path.display()
            );
            Box::new(CFRES::new_euchre(StdRng::seed_from_u64(seed), 0, Some(&path)))
        }
        "hard" => {
            let path: PathBuf = env::var("EUCHRE_HARD_WEIGHTS_PATH")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from(HARD_WEIGHT_PATH));
            assert!(
                path.exists(),
                "hard weights not found at {} (set EUCHRE_HARD_WEIGHTS_PATH)",
                path.display()
            );
            Box::new(CFRES::new_euchre(StdRng::seed_from_u64(seed), 3, Some(&path)))
        }
        _ => panic!("unknown agent: {name}"),
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

struct PairResult {
    a_name: String,
    b_name: String,
    a_match_wins: usize,
    b_match_wins: usize,
    a_points: i32,
    b_points: i32,
    hands: usize,
    elapsed_secs: f64,
}

/// Play one match to WIN_SCORE between two agents. `a_on_team0` true means
/// agent A controls seats 0+2; false means seats 1+3. Returns
/// (a_points, b_points, hands_played).
fn play_match(
    a: &mut EuchreAgent,
    b: &mut EuchreAgent,
    a_on_team0: bool,
    deal_rng: &mut StdRng,
) -> (i32, i32, usize) {
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
        // evaluate(0) > 0 iff team (0,2) won this hand; magnitude is the points.
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
    (a_pts, b_pts, hands)
}

fn run_pair(
    a_name: &str,
    b_name: &str,
    n_matches: usize,
    base_seed: u64,
) -> PairResult {
    let mut a = load_agent(a_name, base_seed);
    let mut b = load_agent(b_name, base_seed.wrapping_add(1));
    let mut deal_rng = StdRng::seed_from_u64(base_seed.wrapping_add(2));

    let mut a_match_wins = 0;
    let mut b_match_wins = 0;
    let mut a_points: i32 = 0;
    let mut b_points: i32 = 0;
    let mut hands = 0;

    let start = Instant::now();
    for i in 0..n_matches {
        let a_on_team0 = i % 2 == 0;
        let match_start = Instant::now();
        let (a_pts, b_pts, h) = play_match(&mut a, &mut b, a_on_team0, &mut deal_rng);
        let match_secs = match_start.elapsed().as_secs_f64();
        if a_pts >= WIN_SCORE {
            a_match_wins += 1;
        } else {
            b_match_wins += 1;
        }
        a_points += a_pts;
        b_points += b_pts;
        hands += h;
        println!(
            "  match {:>3}: {:>6} (seats {}) {} - {} {:>6} (seats {})  hands={}  secs={:.2}",
            i + 1,
            a_name,
            if a_on_team0 { "0,2" } else { "1,3" },
            a_pts,
            b_pts,
            b_name,
            if a_on_team0 { "1,3" } else { "0,2" },
            h,
            match_secs,
        );
    }
    let elapsed_secs = start.elapsed().as_secs_f64();

    PairResult {
        a_name: a_name.to_string(),
        b_name: b_name.to_string(),
        a_match_wins,
        b_match_wins,
        a_points,
        b_points,
        hands,
        elapsed_secs,
    }
}

fn print_summary(results: &[PairResult]) {
    println!();
    println!(
        "{:>8} vs {:<8}  {:>10}  {:>12}  {:>14}  {:>14}  {:>6}  {:>8}",
        "A", "B", "matches", "A match win%", "A points", "B points", "hands", "secs",
    );
    println!("{}", "-".repeat(96));
    for r in results {
        let total = (r.a_match_wins + r.b_match_wins) as f64;
        let a_win_pct = if total > 0.0 {
            100.0 * r.a_match_wins as f64 / total
        } else {
            0.0
        };
        let a_pt_pct = if r.a_points + r.b_points > 0 {
            100.0 * r.a_points as f64 / (r.a_points + r.b_points) as f64
        } else {
            0.0
        };
        println!(
            "{:>8} vs {:<8}  {:>4}-{:<5}  {:>11.1}%  {:>9} ({:>4.1}%)  {:>9} ({:>4.1}%)  {:>6}  {:>8.2}",
            r.a_name,
            r.b_name,
            r.a_match_wins,
            r.b_match_wins,
            a_win_pct,
            r.a_points,
            a_pt_pct,
            r.b_points,
            100.0 - a_pt_pct,
            r.hands,
            r.elapsed_secs,
        );
    }
}

fn main() {
    let n_matches: usize = parse_env("BENCH_MATCHES", 20);
    let base_seed: u64 = parse_env("BENCH_SEED", 0);

    println!(
        "Euchre difficulty benchmark: easy/medium/hard, {} matches per pairing, to-{} points",
        n_matches, WIN_SCORE
    );
    println!("(seats alternate every match; deals shared across matches via shared RNG)");
    println!();

    let pairings = [("easy", "medium"), ("easy", "hard"), ("medium", "hard")];

    let mut results = Vec::new();
    for (i, (a, b)) in pairings.iter().enumerate() {
        println!("=== {} vs {} (pair_id={}) ===", a, b, i);
        let seed = base_seed.wrapping_add((i as u64) * 100);
        let r = run_pair(a, b, n_matches, seed);
        // Emit each pair's kestrel line immediately on completion so signal
        // shows up in the dashboard as work progresses, instead of waiting
        // for all three pairings to finish (which is ~hours at 1k matches).
        let total = (r.a_match_wins + r.b_match_wins) as f64;
        let a_match_win_pct = if total > 0.0 { r.a_match_wins as f64 / total } else { 0.0 };
        let total_pts = (r.a_points + r.b_points) as f64;
        let a_point_win_pct = if total_pts > 0.0 {
            r.a_points as f64 / total_pts
        } else {
            0.0
        };
        println!(
            "kestrel: step={} pair_id={} a_match_win_pct={:.6} \
             a_point_win_pct={:.6} a_match_wins={} b_match_wins={} \
             a_points={} b_points={} hands={} elapsed_secs={:.4} matches={}",
            i,
            i,
            a_match_win_pct,
            a_point_win_pct,
            r.a_match_wins,
            r.b_match_wins,
            r.a_points,
            r.b_points,
            r.hands,
            r.elapsed_secs,
            n_matches,
        );
        results.push(r);
        println!();
    }

    print_summary(&results);
}
