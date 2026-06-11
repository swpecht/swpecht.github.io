//! Checkpoint-selection sweep for Euchre R-NaD runs: head-to-head every
//! candidate checkpoint against a fixed reference (default: the BC
//! bootstrap the run warm-started from), both sides greedy-LM, and
//! report mean payoff + win rate so the peak of the training curve can
//! be picked at higher n than the in-training evals.
//!
//! Knobs (env vars):
//!   RN_REF        reference weights
//!                 (default /home/steven/card_platypus/gomcts/bootstrap_combined.safetensors)
//!   RN_CAND_DIR   directory holding candidate .safetensors
//!                 (default /home/steven/card_platypus/gomcts/rnad)
//!   RN_GAMES      hands per candidate                  (default 2000)
//!   RN_CONFIG     smoke | medium | paper               (default paper)
//!   RN_VHEAD      scalar | outcome                     (default outcome)
//!   RN_EVAL_TEMP  greedy-LM temperature                (default 0.05)
//!   RN_SEED       base RNG seed                        (default 0)

use card_platypus::algorithms::gomcts_transformer::{
    euchre::{EuchreTokenizer, OUTCOME_VALUES as EUCHRE_OUTCOME_VALUES},
    head_to_head_eval_batched_tch_infer, parse_env as parse, parse_env_path, ActionTokenFn,
    GoMctsTransformerTch, InferenceMode, Tokenizer, TransformerConfig,
};
use games::gamestates::euchre::{Euchre, EuchreGameState};
use std::time::Instant;

fn load(
    path: &std::path::Path,
    cfg: TransformerConfig,
    device: tch::Device,
    outcome: bool,
) -> GoMctsTransformerTch {
    let mut net = if outcome {
        GoMctsTransformerTch::new_with_outcomes(cfg, device, EUCHRE_OUTCOME_VALUES.to_vec())
            .expect("build net")
    } else {
        GoMctsTransformerTch::new(cfg, device).expect("build net")
    };
    net.load_safetensors(path).unwrap_or_else(|e| panic!("load {}: {e}", path.display()));
    net
}

fn main() {
    let ref_path = parse_env_path(
        "RN_REF",
        "/home/steven/card_platypus/gomcts/bootstrap_combined.safetensors",
    );
    let cand_dir = parse_env_path("RN_CAND_DIR", "/home/steven/card_platypus/gomcts/rnad");
    let n_games: usize = parse("RN_GAMES", 2000);
    let outcome = std::env::var("RN_VHEAD").as_deref().unwrap_or("outcome") == "outcome";
    let temp: f64 = parse("RN_EVAL_TEMP", 0.05);
    let base_seed: u64 = parse("RN_SEED", 0);
    let device = tch::Device::cuda_if_available();
    let tokenizer = EuchreTokenizer;
    let cfg = TransformerConfig::from_env(
        "RN_CONFIG",
        "paper",
        EuchreTokenizer::VOCAB_SIZE,
        EuchreTokenizer::MAX_CONTEXT,
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
        "R-NaD checkpoint sweep: ref={}, {} candidates, {} games each, temp={}",
        ref_path.display(),
        candidates.len(),
        n_games,
        temp
    );
    let reference = load(&ref_path, cfg, device, outcome);

    for (i, cand) in candidates.iter().enumerate() {
        let t0 = Instant::now();
        let net = load(cand, cfg, device, outcome);
        let (mean, wr) = head_to_head_eval_batched_tch_infer::<EuchreGameState, _, _>(
            &net,
            &reference,
            &tokenizer,
            Euchre::new_state,
            n_games,
            base_seed.wrapping_add(i as u64 * 1_000_003),
            false,
            1,
            InferenceMode::LmSoftmax,
            0.0,
            Some(atf.clone()),
            temp,
        );
        let name = cand.file_name().unwrap_or_default().to_string_lossy();
        println!(
            "{name}  mean={mean:+.4}  wr={wr:.4}  ({:.1}s)",
            t0.elapsed().as_secs_f64()
        );
        println!("kestrel: step={} cand_vs_ref={mean:.6} cand_wr={wr:.6}", i + 1);
    }
}
