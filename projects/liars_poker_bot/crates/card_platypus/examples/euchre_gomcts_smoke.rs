//! Euchre transformer smoke test for the GO-MCTS pipeline (tch).
//!
//! Builds a small transformer with the Euchre tokenizer, runs a handful
//! of cross-game-batched MCTS self-play games to verify the pipeline,
//! trains briefly, and confirms training loss decreases.
//!
//! Run:
//!   cargo run -p card_platypus --release --example euchre_gomcts_smoke
//!
//! Knobs (env vars):
//!   EU_GAMES         self-play games                       (default 30)
//!   EU_EPOCHS        training epochs                       (default 4)
//!   EU_BATCH_SIZE    training batch size                   (default 32)
//!   EU_LR            learning rate                         (default 1e-3)
//!   EU_SEED          base RNG seed                         (default 0)

use card_platypus::algorithms::gomcts_transformer::{
    collect_self_play_games_batched_alphazero_tch, euchre::EuchreTokenizer, train_tch,
    GoMctsTransformerTch, McfsConfig, TransformerConfig,
};
use games::gamestates::euchre::Euchre;
use rand::{rngs::StdRng, SeedableRng};
use std::time::Instant;

fn parse<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}

fn main() {
    let n_games: usize = parse("EU_GAMES", 30);
    let epochs: usize = parse("EU_EPOCHS", 4);
    let batch_size: usize = parse("EU_BATCH_SIZE", 32);
    let lr: f64 = parse("EU_LR", 1e-3);
    let base_seed: u64 = parse("EU_SEED", 0);

    println!(
        "Euchre transformer smoke (tch): games={}, epochs={}, batch={}, lr={}",
        n_games, epochs, batch_size, lr,
    );

    let cfg = TransformerConfig::euchre_smoke(
        EuchreTokenizer::VOCAB_SIZE,
        EuchreTokenizer::MAX_CONTEXT,
    );
    let device = tch::Device::Cpu;
    let mut net = GoMctsTransformerTch::new(cfg, device).expect("build");
    let tokenizer = EuchreTokenizer;
    let mut rng: StdRng = SeedableRng::seed_from_u64(base_seed);

    let t0 = Instant::now();
    let mcfs = McfsConfig::default();
    let examples = collect_self_play_games_batched_alphazero_tch::<_, _, _>(
        &net,
        &tokenizer,
        Euchre::new_state,
        n_games,
        128,
        8,
        mcfs,
        base_seed.wrapping_add(1),
        false,
        1,
    );
    let collect_secs = t0.elapsed().as_secs_f64();
    println!(
        "collected {} examples from {} games in {:.2}s",
        examples.len(),
        n_games,
        collect_secs,
    );

    if examples.is_empty() {
        eprintln!("no examples collected");
        return;
    }

    let t1 = Instant::now();
    let loss_before = train_tch(&mut net, &tokenizer, &examples, 1, batch_size, lr, &mut rng)
        .expect("train one epoch");
    let loss_after = train_tch(&mut net, &tokenizer, &examples, epochs, batch_size, lr, &mut rng)
        .expect("train rest");
    let train_secs = t1.elapsed().as_secs_f64();
    println!(
        "loss before: {:.4}, loss after {} epochs: {:.4} (train wall: {:.2}s)",
        loss_before,
        epochs + 1,
        loss_after,
        train_secs,
    );
    println!(
        "kestrel: step=1 loss_before={:.6} loss_after={:.6} examples={} train_secs={:.4} collect_secs={:.4}",
        loss_before,
        loss_after,
        examples.len(),
        train_secs,
        collect_secs,
    );

    if loss_after < loss_before {
        println!("OK: loss decreased — Euchre training pipeline alive.");
    } else {
        println!(
            "WARNING: loss did NOT decrease (before={:.4}, after={:.4}). \
             Likely too few examples for the larger model.",
            loss_before, loss_after
        );
    }
}
