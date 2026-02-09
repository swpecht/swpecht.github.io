# Liars Poker Bot

Rust workspace for game-playing AI, focused on Euchre. Uses Counterfactual Regret Minimization (CFR) to train Nash equilibrium strategies for imperfect information games.

## Build & Test

```bash
cargo build                          # Build all crates
cargo test                           # Run all tests
cargo bench -p card_platypus         # Run benchmarks
cargo build --profile wasm           # Build WASM frontend
cargo xtask <subcommand>             # Build automation (deploy, serve, etc.)
```

Training a CFR bot:
```bash
cargo run -p card_platypus --release -- euchre-cfr-train <profile>
```
Training profiles are defined in `Train.toml`. Trained weights are stored in `/var/lib/card_platypus/`.

## Project Structure

```
crates/
  games/                  # Core game logic: GameState trait, Euchre, Kuhn Poker, Bluff
  card_platypus/          # AI algorithms: CFR, PIMCTS, ISMCTS, open hand solver
  euchre_server/          # Actix-web backend with WebSocket support
  euchre-app/             # Dioxus WASM frontend (Tailwind CSS)
  client-server-messages/ # Shared serde message types
  ml/                     # ML experiments (Burn framework)
  xtask/                  # Build/deploy automation
```

## Architecture

- **GameState trait** (`crates/games/src/lib.rs`): All games implement this for algorithm-agnostic play
- **CFRES** (`crates/card_platypus/src/algorithms/cfres.rs`): Main training algorithm with linear CFR and feature flags
- **NodeStore** (`crates/card_platypus/src/database/`): Persistent info state storage with disk-backed vectors and mmap
- **Isomorphic state reduction**: Normalizes equivalent game states to shrink the game tree
- Serialization uses MessagePack (rmp-serde) by default

## Key Conventions

- Stable Rust toolchain (see `rust-toolchain.toml`)
- Release builds include debug symbols (`debug = true` in profile)
- `cargo xtask` alias defined in `.cargo/config.toml`
- Verbosity flag `-v` (0=error, 1=warn, 2=info, 3=debug, 4=trace)
