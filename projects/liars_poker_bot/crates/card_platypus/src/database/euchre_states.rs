use std::collections::HashSet;

use games::{
    gamestates::euchre::{actions::Card, util::generate_face_up_deals},
    Action, GameState,
};
use rand::{seq::SliceRandom, thread_rng};

use crate::algorithms::cfres::{DepthChecker, EuchreDepthChecker};

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

#[cfg(test)]
mod tests {}
