//! Euchre transformer smoke test for the GO-MCTS pipeline.
//!
//! Builds a small transformer with the Euchre tokenizer, runs a handful
//! of self-play games to verify the pipeline scales past Kuhn, trains
//! briefly, and confirms the training loss decreases.
//!
//! What this is NOT: a real Euchre training run. The model is tiny
//! (d=64, 2 layers, 4 heads), the data volume is way too small, and
//! we do CPU-only forward/backward over a vocabulary 5× larger than
//! Kuhn's. A paper-faithful Euchre run needs days of GPU training.
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

use card_platypus::algorithms::{
    gomcts::GenerativeModel,
    gomcts_transformer::{
        collect_self_play_game, euchre::EuchreTokenizer, train, GoMctsTransformer, TrainExample,
        TransformerConfig, TransformerGenerativeModel,
    },
};
use candle_core::Device;
use games::{
    gamestates::euchre::{Euchre, EuchreGameState},
    GameState,
};
use rand::{rngs::StdRng, SeedableRng};
use std::time::Instant;

fn parse<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}

type EModel = TransformerGenerativeModel<EuchreGameState, EuchreTokenizer>;

fn main() {
    let n_games: usize = parse("EU_GAMES", 30);
    let epochs: usize = parse("EU_EPOCHS", 4);
    let batch_size: usize = parse("EU_BATCH_SIZE", 32);
    let lr: f64 = parse("EU_LR", 1e-3);
    let base_seed: u64 = parse("EU_SEED", 0);

    println!(
        "Euchre transformer smoke: games={}, epochs={}, batch={}, lr={}",
        n_games, epochs, batch_size, lr,
    );

    let cfg = TransformerConfig::euchre_smoke(
        EuchreTokenizer::VOCAB_SIZE,
        EuchreTokenizer::MAX_CONTEXT,
    );
    let net = GoMctsTransformer::new(cfg, Device::Cpu).expect("build");
    let mut model = EModel::new(net, EuchreTokenizer);
    let mut rng: StdRng = SeedableRng::seed_from_u64(base_seed);

    let t0 = Instant::now();
    let examples = collect(&mut model, n_games, base_seed.wrapping_add(1));
    let collect_secs = t0.elapsed().as_secs_f64();
    println!(
        "collected {} (history, action, value) tuples from {} games in {:.2}s",
        examples.len(),
        n_games,
        collect_secs,
    );

    if examples.is_empty() {
        eprintln!("no examples collected (every game terminated without a non-chance move?)");
        return;
    }

    let t1 = Instant::now();
    let loss_before =
        train(&mut model, &examples, 1, batch_size, lr, &mut rng).expect("train one epoch");
    let loss_after =
        train(&mut model, &examples, epochs, batch_size, lr, &mut rng).expect("train rest");
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

fn collect(model: &mut EModel, n_games: usize, seed: u64) -> Vec<TrainExample> {
    let mut out = Vec::new();
    for game_idx in 0..n_games {
        let mut rng: StdRng = SeedableRng::seed_from_u64(seed.wrapping_add(1 + game_idx as u64));
        let mut buf = Vec::new();
        let exs = collect_self_play_game(
            || Euchre::new_state(),
            |gs: &EuchreGameState, rng: &mut StdRng| {
                let p = gs.cur_player();
                let h = gs.istate_key(p);
                buf.clear();
                gs.legal_actions(&mut buf);
                <EModel as GenerativeModel<EuchreGameState>>::sample(model, &h, &buf, rng)
            },
            &mut rng,
        );
        out.extend(exs);
    }
    out
}
