//! Checkpoint-selection sweep for Oh Hell R-NaD runs: head-to-head every
//! candidate against a fixed reference (default: the bootstrap the run
//! warm-started from) at every trick count, both sides greedy-LM, and
//! report the pooled mean payoff so the training-curve peak can be
//! picked at meaningful n.
//!
//! Knobs (env vars):
//!   RN_REF        reference weights
//!                 (default /home/steven/card_platypus/gomcts/oh_hell/bootstrap_v2.safetensors)
//!   RN_CAND_DIR   directory holding candidate .safetensors
//!                 (default /home/steven/card_platypus/gomcts/oh_hell/rnad)
//!   RN_GAMES      hands per candidate PER TRICK COUNT      (default 1000)
//!   RN_PLAYERS    players                                  (default 3)
//!   RN_TRICKS_MIN/MAX trick-count range                    (default 1/10)
//!   RN_CONFIG     smoke | medium | paper                   (default paper)
//!   RN_EVAL_TEMP  greedy-LM temperature                    (default 0.05)
//!   RN_SEED       base RNG seed                            (default 0)

use card_platypus::algorithms::gomcts_transformer::{
    head_to_head_eval_batched_tch_infer, oh_hell::OhHellTokenizer, parse_env as parse,
    parse_env_path, ActionTokenFn, GoMctsTransformerTch, InferenceMode, Tokenizer,
    TransformerConfig,
};
use games::gamestates::oh_hell::{OhHell, OhHellGameState};
use std::time::Instant;

fn load(
    path: &std::path::Path,
    cfg: TransformerConfig,
    device: tch::Device,
) -> GoMctsTransformerTch {
    let mut net = GoMctsTransformerTch::new(cfg, device).expect("build net");
    net.load_safetensors(path).unwrap_or_else(|e| panic!("load {}: {e}", path.display()));
    net
}

fn main() {
    let ref_path = parse_env_path(
        "RN_REF",
        "/home/steven/card_platypus/gomcts/oh_hell/bootstrap_v2.safetensors",
    );
    let cand_dir = parse_env_path("RN_CAND_DIR", "/home/steven/card_platypus/gomcts/oh_hell/rnad");
    let n_games: usize = parse("RN_GAMES", 1000);
    let players: usize = parse("RN_PLAYERS", 3);
    let tricks_min: usize = parse("RN_TRICKS_MIN", 1);
    let tricks_max: usize = parse("RN_TRICKS_MAX", 10);
    let temp: f64 = parse("RN_EVAL_TEMP", 0.05);
    let base_seed: u64 = parse("RN_SEED", 0);
    let device = tch::Device::cuda_if_available();
    let tokenizer = OhHellTokenizer;
    let cfg = TransformerConfig::from_env(
        "RN_CONFIG",
        "paper",
        OhHellTokenizer::VOCAB_SIZE,
        OhHellTokenizer::MAX_CONTEXT,
    );
    let atf: ActionTokenFn = std::sync::Arc::new(move |a| tokenizer.action_token(a));

    let mut candidates: Vec<std::path::PathBuf> = std::fs::read_dir(&cand_dir)
        .expect("read candidate dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "safetensors"))
        .collect();
    candidates.sort();
    assert!(!candidates.is_empty(), "no .safetensors in {}", cand_dir.display());

    println!(
        "OhHell R-NaD checkpoint sweep: ref={}, {} candidates, {} games × t{}..={} each, temp={}",
        ref_path.display(),
        candidates.len(),
        n_games,
        tricks_min,
        tricks_max,
        temp
    );
    let reference = load(&ref_path, cfg, device);

    for (i, cand) in candidates.iter().enumerate() {
        let t0 = Instant::now();
        let net = load(cand, cfg, device);
        let mut pooled = 0.0;
        let mut per_t = String::new();
        let n_tricks = tricks_max - tricks_min + 1;
        for t in tricks_min..=tricks_max {
            let new_state = move || OhHell::new_state(players, t);
            let (mean, _) = head_to_head_eval_batched_tch_infer::<OhHellGameState, _, _>(
                &net,
                &reference,
                &tokenizer,
                new_state,
                n_games,
                base_seed.wrapping_add((i * 100 + t) as u64 * 1_000_003),
                false,
                1,
                InferenceMode::LmSoftmax,
                0.0,
                Some(atf.clone()),
                temp,
            );
            pooled += mean / n_tricks as f64;
            per_t.push_str(&format!(" t{t}={mean:+.3}"));
        }
        let name = cand.file_name().unwrap_or_default().to_string_lossy();
        println!(
            "{name}  pooled={pooled:+.4} {per_t}  ({:.1}s)",
            t0.elapsed().as_secs_f64()
        );
        println!("kestrel: step={} cand_vs_ref={pooled:.6}", i + 1);
    }
}
