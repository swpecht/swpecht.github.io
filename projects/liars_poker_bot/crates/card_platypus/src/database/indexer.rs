use boomphf::Mphf;
use games::{
    gamestates::{
        bluff::Bluff, euchre::iterator::EuchreIsomorphicIStateIterator, kuhn_poker::KuhnPoker,
    },
    istate::IStateKey,
    iterator::IStateIterator,
};
use itertools::Itertools;

const GAMMA: f64 = 1.7;

pub struct Indexer {
    phf: Mphf<IStateKey>,
    len: usize,
}

impl Indexer {
    /// May return None if the key isn't in the original function, but this isn't guaranteed
    pub fn index(&self, key: &IStateKey) -> Option<usize> {
        self.phf.try_hash(key).map(|x| x as usize)
    }

    pub fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn euchre() -> Self {
        // TODO: in the future can use make it so the hashing happens in stages so that later istates are offset from others as a way to save space
        // Or can pass in the max num cards as a parameter
        let istate_iter = EuchreIsomorphicIStateIterator::new(4);
        let istates = istate_iter.collect_vec();
        let phf = Mphf::new(GAMMA, &istates);
        Self {
            phf,
            len: istates.len(),
        }
    }

    pub fn kuhn_poker() -> Self {
        let istate_iter = IStateIterator::new(KuhnPoker::new_state());
        let istates = istate_iter.collect_vec();
        let phf = Mphf::new(GAMMA, &istates);
        Self {
            phf,
            len: istates.len(),
        }
    }

    pub fn bluff_11() -> Self {
        let istate_iter = IStateIterator::new(Bluff::new_state(1, 1));
        let istates = istate_iter.collect_vec();
        let phf = Mphf::new(GAMMA, &istates);
        Self {
            phf,
            len: istates.len(),
        }
    }
}
