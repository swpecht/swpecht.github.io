//! Card-game isomorphism utilities.
//!
//! Currently houses a Rust port of Kevin Waugh's hand isomorphism
//! algorithm (Waugh 2013, "A Fast and Optimal Hand Isomorphism
//! Algorithm"), which enumerates canonical card-revelation sequences
//! across multi-round games (poker preflop / flop / turn / river — or
//! in our case, Oh Hell hand / face-up / play / play / …) with
//! suit-symmetry reduction.
//!
//! The algorithm gives us two pieces we need for the disk-backed CFR
//! indexer:
//!   * a dense bijection between canonical card sequences and
//!     `[0, size(round))` (so we can feed unique keys into `boomphf`),
//!   * an `unindex(round, idx) → cards` inverse so we can stream
//!     canonical sequences out without a HashSet.
//!
//! See `hand_indexer` for the full implementation and the test module
//! that cross-checks every result against published holdem numbers and
//! against the existing OH iso enumerator.

pub mod hand_indexer;
