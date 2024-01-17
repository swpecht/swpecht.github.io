use boomphf::Mphf;
use games::{
    gamestates::{
        bluff::Bluff,
        euchre::{
            ismorphic::normalize_euchre_istate,
            iterator::{EuchreIsomorphicIStateIterator, EuchreIsomorphicIStates},
        },
        kuhn_poker::KuhnPoker,
    },
    istate::IStateKey,
    iterator::IStateIterator,
};
use itertools::Itertools;

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
        let istates = istate_iter.collect_vec();
        let phf = Mphf::new(GAMMA, &istates);
        let n = istates.len();
        // use games::gamestates::euchre::actions::EAction::*;
        // let istates = vec![
        //     EuchreIsomorphicIStates::with_face_up(max_cards_played, &[NS]),
        //     EuchreIsomorphicIStates::with_face_up(max_cards_played, &[TS]),
        //     EuchreIsomorphicIStates::with_face_up(max_cards_played, &[JS]),
        //     EuchreIsomorphicIStates::with_face_up(max_cards_played, &[QS]),
        //     EuchreIsomorphicIStates::with_face_up(max_cards_played, &[KS]),
        //     EuchreIsomorphicIStates::with_face_up(max_cards_played, &[AS]),
        // ];
        // let n = istates.iter().map(|x| x.into_iter().count() as u64).sum();

        // let phf = Mphf::from_chunked_iterator(GAMMA, &istates, n);
        Self {
            phf,
            len: n as usize,
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
