use boomphf::Mphf;
use games::{
    gamestates::{
        bluff::Bluff,
        euchre::{
            actions::EAction, ismorphic::normalize_euchre_istate,
            iterator::EuchreIsomorphicIStateIterator,
        },
        kuhn_poker::KuhnPoker,
    },
    istate::IStateKey,
    iterator::IStateIterator,
    Action,
};
use itertools::Itertools;

use crate::collections::mmapvec::MMapVec;

const GAMMA: f64 = 1.7;

pub struct Indexer {
    phf: Mphf<IStateKey>,
    shard_len: usize,
    num_shards: usize,
    /// Returns the normalized istatekey and the associated shard
    /// Shards can be used to keep similar istates near each other in the database
    sharder: fn(&IStateKey) -> Option<(usize, IStateKey)>,
}

impl Indexer {
    /// May return None if the key isn't in the original function, but this isn't guaranteed
    pub fn index(&self, key: &IStateKey) -> Option<usize> {
        let (shard, normed) = (self.sharder)(key)?;
        self.phf
            .try_hash(&normed)
            .map(|x| x as usize + (shard * self.shard_len))
    }

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
        let istate_iter = EuchreIsomorphicIStateIterator::new(max_cards_played);
        // Use an mmap vector as this collection may not fit into memory. This is also
        // more performant than the chunked iterator approach as we do not have an efficient method to
        // find the nth item for the iterator -- a common call in later rounds of the phf.
        let istates = MMapVec::from_iter(istate_iter);
        let phf = Mphf::new(GAMMA, &istates);

        Self {
            phf,
            shard_len: istates.len(),
            num_shards: 6, // one for each possible face up card
            sharder: euchre_sharder,
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
            sharder: |x| Some((0, *x)),
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
            sharder: |x| Some((0, *x)),
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
