use boomphf::Mphf;
use games::{
    gamestates::{
        bluff::Bluff,
        euchre::{ismorphic::normalize_euchre_istate, iterator::EuchreIsomorphicIStateIterator},
        kuhn_poker::KuhnPoker,
    },
    istate::IStateKey,
    iterator::IStateIterator,
};
use itertools::Itertools;

use crate::collections::mmapvec::MMapVec;

const GAMMA: f64 = 1.7;

pub struct Indexer {
    phf: Mphf<IStateKey>,
    len: usize,
    normalizer: fn(&IStateKey) -> IStateKey,
}

impl Indexer {
    /// May return None if the key isn't in the original function, but this isn't guaranteed
    pub fn index(&self, key: &IStateKey) -> Option<usize> {
        let normed = (self.normalizer)(key);
        self.phf.try_hash(&normed).map(|x| x as usize)
    }

    pub fn len(&self) -> usize {
        self.len
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
        let n = istates.len();

        Self {
            phf,
            len: n,
            normalizer: normalize_euchre_istate,
        }
    }

    pub fn kuhn_poker() -> Self {
        let istate_iter = IStateIterator::new(KuhnPoker::new_state());
        let istates = istate_iter.collect_vec();
        let phf = Mphf::new(GAMMA, &istates);
        Self {
            phf,
            len: istates.len(),
            normalizer: |x| *x,
        }
    }

    pub fn bluff_11() -> Self {
        let istate_iter = IStateIterator::new(Bluff::new_state(1, 1));
        let istates = istate_iter.collect_vec();
        let phf = Mphf::new(GAMMA, &istates);
        Self {
            phf,
            len: istates.len(),
            normalizer: |x| *x,
        }
    }
}
