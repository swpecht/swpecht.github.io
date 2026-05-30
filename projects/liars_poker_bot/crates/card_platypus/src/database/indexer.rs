use boomphf::Mphf;
use games::{
    gamestates::{
        bluff::Bluff,
        euchre::{
            actions::EAction, isomorphic::normalize_euchre_istate,
            iterator::EuchreIsomorphicIStateIterator,
        },
        kuhn_poker::KuhnPoker,
        oh_hell::iterator::OhHellIsomorphicIStateIterator,
    },
    istate::IStateKey,
    iterator::IStateIterator,
    Action,
};
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::collections::mmapvec::MMapVec;

const GAMMA: f64 = 1.7;

#[derive(Clone, Copy, Serialize, Deserialize)]
enum Sharder {
    Euchre,
    NoOp,
}

impl Sharder {
    fn shard(&self, istate: &IStateKey) -> Option<(usize, IStateKey)> {
        match self {
            Sharder::Euchre => euchre_sharder(istate),
            Sharder::NoOp => Some((0, *istate)),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Indexer {
    phf: Mphf<IStateKey>,
    shard_len: usize,
    num_shards: usize,
    /// Returns the normalized istatekey and the associated shard
    /// Shards can be used to keep similar istates near each other in the database
    sharder: Sharder,
}

impl Indexer {
    /// May return None if the key isn't in the original function, but this isn't guaranteed
    pub fn index(&self, key: &IStateKey) -> Option<usize> {
        let (shard, normed) = self.sharder.shard(key)?;
        self.phf
            .try_hash(&normed)
            .map(|x| x as usize + (shard * self.shard_len))
    }

    /// Returns the total length of the indexer
    pub fn len(&self) -> usize {
        self.shard_len * self.num_shards
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn euchre(max_cards_played: usize) -> Self {
        // TODO: in the future can use make it so the hashing happens in stages so that later istates are offset from others as a way to save space
        // Or can pass in the max num cards as a parameter
        let istate_iter =
            EuchreIsomorphicIStateIterator::with_face_up(max_cards_played, &[EAction::NS]);
        // Use an mmap vector as this collection may not fit into memory. This is also
        // more performant than the chunked iterator approach as we do not have an efficient method to
        // find the nth item for the iterator -- a common call in later rounds of the phf.
        let istates = MMapVec::from_iter(istate_iter);
        let phf = Mphf::new(GAMMA, &istates);

        Self {
            phf,
            shard_len: istates.len(),
            num_shards: 6, // one for each possible face up card
            sharder: Sharder::Euchre,
        }
    }

    pub fn kuhn_poker() -> Self {
        let istate_iter = IStateIterator::new(KuhnPoker::new_state());
        let istates = istate_iter.collect_vec();
        let phf = Mphf::new(GAMMA, &istates);
        Self {
            phf,
            shard_len: istates.len(),
            num_shards: 1,
            sharder: Sharder::NoOp,
        }
    }

    pub fn bluff_11() -> Self {
        let istate_iter = IStateIterator::new(Bluff::new_state(1, 1));
        let istates = istate_iter.collect_vec();
        let phf = Mphf::new(GAMMA, &istates);
        Self {
            phf,
            shard_len: istates.len(),
            num_shards: 1,
            sharder: Sharder::NoOp,
        }
    }

    /// Build a PHF over the bidding-phase iso classes for Oh Hell with
    /// `(num_players, n_tricks)`. Used by the disk-backed
    /// [`crate::database::NodeStore`] variant that targets bidding-
    /// only CFR (play phase delegated to `OpenHandSolver`).
    ///
    /// The iterator streams canonical IStateKeys via the hand-rolled
    /// `bidding_only` enumeration (Waugh-cross-checked) so we never
    /// hold the full set in a HashSet during PHF construction —
    /// MMapVec spools to disk if it doesn't fit in RAM.
    pub fn oh_hell_bidding(num_players: usize, n_tricks: usize) -> Self {
        let istate_iter = OhHellIsomorphicIStateIterator::bidding_only(num_players, n_tricks);
        let istates = MMapVec::from_iter(istate_iter);
        let phf = Mphf::new(GAMMA, &istates);
        Self {
            phf,
            shard_len: istates.len(),
            num_shards: 1,
            sharder: Sharder::NoOp,
        }
    }

    /// Build a PHF over the **full-game** iso classes for Oh Hell with
    /// `(num_players, n_tricks, max_cards_played)`. Enumerates every
    /// canonical bidding + play-phase decision point via the Waugh-
    /// based iterator with the multi-trick feasibility filter applied.
    ///
    /// Note that for multi-trick configs the Waugh enumerator emits a
    /// strict superset of the walker's iso set (Waugh's per-round
    /// configuration sort doesn't fully fold OH's cross-round suit
    /// symmetry). For the PHF use case this is sound — the extra
    /// slots are simply never queried by CFR — but it does inflate
    /// the mmap footprint relative to the minimal set. The
    /// "tighten-via-round-trip" pass is the natural follow-up.
    pub fn oh_hell_full_game(
        num_players: usize,
        n_tricks: usize,
        max_cards_played: usize,
    ) -> Self {
        let istate_iter = OhHellIsomorphicIStateIterator::full_game_via_waugh(
            num_players,
            n_tricks,
            max_cards_played,
        );
        let istates = MMapVec::from_iter(istate_iter);
        let phf = Mphf::new(GAMMA, &istates);
        Self {
            phf,
            shard_len: istates.len(),
            num_shards: 1,
            sharder: Sharder::NoOp,
        }
    }
}

fn euchre_sharder(istate: &IStateKey) -> Option<(usize, IStateKey)> {
    let mut normed = normalize_euchre_istate(istate);
    let face_up = *normed.get(5)?;
    normed.swap(Action::from(EAction::NS), face_up); // swap to be an istate with ns as the face up card
    normed.sort_range(0, 5.min(normed.len()));

    let face_up = EAction::from(face_up);
    use EAction::*;
    let shard = match face_up {
        NS => 0,
        TS => 1,
        JS => 2,
        QS => 3,
        KS => 4,
        AS => 5,
        _ => panic!("found non-spades face up card after normalization"),
    };

    Some((shard, normed))
}
