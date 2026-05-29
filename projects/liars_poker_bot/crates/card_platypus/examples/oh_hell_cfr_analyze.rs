//! Inspect a trained 1-trick Oh Hell CFR policy and summarise the
//! bidding/play strategy by perspective.
//!
//! Loads weights from `CFR_LOAD_PATH` (the file produced by the
//! `oh_hell_cfr_train` example when `CFR_SAVE_PATH` is set), constructs
//! representative game states by replaying chance + bid actions, and
//! queries `CFRES::action_probabilities` to read the trained policy.
//!
//! Defaults to the 2-player 1-trick saved weights but accepts
//! `CFR_PLAYERS={2,3}` to switch.

use std::path::PathBuf;

use card_platypus::{
    algorithms::{
        cfres::{self, CFRES, OH_MAX_ACTIONS},
    },
    policy::Policy,
};
use games::{
    gamestates::oh_hell::{
        actions::{OHAction, OHCard, OH_DECK},
        OhHell, OhHellGameState,
    },
    Action, GameState,
};

type OhCfres = CFRES<OhHellGameState, OH_MAX_ACTIONS>;

fn parse_env<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn main() {
    cfres::feature::enable(cfres::feature::LinearCFR);
    cfres::feature::disable(cfres::feature::SingleThread);

    let n_players: usize = parse_env("CFR_PLAYERS", 2);
    let n_tricks: usize = parse_env("CFR_TRICKS", 1);
    let max_cards: usize = parse_env("CFR_MAX_CARDS", 100);
    let load_path: PathBuf = std::env::var("CFR_LOAD_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(format!("/tmp/oh_cfr_{}p_{}t.msgpack", n_players, n_tricks))
        });

    if !load_path.exists() {
        eprintln!(
            "no checkpoint at {} — train first with CFR_PLAYERS={} CFR_TRICKS={} CFR_SAVE_PATH={}",
            load_path.display(),
            n_players,
            n_tricks,
            load_path.display()
        );
        std::process::exit(1);
    }

    println!(
        "Loading {} weights for {} players × {} trick(s)…",
        load_path.display(),
        n_players,
        n_tricks
    );
    let mut cfr: OhCfres = CFRES::new_oh_hell(n_players, n_tricks, max_cards, Some(load_path.as_path()));
    println!("info_states loaded: {}", cfr.num_info_states());
    println!();

    analyze_p0_open_bid(&mut cfr, n_players, n_tricks);
    println!();
    analyze_followers(&mut cfr, n_players, n_tricks);
    println!();
    analyze_play(&mut cfr, n_players, n_tricks);
}

/// Aggregate bid probabilities at the P0 opening-bid decision over every
/// distinct (own-card iso-class, face-up rank) combination.
fn analyze_p0_open_bid(cfr: &mut OhCfres, n_players: usize, n_tricks: usize) {
    println!("==== P0 opening bid (2p 1-trick: bid 0 = predict you lose, bid 1 = predict you win) ====");
    println!(
        "{:>14} {:>10} {:>10} {:>10}",
        "own_card", "face_up", "p(bid 0)", "p(bid 1)"
    );

    // Enumerate distinct (own_card, face_up) pairs. With iso reduction
    // many will collapse to the same policy; we deduplicate later.
    let mut rows: Vec<(OHCard, OHCard, f64, f64)> = Vec::new();
    for &face_up in OH_DECK.iter() {
        for &own in OH_DECK.iter() {
            if own == face_up {
                continue;
            }
            let (b0, b1) = bid_probs_at_open(cfr, n_players, n_tricks, own, face_up);
            rows.push((own, face_up, b0, b1));
        }
    }

    // Print representative rows: bucket by (own_role, own_rank, face_up_rank)
    // where own_role = "Trump" if own.suit() == face_up.suit() else "Off".
    print_bid_buckets(&rows);
}

fn bid_probs_at_open(
    cfr: &mut OhCfres,
    n_players: usize,
    n_tricks: usize,
    own_card: OHCard,
    face_up: OHCard,
) -> (f64, f64) {
    let gs = make_bidding_state(n_players, n_tricks, own_card, face_up, &[]);
    if gs.cur_player() != 0 {
        return (f64::NAN, f64::NAN);
    }
    let policy = cfr.action_probabilities(&gs);
    let bid0: Action = OHAction::Bid(0).into();
    let bid1: Action = OHAction::Bid(1).into();
    (policy[bid0], policy[bid1])
}

/// Enumerate the bidder-2 / bidder-3 decisions for various (own_card,
/// face_up, prior_bids) combinations and summarise.
fn analyze_followers(cfr: &mut OhCfres, n_players: usize, n_tricks: usize) {
    for player in 1..n_players {
        println!(
            "==== P{} bid (after {} prior bid(s)) ====",
            player, player
        );
        // Enumerate prior bid configurations (each prior bidder bid 0 or 1).
        let n_prior = player;
        let total_configs = 1usize << n_prior;
        for cfg in 0..total_configs {
            let prior_bids: Vec<u8> = (0..n_prior)
                .map(|i| ((cfg >> i) & 1) as u8)
                .collect();
            println!(
                "  prior_bids = {:?}  (P0..P{}-1 bids)",
                prior_bids, player
            );
            let mut rows: Vec<(OHCard, OHCard, f64, f64)> = Vec::new();
            for &face_up in OH_DECK.iter() {
                for &own in OH_DECK.iter() {
                    if own == face_up {
                        continue;
                    }
                    let (b0, b1) = bid_probs_at_player(
                        cfr,
                        n_players,
                        n_tricks,
                        own,
                        face_up,
                        &prior_bids,
                        player,
                    );
                    if b0.is_nan() {
                        continue;
                    }
                    rows.push((own, face_up, b0, b1));
                }
            }
            print_bid_buckets(&rows);
            println!();
        }
    }
}

fn bid_probs_at_player(
    cfr: &mut OhCfres,
    n_players: usize,
    n_tricks: usize,
    own_card: OHCard,
    face_up: OHCard,
    prior_bids: &[u8],
    target_player: usize,
) -> (f64, f64) {
    let gs = make_bidding_state(n_players, n_tricks, own_card, face_up, prior_bids);
    if gs.cur_player() != target_player {
        return (f64::NAN, f64::NAN);
    }
    let policy = cfr.action_probabilities(&gs);
    let bid0: Action = OHAction::Bid(0).into();
    let bid1: Action = OHAction::Bid(1).into();
    (policy[bid0], policy[bid1])
}

/// 1-trick play is forced (each player has exactly 1 card) but the
/// CFR action probabilities are still useful as a sanity check that
/// the trained policy plays the single legal card with probability 1.
fn analyze_play(cfr: &mut OhCfres, n_players: usize, n_tricks: usize) {
    println!("==== Play phase (1-trick: each player has 1 forced card) ====");
    if n_tricks > 1 {
        println!("  skipped — play analysis only makes sense for 1-trick games");
        return;
    }
    // Sample one playable state for P0 and confirm it plays its forced
    // card. Doing this for every iso class is redundant.
    let own = OHCard::AS;
    let face_up = OHCard::TS;
    let gs = make_bidding_state(n_players, n_tricks, own, face_up, &vec![0u8; n_players]);
    // Now in Play phase, P0 to lead.
    if gs.cur_player() != 0 {
        println!("  unexpected cur_player after bidding");
        return;
    }
    let policy = cfr.action_probabilities(&gs);
    let act: Action = OHAction::Card(own).into();
    let p = policy[act];
    println!(
        "  P0 holding {:?} with face_up {:?} after all-zero bids → p(play {:?}) = {:.6}",
        own, face_up, own, p
    );
}

/// Build a state where:
///   * cards are dealt one to each player in order (P0 gets `own_card`,
///     the rest get arbitrary distinct dummy cards)
///   * face-up is dealt
///   * the first `prior_bids.len()` players have bid as specified
///
/// The dummy cards are intentionally chosen *not* to affect the
/// perspective player's istate (the iso normaliser collapses suit
/// labelling, and the player can't see opponents' specific cards
/// anyway).
fn make_bidding_state(
    n_players: usize,
    n_tricks: usize,
    own_card: OHCard,
    face_up: OHCard,
    prior_bids: &[u8],
) -> OhHellGameState {
    let mut gs = OhHell::new_state(n_players, n_tricks);
    // Pick dummy cards for the other players that aren't own_card or face_up.
    let mut dummies: Vec<OHCard> = OH_DECK
        .iter()
        .copied()
        .filter(|c| *c != own_card && *c != face_up)
        .collect();
    // Deal: P0 first, then P1, P2... All players need n_tricks cards.
    let mut deal_cards: Vec<OHCard> = Vec::with_capacity(n_players * n_tricks);
    for t in 0..n_tricks {
        for p in 0..n_players {
            if t == 0 && p == 0 {
                deal_cards.push(own_card);
            } else {
                deal_cards.push(dummies.pop().expect("enough dummies"));
            }
        }
    }
    for c in deal_cards {
        gs.apply_action(OHAction::Card(c).into());
    }
    gs.apply_action(OHAction::Card(face_up).into());
    for &b in prior_bids {
        gs.apply_action(OHAction::Bid(b).into());
    }
    gs
}

/// Bucket the (own_card, face_up_card, p0, p1) rows by:
///   own_role (Trump / Off-trump), own_rank, face_up_rank
/// and print one representative line per bucket alongside the
/// frequency of bid-1.
fn print_bid_buckets(rows: &[(OHCard, OHCard, f64, f64)]) {
    use std::collections::BTreeMap;

    #[derive(PartialEq, Eq, PartialOrd, Ord)]
    struct Bucket {
        own_role: &'static str,
        own_rank: u8,
        face_up_rank: u8,
    }

    let mut agg: BTreeMap<Bucket, Vec<(f64, f64)>> = BTreeMap::new();
    for &(own, face_up, b0, b1) in rows {
        let own_role = if own.suit() == face_up.suit() {
            "Trump"
        } else {
            "Off-T"
        };
        let bucket = Bucket {
            own_role,
            own_rank: own.rank(),
            face_up_rank: face_up.rank(),
        };
        agg.entry(bucket).or_default().push((b0, b1));
    }

    // Pretty-print one row per (own_role, own_rank) summarising p(bid 1)
    // across face_up_ranks.
    println!(
        "  {:>6} {:>9} {:>30}",
        "own", "rank", "p(bid 1) by face_up_rank: 2..A"
    );
    let mut by_role_rank: BTreeMap<(&'static str, u8), Vec<(u8, f64)>> = BTreeMap::new();
    for (k, vs) in &agg {
        // Average p(bid 1) across (face_up_rank) bucket — should already
        // be unique by face_up_rank, but average defensively.
        let mean_b1: f64 = vs.iter().map(|(_, b1)| *b1).sum::<f64>() / vs.len() as f64;
        by_role_rank
            .entry((k.own_role, k.own_rank))
            .or_default()
            .push((k.face_up_rank, mean_b1));
    }
    for ((role, own_rank), face_up_probs) in by_role_rank {
        let mut sorted: Vec<(u8, f64)> = face_up_probs;
        sorted.sort_by_key(|&(r, _)| r);
        let s = sorted
            .iter()
            .map(|(_, p)| format!("{:.2}", p))
            .collect::<Vec<_>>()
            .join(" ");
        println!(
            "  {:>6} {:>9} {}",
            role,
            rank_name(own_rank),
            s
        );
    }
}

fn rank_name(rank: u8) -> &'static str {
    match rank {
        2 => "2",
        3 => "3",
        4 => "4",
        5 => "5",
        6 => "6",
        7 => "7",
        8 => "8",
        9 => "9",
        10 => "T",
        11 => "J",
        12 => "Q",
        13 => "K",
        14 => "A",
        _ => "?",
    }
}
