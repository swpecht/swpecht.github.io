use std::collections::HashSet;

use rand::{seq::SliceRandom, thread_rng};

use crate::{
    algorithms::cfres::{DepthChecker, EuchreDepthChecker},
    game::{
        euchre::{
            actions::{Card, EAction},
            util::generate_face_up_deals,
        },
        Action, GameState,
    },
    istate::IStateKey,
};

pub fn collect_istates(
    istates: &mut HashSet<Vec<Action>>,
    samples: usize,
    face_up: Card,
    max_cards_played: usize,
) {
    let checker = EuchreDepthChecker { max_cards_played };
    let mut actions = Vec::new();

    for _ in 0..samples {
        let mut gs = generate_face_up_deals(face_up);
        while !checker.is_max_depth(&gs) {
            let istate = gs.istate_key(gs.cur_player());
            istates.insert(istate.to_vec());
            gs.legal_actions(&mut actions);
            let a = actions.choose(&mut thread_rng()).unwrap();
            gs.apply_action(*a);
        }
    }
}

fn translate_euchre_key(key: IStateKey) -> IStateKey {
    // All of our keys have the NS as the baseline faceup card
    let baseline_card: Action = EAction::from(Card::NS).into();
    let shard = get_euchre_shard(&key);

    let mut new_key = IStateKey::default();
    key.iter().for_each(|a| {
        let new_a = match a {
            x if *x == shard => baseline_card,
            x if *x == baseline_card => shard,
            x => *x,
        };
        new_key.push(new_a);
    });

    new_key
}

/// We shard the istates by the face up card, for euchre, this is the action at position
/// 20 in the istatekey.
fn get_euchre_shard(key: &IStateKey) -> Action {
    const FACE_UP_INDEX: usize = 20;
    *key.get(FACE_UP_INDEX)
        .expect("only support full deals of cards")
}

#[cfg(test)]
mod tests {}
