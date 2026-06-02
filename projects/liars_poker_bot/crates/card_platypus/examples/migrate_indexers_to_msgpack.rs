//! One-shot migration tool: re-serialize any indexer files that are still
//! JSON-encoded to MessagePack (`rmp-serde`). The runtime load path now
//! prefers MessagePack — JSON files still work, but parsing the 216 MB
//! `infostate.three_card_played_f32` JSON takes ~9 minutes vs ~2 seconds
//! for the MessagePack version.
//!
//! Behaviour: scans each provided directory for a file called `indexer`,
//! attempts to deserialize it as MessagePack; if that succeeds the file
//! is already migrated and we skip it. Otherwise we parse it as JSON
//! (slow), then atomically rewrite the file in MessagePack form via a
//! `indexer.msgpack.tmp` sidecar + rename so a crash mid-write leaves
//! the original intact.
//!
//! Usage:
//!   cargo run -p card_platypus --release --example migrate_indexers_to_msgpack \
//!     -- /home/steven/card_platypus/infostate.baseline \
//!        /home/steven/card_platypus/infostate.three_card_played_f32 \
//!        /home/steven/cache/oh_cfr/2p_2t_max0
//!
//! With no arguments, defaults to migrating every `infostate.*` directory
//! under `/home/steven/card_platypus/` plus every `*p_*t_max*` directory
//! under `/home/steven/cache/oh_cfr/`.

use std::{
    env, fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Instant,
};

use card_platypus::database::indexer::Indexer;

fn default_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for base in [
        "/home/steven/card_platypus",
        "/home/steven/cache/oh_cfr",
    ] {
        let Ok(entries) = fs::read_dir(base) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() && p.join("indexer").exists() {
                dirs.push(p);
            }
        }
    }
    dirs
}

fn migrate_one(dir: &Path) -> anyhow::Result<bool> {
    let indexer_path = dir.join("indexer");
    let bytes = {
        let mut f = fs::File::open(&indexer_path)?;
        let mut b = Vec::new();
        f.read_to_end(&mut b)?;
        b
    };
    let orig_size = bytes.len();

    // Already MessagePack? Skip — let an existing migrated file alone
    // even if the user re-runs the tool.
    if rmp_serde::from_slice::<Indexer>(&bytes).is_ok() {
        println!(
            "  skip  {} (already MessagePack, {:.1} MB)",
            indexer_path.display(),
            orig_size as f64 / 1_048_576.0
        );
        return Ok(false);
    }

    // Parse as JSON (the slow side). Try the current `Phf`/`WaughOh`
    // enum tag first; fall back to the pre-enum struct layout that the
    // older Euchre indexers were written with.
    let json_start = Instant::now();
    let utf8 = std::str::from_utf8(&bytes)
        .map_err(|_| anyhow::anyhow!("indexer is neither MessagePack nor valid UTF-8"))?;
    let indexer: Indexer = match serde_json::from_str::<Indexer>(utf8) {
        Ok(i) => i,
        Err(_) => Indexer::from_legacy_struct_json(utf8)?,
    };
    let json_elapsed = json_start.elapsed().as_secs_f64();

    // Serialize back as MessagePack.
    let msgpack_start = Instant::now();
    let new_bytes = rmp_serde::to_vec(&indexer)?;
    let msgpack_elapsed = msgpack_start.elapsed().as_secs_f64();

    // Write atomically: sidecar tmp + rename. If anything goes wrong
    // mid-write, the original JSON file stays intact and the tool can
    // be re-run.
    let tmp_path = dir.join("indexer.msgpack.tmp");
    {
        let mut f = fs::File::create(&tmp_path)?;
        f.write_all(&new_bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp_path, &indexer_path)?;

    println!(
        "  done  {}  {:.1} MB JSON -> {:.1} MB msgpack  (parse {:.1}s, encode {:.2}s)",
        indexer_path.display(),
        orig_size as f64 / 1_048_576.0,
        new_bytes.len() as f64 / 1_048_576.0,
        json_elapsed,
        msgpack_elapsed,
    );
    Ok(true)
}

fn main() {
    let args: Vec<PathBuf> = env::args().skip(1).map(PathBuf::from).collect();
    let dirs = if args.is_empty() {
        let d = default_dirs();
        println!(
            "no dirs passed; scanning defaults — found {} indexer dirs",
            d.len()
        );
        d
    } else {
        args
    };

    let mut migrated = 0u32;
    let mut skipped = 0u32;
    let mut failed = 0u32;
    for dir in &dirs {
        match migrate_one(dir) {
            Ok(true) => migrated += 1,
            Ok(false) => skipped += 1,
            Err(e) => {
                eprintln!("  FAIL  {}: {}", dir.display(), e);
                failed += 1;
            }
        }
    }

    println!();
    println!(
        "summary: {} migrated, {} already MessagePack, {} failed",
        migrated, skipped, failed
    );
    if failed > 0 {
        std::process::exit(1);
    }
}
