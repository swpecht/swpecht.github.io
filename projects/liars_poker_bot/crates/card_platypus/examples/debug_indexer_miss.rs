//! Reproduce the istate lookup that caused the CFRES mismatch panic.
//! Constructs the exact game state from the panic dump, computes the
//! sharded key the PHF is queried with, and asks two questions:
//!
//!   1. Was that sharded key present in the iterator's enumerated set
//!      (the canonical PHF training domain)?
//!   2. What slot does the on-disk PHF map it to, and what stored
//!      actions are at that slot?
//!
//! If (1) is false → the iterator misses bidding-phase istates that
//! are actually reached at runtime; that's the root cause.
//!
//! Run:
//!   cargo run -p card_platypus --release --example debug_indexer_miss

use card_platypus::{
    algorithms::cfres::EuchreCfres,
    database::indexer::Indexer,
};
use games::{
    gamestates::euchre::{
        actions::EAction,
        isomorphic::normalize_euchre_istate,
        iterator::EuchreIsomorphicIStateIterator,
        EuchreGameState,
    },
    istate::IStateNormalizer,
    Action, GameState,
};
use games::istate::IStateKey;
use rand::{rngs::StdRng, SeedableRng};
use std::path::PathBuf;

fn main() {
    // Reconstruct the failing state by replaying its action sequence.
    // From the panic dump: key = [1, 9, 28, 0, 10, 16, 18, 17, 12, 4, 24, 8, 20, 29, 3, 26, 5, 11, 21, 13, 25, 31]
    let mut gs = games::gamestates::euchre::Euchre::new_state();
    let action_bytes: [u8; 22] = [
        1, 9, 28, 0, 10, 16, 18, 17, 12, 4, 24, 8, 20, 29, 3, 26, 5, 11, 21, 13, 25, 31,
    ];
    for b in action_bytes {
        gs.apply_action(Action(b));
    }
    assert_eq!(gs.cur_player(), 1, "expected cur_player=1");
    let mut legal = Vec::new();
    gs.legal_actions(&mut legal);
    println!("reconstructed gs: {:?}", gs);
    println!("legal actions: {:?}", legal);

    // Raw istate for P1 and its iso-normalized form.
    let raw = gs.istate_key(1);
    let normed = normalize_euchre_istate(&raw);
    println!("\nraw istate    : {:?}", raw);
    println!("iso-normed    : {:?}", normed);

    // Apply the sharder transform that the indexer uses.
    let face_up = *normed.get(5).unwrap();
    let mut sharded = normed;
    sharded.swap(Action::from(EAction::NS), face_up);
    sharded.sort_range(0, 5.min(sharded.len()));
    println!("sharded key   : {:?}  (face_up was {:?})", sharded, face_up);

    // Build the iterator with face-up = NS, max_cards_played = 0 (matches
    // what Indexer::euchre(0) does).
    println!("\nscanning iterator with face_up=NS, max_cards_played=0 ...");
    let mut found = false;
    let mut total = 0u64;
    for k in EuchreIsomorphicIStateIterator::with_face_up(0, &[EAction::NS]) {
        total += 1;
        if k == sharded {
            found = true;
            break;
        }
        if total % 5_000_000 == 0 {
            println!("  scanned {}M ...", total / 1_000_000);
        }
    }
    println!(
        "iterator emitted the sharded key? {} (scanned {} entries{})",
        found,
        total,
        if found { "" } else { " — full set" }
    );

    // What does the actual on-disk PHF say?
    println!("\nquerying the on-disk PHF ...");
    let indexer_dir = PathBuf::from("/home/steven/card_platypus/infostate.baseline");
    if !indexer_dir.exists() {
        println!("  (medium weights dir missing; skipping PHF query)");
        return;
    }

    // Use EuchreCfres::new_euchre to load the same path; it constructs
    // the same NodeStore the bench bot uses (u32-backed ActionList).
    let cfres: EuchreCfres =
        EuchreCfres::new_euchre(StdRng::seed_from_u64(0), 0, Some(indexer_dir.as_path()));
    println!("  loaded {} info states", cfres.num_info_states());

    // Same as IStateNormalizer call inside CFRES::action_probabilities.
    let normer = games::gamestates::euchre::isomorphic::EuchreNormalizer::default();
    let key_for_phf = normer.normalize_istate(&raw, &gs);
    println!("  normalize_istate produced: {:?}", key_for_phf.get());

    // Load the SAVED indexer the weights were trained against; compare its
    // slot assignment to a freshly-built one. If they differ, the saved PHF
    // is out of sync with the current iterator's istate set.
    //
    // Tries MessagePack (current format) first and falls back to JSON
    // (legacy) so this tool works against an un-migrated indexer too.
    use std::fs::File;
    use std::io::Read;
    let saved_indexer: Indexer = {
        let mut f = File::open(indexer_dir.join("indexer")).expect("open indexer");
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes).expect("read indexer");
        if let Ok(idx) = rmp_serde::from_slice::<Indexer>(&bytes) {
            idx
        } else {
            let utf8 = std::str::from_utf8(&bytes).expect("indexer not utf8");
            serde_json::from_str(utf8).expect("parse indexer")
        }
    };
    let idx_saved = saved_indexer.index(&key_for_phf.get());
    println!("  saved Indexer.index(key) = {:?}", idx_saved);
    println!("  saved indexer.len() = {}", saved_indexer.len());

    let fresh = Indexer::euchre(0);
    let idx_fresh = fresh.index(&key_for_phf.get());
    println!("  fresh Indexer::euchre(0).index(key) = {:?}", idx_fresh);
    println!("  fresh indexer.len() = {}", fresh.len());

    // Count the full current iterator output for face_up=NS and check
    // whether the SAVED PHF is injective on it. If the iterator's set has
    // grown since the saved PHF was built, two distinct iterator keys will
    // hash to the same saved-PHF slot — that's the collision pattern that
    // explains the runtime mismatch.
    println!("\nfull scan: saved-PHF slot uniqueness over current iterator keys ...");
    let saved_shard_len = saved_indexer.len() / 6;
    println!("  saved shard_len (PHF training size per shard): {}", saved_shard_len);
    let mut total = 0u64;
    let mut slot_counts: std::collections::HashMap<usize, u32> = std::collections::HashMap::new();
    for k in EuchreIsomorphicIStateIterator::with_face_up(0, &[EAction::NS]) {
        total += 1;
        if let Some(idx) = saved_indexer.index(&k) {
            *slot_counts.entry(idx).or_insert(0) += 1;
        }
        if total % 100_000 == 0 {
            println!("  scanned {}k keys so far ...", total / 1_000);
        }
    }
    let collisions: u32 = slot_counts.values().filter(|&&c| c > 1).count() as u32;
    let max_collision = slot_counts.values().copied().max().unwrap_or(0);
    let extra = total as i64 - saved_shard_len as i64;
    println!(
        "  total iterator keys (face_up=NS): {}\n  saved_shard_len:                  {}\n  difference (current - saved):     {:+}\n  saved-PHF slots hit at least once: {}\n  slots with collisions (>1 hit):    {}\n  max hits on any single slot:       {}",
        total,
        saved_shard_len,
        extra,
        slot_counts.len(),
        collisions,
        max_collision,
    );

    if collisions > 0 {
        println!("\n=> SAVED PHF IS STALE: iterator now emits keys that didn't exist at training time.");
        println!("   The runtime queries with these new keys hit slots assigned to OLD keys.");
    } else if extra != 0 {
        println!("\n=> Size mismatch but no collisions yet — keep scanning more keys.");
    } else {
        println!("\n=> Iterator output matches saved PHF training set.");
    }

    // Now find the EXACT iterator key that hashes to slot 1494931. That's
    // what the training-time CFR was writing when it touched that slot.
    let target_slot = 1494931usize;
    println!("\nlocating the iterator key at slot {} ...", target_slot);
    let mut found_key: Option<IStateKey> = None;
    for k in EuchreIsomorphicIStateIterator::with_face_up(0, &[EAction::NS]) {
        if saved_indexer.index(&k) == Some(target_slot) {
            found_key = Some(k);
            break;
        }
    }
    if let Some(k) = found_key {
        println!("  iterator key at slot {}: {:?}", target_slot, k);
        println!("  vs runtime sharded query key:  {:?}", sharded);
        if k == sharded {
            println!("\n=> Same key. The PHF lookup itself is correct; the stored data really is for this key.");
            println!("   That means at training time, some EuchreGameState mapped to this same iso form");
            println!("   but had a DIFFERENT legal action set — i.e., the iso normalizer is collapsing");
            println!("   non-equivalent states. That's a normalizer bug, not a PHF bug.");
        } else {
            println!("\n=> Different keys mapping to same slot. PHF collision => stale training set.");
        }
    } else {
        println!("  no iterator key maps to slot {} (slot was never trained)", target_slot);
    }
}
