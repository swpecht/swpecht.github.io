use std::collections::HashSet;

use games::{
    gamestates::kuhn_poker::{KPAction, KuhnPoker},
    Action, GameState,
};
use hand_indexer::{
    cards::{Card, Deck},
    indexer::GameIndexer,
};
use itertools::Itertools;
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

#[test]
fn kuhn_poker_indexer_test() {
    let indexer = kuhn_poker();
    // 1st card: 3 options
    // P
    // PP
    // PB
    // PBB
    // PBP
    // B
    // BP
    // BB
    // 3 *  8
    assert_eq!(indexer.size(), 24);

    indexer.index(&[4, 0]).unwrap();

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
        (0..30).collect_vec()
    );
}

pub fn kuhn_poker() -> GameIndexer {
    use games::gamestates::kuhn_poker::KPAction::*;
    use hand_indexer::indexer::RoundType::*;

    let card_choices = vec![vec![Jack], vec![Queen], vec![King]]
        .into_iter()
        .map(|x| {
            x.into_iter()
                .map(|y| u8::from(Action::from(y)))
                .collect_vec()
                .into()
        })
        .collect_vec();

    let bet_choices = vec![
        vec![Pass],
        vec![Pass, Pass],
        vec![Pass, Bet],
        vec![Pass, Bet, Pass],
        vec![Pass, Bet, Bet],
        vec![Bet],
        vec![Bet, Bet],
        vec![Pass, Pass],
    ]
    .into_iter()
    .map(|x| {
        x.into_iter()
            .map(|y| u8::from(Action::from(y)))
            .collect_vec()
            .into()
    })
    .collect_vec();

    GameIndexer::new(vec![
        Choice {
            choices: card_choices,
        },
        Choice {
            choices: bet_choices,
        },
    ])
}