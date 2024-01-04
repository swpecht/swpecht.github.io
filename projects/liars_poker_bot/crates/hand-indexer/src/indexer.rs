use games::{istate::IStateKey, Action};

use crate::{
    cards::{
        iterators::{DealEnumerationIterator, IsomorphicDealIterator},
        Deck,
    },
    phf::RoundIndexer,
};

/// Define different rounds for a game to index. Games are fully defined by a collection of GameRounds
///
/// Each game round must be independent from the others
pub enum RoundType {
    Standard {
        cards_per_round: Vec<usize>,
    },
    Euchre {
        cards_per_round: Vec<usize>,
    },
    /// Warning, this round type performs no isomorphism
    CustomDeck {
        deck: Deck,
        cards_per_round: Vec<usize>,
    },
    /// Support for arbitrary collection of actions
    /// TODO: have this try to match the longest possible collection first
    Choice {
        choices: Vec<Vec<Action>>,
    },
}

/// Convert an information state into an index across all rounds.
///
/// This struct is responsible for all normalizing of inputs
pub struct GameIndexer {
    round_indexers: Vec<RoundIndexer<Vec<Action>>>,
}

impl GameIndexer {
    pub fn new(rounds: Vec<RoundType>) -> Self {
        let mut round_indexers = Vec::new();

        for round_type in rounds {
            let round_indexer = match round_type {
                RoundType::Standard { cards_per_round } => todo!(),
                RoundType::Euchre { cards_per_round } => todo!(),
                RoundType::CustomDeck {
                    deck,
                    cards_per_round,
                } => custom_deck_indexer(deck, &cards_per_round),
                RoundType::Choice { choices } => todo!(),
            };
            round_indexers.push(round_indexer);
        }

        Self { round_indexers }
    }

    pub fn kuhn_poker() -> Self {
        let deck = Deck::kuhn_poker();

        use games::gamestates::kuhn_poker::KPAction::*;
        use RoundType::*;
        GameIndexer::new(vec![
            CustomDeck {
                deck,
                cards_per_round: vec![1, 1],
            },
            Choice {
                choices: vec![
                    vec![Pass.into()],
                    vec![Pass.into(), Pass.into()],
                    vec![Pass.into(), Bet.into()],
                    vec![Pass.into(), Bet.into(), Pass.into()],
                    vec![Pass.into(), Bet.into(), Bet.into()],
                ],
            },
        ])
    }

    /// Calculates the index for a given IStateKey
    ///
    /// This function will perform all necessary istate normalization
    pub fn index(&self, istate: IStateKey) -> usize {
        todo!()
    }

    /// Returns the size of the indexer. The maximum index is size-1
    pub fn size(&self) -> usize {
        self.round_indexers.iter().map(|x| x.size()).product()
    }
}

fn custom_deck_indexer(deck: Deck, cards_per_round: &[usize]) -> RoundIndexer<Vec<Action>> {
    let iterator = DealEnumerationIterator::new(deck, cards_per_round);
    let indexer = RoundIndexer::new(iterator);
    indexer
}

#[cfg(test)]
mod tests {

    use games::gamestates::kuhn_poker::KuhnPoker;

    use super::*;

    #[test]
    fn test_kuhn_poker() {
        let indexer = GameIndexer::kuhn_poker();
        // todo: fix this
        assert_eq!(indexer.size(), 500);

        let mut gs = KuhnPoker::new_state();
    }

    #[test]
    fn test_iso_morphism() {
        // // test the isomorphism works
        // let deals = DealEnumerationIterator::new(Deck::standard(), &[2]);
        // let mut combination_indexes = HashSet::new();
        // for deal in deals {
        //     combination_indexes.insert(indexer.index(&deal).unwrap());
        // }
        // let mut combination_indexes = combination_indexes.into_iter().collect_vec();
        // combination_indexes.sort();
        // assert_eq!(combination_indexes, indexes);

        todo!()
    }
}
