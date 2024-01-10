use smallvec::SmallVec;

use crate::{
    cards::{
        cardset::CardSet,
        iterators::{DealEnumerationIterator, IsomorphicDealIterator},
        Deck,
    },
    phf::RoundIndexer,
};

type Action = u8;
/// Performant type for storing a varying number of actions
pub type ActionVec = SmallVec<[u8; 20]>;

/// Define different rounds for a game to index. Games are fully defined by a collection of GameRounds
///
/// Each game round must be independent from the others
#[derive(Clone)]
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
    Choice {
        choices: Vec<ActionVec>,
    },
}

impl RoundType {
    /// Returns the longest number of actions that match this RoundType
    pub fn matching_actions(&self, actions: &[Action]) -> Option<usize> {
        match self {
            RoundType::Standard { cards_per_round }
            | RoundType::Euchre { cards_per_round }
            | RoundType::CustomDeck {
                deck: _,
                cards_per_round,
            } => {
                let count = cards_per_round.iter().sum::<usize>();
                assert!(
                    count <= actions.len(),
                    "need to implement handling only some of the rounds being dealt"
                );
                Some(count)
            }
            RoundType::Choice { choices } => {
                let mut found_match: Option<&[Action]> = None;
                for c in choices {
                    if c.len() <= actions.len()
                        && c[..] == actions[..c.len()]
                        && c.len() > found_match.map(|x| x.len()).unwrap_or(0)
                    {
                        found_match = Some(&c[..])
                    }
                }
                found_match.map(|x| x.len())
            }
        }
    }
}

/// Convert an information state into an index across all rounds.
///
/// This struct is responsible for all normalizing of inputs
pub struct GameIndexer {
    round_types: Vec<RoundType>,
    round_indexers: Vec<RoundIndexer<ActionVec>>,
}

impl GameIndexer {
    pub fn new(rounds: Vec<RoundType>) -> Self {
        let mut round_indexers = Vec::new();

        for round_type in rounds.clone() {
            let round_indexer = match round_type {
                RoundType::Standard { cards_per_round } => todo!(),
                RoundType::Euchre { cards_per_round } => euchre_indexer(&cards_per_round),
                RoundType::CustomDeck {
                    deck,
                    cards_per_round,
                } => custom_deck_indexer(deck, &cards_per_round),
                RoundType::Choice { choices } => choice_indexer(choices),
            };
            round_indexers.push(round_indexer);
        }

        Self {
            round_indexers,
            round_types: rounds,
        }
    }

    /// Calculates the index for a given IStateKey
    ///
    /// This function will perform all necessary istate normalization.
    ///
    /// IStates with the same starting rounds are grouped near each other.
    pub fn index(&self, istate: &[Action]) -> Option<usize> {
        // todo: implement normalization

        let mut istate_cursor = 0;
        let mut indexes: SmallVec<[usize; 20]> = SmallVec::new();
        for (i, round_type) in self.round_types.iter().enumerate() {
            let len = round_type.matching_actions(&istate[istate_cursor..])?;
            let actions_vec = SmallVec::from_slice(&istate[istate_cursor..istate_cursor + len]);
            let round_index = self.round_indexers[i].index(&actions_vec)?;
            indexes.push(round_index);
            istate_cursor += len;

            if istate_cursor >= istate.len() {
                break;
            }
        }

        let mut offsets: SmallVec<[usize; 20]> = SmallVec::new();
        let mut cur_offset = 1;
        for indexer in self.round_indexers.iter().rev() {
            offsets.push(cur_offset);
            cur_offset *= indexer.size();
        }

        offsets.reverse();
        Some(indexes.iter().zip(offsets).map(|(a, b)| *a * b).sum())
    }

    /// Returns the size of the indexer. The maximum index is size-1
    pub fn size(&self) -> usize {
        self.round_indexers.iter().map(|x| x.size()).product()
    }
}

fn custom_deck_indexer(deck: Deck, cards_per_round: &[usize]) -> RoundIndexer<ActionVec> {
    let iterator = DealEnumerationIterator::new(deck, cards_per_round).map(deal_to_actions);
    RoundIndexer::new(iterator)
}

fn choice_indexer(mut choices: Vec<ActionVec>) -> RoundIndexer<ActionVec> {
    choices.sort();
    choices.dedup();
    RoundIndexer::new(choices.into_iter())
}

fn euchre_indexer(cards_per_round: &[usize]) -> RoundIndexer<ActionVec> {
    let iterator =
        IsomorphicDealIterator::new(cards_per_round, crate::cards::iterators::DeckType::Euchre)
            .map(deal_to_actions);
    RoundIndexer::new(iterator)
}

fn deal_to_actions(deal: [CardSet; 5]) -> ActionVec {
    let mut actions = SmallVec::new();

    for round in deal {
        for card in round {
            actions.push(card.index() as Action);
        }
    }

    actions
}

#[cfg(test)]
mod tests {

    use super::*;

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
