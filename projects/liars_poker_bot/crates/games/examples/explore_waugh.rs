//! Exploration: Waugh-based direct indexing vs the current iterator
//! + PHF approach for Oh Hell.
//!
//! BIDDING (max=0): Waugh exactly predicts the iterator's iso count
//! via the formula
//!   waugh([nt, 1]).size(1) × Σ_{p=0..np} (nt+1)^p
//! (verified for all 6 configs above).
//!
//! PLAY (max≥1): the iterator's count is much smaller than Waugh's
//! combinatorial iso class count. Two reasons:
//!   1. The max≤1 opp-perm short-circuit only emits one placeholder
//!      representative per (perspective, canonical hand, bid sequence).
//!      Under OhHellNormalizer's suit-perm canonicalisation (which
//!      uses an order-sensitive fingerprint per suit and pins the
//!      trump suit) many distinct combinatorial (face_up, hand, opp
//!      plays so far) tuples collapse to the same iso class.
//!   2. Game-rule constraints (follow-suit on trick 1) make many
//!      Waugh-canonical (face_up, hand, opp_lead_card) tuples
//!      unreachable in actual play. The iterator's walker emits only
//!      reachable ones.
//!
//! So Waugh is a strict upper bound on the play-phase slot count, not
//! a perfect match. This pass tabulates the bound vs reality and
//! projects what 3p×3t×max=2 (the stuck config) would cost.

use games::iso::hand_indexer::HandIndexer;

fn bidding(np: usize, nt: usize) -> u64 {
    let waugh = HandIndexer::init(&[nt as u8, 1]).unwrap().size(1);
    let bid_base = (nt + 1) as u64;
    let prefix_sum: u64 = (0..np).map(|p| bid_base.pow(p as u32)).sum();
    waugh * prefix_sum
}

/// Waugh upper bound for total iso classes (bidding + play through
/// depth `max_cards`). Assumes one play istate per perspective per
/// full bid history per Waugh-canonical (face_up, hand, plays_so_far).
fn waugh_total(np: usize, nt: usize, max_cards: usize) -> u64 {
    let bid_full = ((nt + 1) as u64).pow(np as u32);

    let mut rounds = vec![nt as u8, 1];
    for _ in 0..max_cards {
        rounds.push(1);
    }
    let indexer = HandIndexer::init(&rounds).unwrap();

    let mut play_iso = 0u64;
    for d in 0..max_cards {
        // Depth d plays so far → round 1 (hand+face_up) + d play rounds = round (1+d).
        // Each perspective contributes one istate at the depth at which they first act
        // (depth 0 for the leader, depth k for the player k seats after the leader).
        // Cap at np perspectives that can possibly act within `max_cards` plays.
        let perspectives_at_depth = if d < np { 1 } else { 0 };
        play_iso += perspectives_at_depth * indexer.size(1 + d);
    }
    play_iso *= bid_full;

    bidding(np, nt) + play_iso
}

fn report(np: usize, nt: usize, max: usize) {
    let pred = waugh_total(np, nt, max);
    let meta_path = format!(
        "{}/cache/oh_cfr/{}p_{}t_max{}/meta",
        std::env::var("HOME").unwrap_or_default(),
        np, nt, max
    );
    let meta = std::fs::read_to_string(&meta_path)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok());
    let mem_mb = (pred as f64 * 88.0) / (1024.0 * 1024.0);
    match meta {
        Some(m) => {
            let ratio = if m > 0 { pred as f64 / m as f64 } else { 0.0 };
            println!(
                "{}p_{}t_max{}  waugh={:>12}  meta={:>10}  ratio={:>5.1}×  mmap={:>7.1} MB",
                np, nt, max, pred, m, ratio, mem_mb
            );
        }
        None => {
            println!(
                "{}p_{}t_max{}  waugh={:>12}  meta=N/A           mmap={:>7.1} MB",
                np, nt, max, pred, mem_mb
            );
        }
    }
}

fn main() {
    println!("=== existing configs (bidding-only matches exactly) ===");
    for (np, nt, max) in [
        (2, 1, 0), (2, 2, 0), (2, 3, 0),
        (3, 1, 0), (3, 2, 0), (3, 3, 0),
    ] {
        report(np, nt, max);
    }
    println!();
    println!("=== max=1 configs (Waugh = strict upper bound) ===");
    for (np, nt, max) in [
        (2, 2, 1), (2, 3, 1),
        (3, 2, 1), (3, 3, 1),
    ] {
        report(np, nt, max);
    }
    println!();
    println!("=== max=2 (one trained, three not-yet-trained) ===");
    for (np, nt, max) in [
        (2, 2, 2), (2, 3, 2), (3, 2, 2), (3, 3, 2),
    ] {
        report(np, nt, max);
    }
}
