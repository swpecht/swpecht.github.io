//! Migrate Euchre training weights from the legacy PHF-indexed layout to
//! the Waugh-indexed layout.
//!
//! **STATUS — Phase 1 scaffold.** Wires up arg parsing, opens the legacy
//! checkpoint, and counts istates that would need to be migrated. The
//! actual slot-copy step is gated on `WaughEuchreIndexer::index()` being
//! implemented (Phase 2) — until then this tool reports what *would*
//! happen but writes nothing.
//!
//! Usage (Phase 2 onward):
//!
//!   cargo run --release --example migrate_euchre_phf_to_waugh -- \
//!     --src /var/lib/card_platypus/infostate.baseline \
//!     --dst /var/lib/card_platypus/infostate.baseline.waugh \
//!     --max-cards 0
//!
//! Migration is per (max_cards_played) checkpoint dir, mirroring the
//! existing Euchre training layout (one dir per depth slice).
//!
//! Algorithm:
//!   1. Load legacy PHF indexer from `src/indexer` (JSON, pre-2f075ed
//!      format — handled by `Indexer::from_legacy_struct_json`).
//!   2. Build `Indexer::euchre_waugh(max_cards)` for `dst`.
//!   3. Open both mmaps (`src/mmap` read-only, `dst/mmap` read-write
//!      sized to the Waugh indexer's `len()`).
//!   4. Walk `EuchreIsomorphicIStateIterator` over every face_up shard
//!      (NS, TS, JS, QS, KS, AS). For each emitted istate:
//!         a. `old_slot = phf_indexer.index(istate).unwrap()`
//!         b. `new_slot = waugh_indexer.index(istate).unwrap()`
//!         c. If `mmap_src[old_slot]` is non-empty, copy it to
//!            `mmap_dst[new_slot]`.
//!         d. Increment migrated counter.
//!   5. Write the new `dst/indexer` (Waugh, tiny — just the max_cards
//!      param). Write `dst/meta` with the populated count.
//!
//! Correctness checks the tool runs before writing:
//!   * `waugh.len()` ≥ count of distinct slots produced by step 4(b).
//!   * No two source slots resolve to the same destination slot
//!     (otherwise we'd silently merge InfoStates, which would corrupt
//!     CFR's accumulated regrets and average policy).
//!
//! After migration, kick off a `cargo run -p card_platypus --release --
//! euchre-cfr-train <profile>` resume — it should pick up the Waugh
//! checkpoint and continue without retraining from scratch.

use std::path::PathBuf;

use card_platypus::database::indexer::Indexer;

#[derive(Default)]
struct Args {
    src: Option<PathBuf>,
    dst: Option<PathBuf>,
    max_cards: usize,
}

fn parse_args() -> Args {
    let mut a = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--src" => a.src = it.next().map(PathBuf::from),
            "--dst" => a.dst = it.next().map(PathBuf::from),
            "--max-cards" => {
                a.max_cards = it
                    .next()
                    .and_then(|s| s.parse().ok())
                    .expect("--max-cards needs a non-negative integer")
            }
            other => {
                eprintln!("unknown flag: {}", other);
                std::process::exit(2);
            }
        }
    }
    a
}

fn main() -> anyhow::Result<()> {
    let args = parse_args();
    let src = args.src.expect("--src required");
    let dst = args.dst.expect("--dst required");

    let legacy_indexer_path = src.join("indexer");
    anyhow::ensure!(
        legacy_indexer_path.exists(),
        "source checkpoint missing indexer file: {}",
        legacy_indexer_path.display()
    );

    println!("Loading legacy PHF indexer from {}…", legacy_indexer_path.display());
    let json = std::fs::read_to_string(&legacy_indexer_path)?;
    let phf = Indexer::from_legacy_struct_json(&json)?;
    println!("  PHF size: {}", phf.len());

    println!("Building Waugh indexer for max_cards_played={}…", args.max_cards);
    let waugh = Indexer::euchre_waugh(args.max_cards);
    println!("  Waugh size: {}", waugh.len());
    let ratio = waugh.len() as f64 / phf.len().max(1) as f64;
    println!(
        "  Waugh / PHF size ratio: {:.3}× ({} slots vs {})",
        ratio,
        waugh.len(),
        phf.len()
    );

    // Phase 1: the actual istate walk + copy is blocked on WaughEuchreIndexer
    // having a real `index()` body. Print a clear marker and exit non-zero
    // so CI / wrappers know this is an incomplete run.
    eprintln!();
    eprintln!("=== Phase 1 scaffold: migration not yet executable ===");
    eprintln!(
        "WaughEuchreIndexer::index() panics by design at this point — see the \
         roadmap comment in crates/card_platypus/src/database/indexer.rs."
    );
    eprintln!(
        "When Phase 2 lands, this tool will walk the Euchre iterator across \
         all 6 face_up shards, look up each istate's old PHF slot and new \
         Waugh slot, and copy InfoStates between mmaps."
    );
    eprintln!("Args confirmed:");
    eprintln!("  src       = {}", src.display());
    eprintln!("  dst       = {}", dst.display());
    eprintln!("  max_cards = {}", args.max_cards);
    std::process::exit(1);
}
