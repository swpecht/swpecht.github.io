use std::{collections::HashSet, vec};

use games::{
    gamestates::{
        euchre::{processors::post_cards_played, Euchre, EuchreGameState},
        kuhn_poker::{KPAction, KuhnPoker},
    },
    Action, GameState,
};
use hand_indexer::{
    cards::{Card, Deck},
    indexer::{self, GameIndexer},
};
use itertools::Itertools;
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

#[test]
fn kuhn_poker_indexer_test() {
    let indexer = kuhn_poker();
    // 1st card: 3 options
    // bets: 3 non-terminal options: P, B, PB
    // 3 *  3
    assert_eq!(indexer.size(), 9);

    let mut indexes = HashSet::new();
    let mut rng: StdRng = SeedableRng::seed_from_u64(42);

    let mut actions = Vec::new();
    for _ in 0..10000 {
        let mut gs = KuhnPoker::new_state();

        while !gs.is_terminal() {
            if !gs.is_chance_node() {
                let istate = gs.istate_key(gs.cur_player());
                let idx = indexer
                    .index(istate.as_bytes())
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
        (0..9).collect_vec()
    );
}

#[test]
fn bluff22_indexer_test() {
    let indexer = bluff22();
    assert_eq!(indexer.size(), 21);
    todo!("add second round")
}

#[test]
fn euchre_indexer_test() {
    let indexer = euchre();
    // todo on confirming this number
    assert_eq!(indexer.size(), 6_060_955_824);

    let mut rng: StdRng = SeedableRng::seed_from_u64(42);

    let mut actions = Vec::new();
    for _ in 0..1_000_000 {
        let mut gs = Euchre::new_state();

        while !gs.is_terminal() && !post_cards_played(&gs, 4) {
            if !gs.is_chance_node() {
                let istate = gs.istate_key(gs.cur_player());
                let idx = indexer
                    .index(istate.as_bytes())
                    .unwrap_or_else(|| panic!("failed to index: {}, {:?}", gs, istate));
            }

            gs.legal_actions(&mut actions);
            let a = *actions.choose(&mut rng).unwrap();
            gs.apply_action(a);
        }
    }

    todo!("implement the remaining tests and confirm the total index size")
}

#[test]
fn hold_em_indexer_test() {
    // test indexer for just pockets cards, then flop, etc.
    todo!("implement this test")
}

pub fn kuhn_poker() -> GameIndexer {
    use games::gamestates::kuhn_poker::KPAction::*;

    let card_choices = vec![vec![Jack], vec![Queen], vec![King]]
        .into_iter()
        .map(|x| {
            x.into_iter()
                .map(|y| u8::from(Action::from(y)))
                .collect_vec()
                .into()
        })
        .collect_vec();

    // only include non-terminal actions
    let bet_choices = vec![vec![Pass], vec![Pass, Bet], vec![Bet]]
        .into_iter()
        .map(|x| {
            x.into_iter()
                .map(|y| u8::from(Action::from(y)))
                .collect_vec()
                .into()
        })
        .collect_vec();

    use hand_indexer::indexer::RoundType::*;
    GameIndexer::new(vec![
        Choice {
            choices: card_choices,
        },
        Choice {
            choices: bet_choices,
        },
    ])
}

pub fn bluff22() -> GameIndexer {
    use games::gamestates::bluff::BluffActions::*;
    use games::gamestates::bluff::Dice::*;
    let rolls = vec![
        Roll(One),
        Roll(Two),
        Roll(Three),
        Roll(Four),
        Roll(Five),
        Roll(Wild),
    ]
    .into_iter()
    .map(|x| u8::from(Action::from(x)))
    .combinations_with_replacement(2)
    .map(|x| x.into())
    .collect_vec();

    use hand_indexer::indexer::RoundType::*;
    GameIndexer::new(vec![Choice { choices: rolls }])
}

pub fn euchre() -> GameIndexer {
    use games::gamestates::euchre::actions::EAction::*;

    let bids = vec![
        // 3 pickup-passes
        vec![Pass],
        vec![Pass, Pass],
        vec![Pass, Pass, Pass],
        // 4 pickup states
        vec![Pickup],
        vec![Pass, Pickup],
        vec![Pass, Pass, Pickup],
        vec![Pass, Pass, Pass, Pickup],
        // 4 discard options
        vec![Pickup, DiscardMarker],
        vec![Pass, Pickup, DiscardMarker],
        vec![Pass, Pass, Pickup, DiscardMarker],
        vec![Pass, Pass, Pass, Pickup, DiscardMarker],
        // 4 suit passes, 1 more than the pickup passes because we have the initiall empty states from passing all pickups
        vec![Pass, Pass, Pass, Pass],
        vec![Pass, Pass, Pass, Pass, Pass],
        vec![Pass, Pass, Pass, Pass, Pass, Pass],
        vec![Pass, Pass, Pass, Pass, Pass, Pass, Pass],
        // 16 suit passes
        vec![Pass, Pass, Pass, Pass, Spades],
        vec![Pass, Pass, Pass, Pass, Clubs],
        vec![Pass, Pass, Pass, Pass, Hearts],
        vec![Pass, Pass, Pass, Pass, Diamonds],
        vec![Pass, Pass, Pass, Pass, Pass, Spades],
        vec![Pass, Pass, Pass, Pass, Pass, Clubs],
        vec![Pass, Pass, Pass, Pass, Pass, Hearts],
        vec![Pass, Pass, Pass, Pass, Pass, Diamonds],
        vec![Pass, Pass, Pass, Pass, Pass, Pass, Spades],
        vec![Pass, Pass, Pass, Pass, Pass, Pass, Clubs],
        vec![Pass, Pass, Pass, Pass, Pass, Pass, Hearts],
        vec![Pass, Pass, Pass, Pass, Pass, Pass, Diamonds],
        vec![Pass, Pass, Pass, Pass, Pass, Pass, Pass, Spades],
        vec![Pass, Pass, Pass, Pass, Pass, Pass, Pass, Clubs],
        vec![Pass, Pass, Pass, Pass, Pass, Pass, Pass, Hearts],
        vec![Pass, Pass, Pass, Pass, Pass, Pass, Pass, Diamonds],
    ]
    .into_iter()
    .map(|x| {
        x.into_iter()
            .map(|y| Action::from(y).into())
            .collect_vec()
            .into()
    })
    .collect_vec();
    assert_eq!(bids.len(), 31);

    use hand_indexer::indexer::RoundType::*;
    GameIndexer::new(vec![
        Euchre {
            cards_per_round: vec![1, 5],
        },
        Choice { choices: bids },
        // For now, no constraints on this. We don't need to index
        // the 4th cards since we don't store it's istate
        Euchre {
            cards_per_round: vec![1, 1, 1],
        },
    ])
}
