use rand::{seq::SliceRandom, thread_rng};

use crate::{
    gamestates::euchre::{actions::EAction, EPhase, Euchre},
    GameState,
};

use super::{actions::Card, EuchreGameState};

/// Generator for games with arbitrary face up cards
pub fn generate_face_up_deals(face_up: Card) -> EuchreGameState {
    let mut gs = Euchre::new_state();
    let mut actions = Vec::new();
    for _ in 0..20 {
        gs.legal_actions(&mut actions);
        actions.retain(|&a| EAction::from(a).card() != face_up);
        let a = actions
            .choose(&mut thread_rng())
            .expect("error dealing cards");
        gs.apply_action(*a);
        actions.clear();
    }

    gs.apply_action(EAction::from(face_up).into());

    assert_eq!(gs.phase(), EPhase::Pickup);
    gs
}
