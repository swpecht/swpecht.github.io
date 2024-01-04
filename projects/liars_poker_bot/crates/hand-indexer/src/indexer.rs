use games::{istate::IStateKey, Action};
use itertools::Itertools;
use smallvec::SmallVec;

use crate::{
    cards::{cardset::CardSet, iterators::DealEnumerationIterator, Deck},
    phf::RoundIndexer,
};

/// Performant type for storing a varying number of actions
pub type ActionVec = SmallVec<[Action; 20]>;

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
    /// TODO: have this try to match the longest possible collection first
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
                    if c[..] == actions[..c.len()]
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
                RoundType::Euchre { cards_per_round } => todo!(),
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

    pub fn kuhn_poker() -> Self {
        let deck = Deck::kuhn_poker();

        use games::gamestates::kuhn_poker::KPAction::*;
        use RoundType::*;
        GameIndexer::new(vec![
            CustomDeck {
                deck,
                cards_per_round: vec![1],
            },
            Choice {
                choices: vec![
                    vec![Pass.into()].into(),
                    vec![Pass.into(), Pass.into()].into(),
                    vec![Pass.into(), Bet.into()].into(),
                    vec![Pass.into(), Bet.into(), Pass.into()].into(),
                    vec![Pass.into(), Bet.into(), Bet.into()].into(),
                ],
            },
        ])
    }

    /// Calculates the index for a given IStateKey
    ///
    /// This function will perform all necessary istate normalization.
    ///
    /// IStates with the same starting rounds are grouped near each other.
    pub fn index(&self, istate: IStateKey) -> Option<usize> {
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
            cur_offset *= indexer.size();
            offsets.push(cur_offset);
        }

        offsets.reverse();
        Some(indexes.iter().zip(offsets).map(|(a, b)| *a * b).sum())
    }

    fn round_index(&self, key_part: &[Action]) -> Option<usize> {
        todo!()
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

fn choice_indexer(choices: Vec<ActionVec>) -> RoundIndexer<ActionVec> {
    RoundIndexer::new(choices.into_iter())
}

fn deal_to_actions(deal: [CardSet; 5]) -> ActionVec {
    let mut actions = SmallVec::new();

    for round in deal {
        for card in round {
            actions.push(Action(card.index() as u8));
        }
    }

    actions
}

#[cfg(test)]
mod tests {

    use std::collections::HashSet;

    use games::{gamestates::kuhn_poker::KuhnPoker, GameState};
    use itertools::Itertools;
    use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

    use super::*;

    #[test]
    fn test_kuhn_poker() {
        let indexer = GameIndexer::kuhn_poker();
        // 1st card: 3 options
        // P
        // PP
        // PB
        // PBB
        // PBP
        // 3 *  5
        assert_eq!(indexer.size(), 15);

        let mut indexes = HashSet::new();
        let mut rng: StdRng = SeedableRng::seed_from_u64(42);

        let mut actions = Vec::new();
        for _ in 0..10000 {
            let mut gs = KuhnPoker::new_state();

            while !gs.is_terminal() {
                if !gs.is_chance_node() {
                    let istate = gs.istate_key(gs.cur_player());
                    let idx = indexer
                        .index(istate)
                        .unwrap_or_else(|| panic!("failed to index: {}, {:?}", gs, istate));
                    indexes.insert(idx);
                }

                gs.legal_actions(&mut actions);
                let a = *actions.choose(&mut rng).unwrap();
                gs.apply_action(a);
            }
        }

        assert_eq!(
            indexes.into_iter().sorted().collect_vec(),
            (0..30).collect_vec()
        );
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
