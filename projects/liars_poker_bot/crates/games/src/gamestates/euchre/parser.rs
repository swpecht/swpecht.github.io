use itertools::Itertools;

use crate::{
    gamestates::euchre::{
        actions::{Card, EAction},
        EPhase, Euchre,
    },
    Action, GameState,
};

use super::EuchreGameState;

impl From<&str> for EuchreGameState {
    fn from(value: &str) -> Self {
        let mut gs = Euchre::new_state();
        let mut action_buffer = String::new();
        let mut actions = Vec::new();

        for c in value.chars() {
            if c == '|' {
                continue;
            }

            action_buffer.push(c);
            let chars_per_action = match gs.phase() {
                EPhase::DealHands => 2,
                EPhase::DealFaceUp => 2,
                EPhase::Pickup => 1,
                EPhase::Discard => 2,
                EPhase::ChooseTrump => 1,
                EPhase::Play => 2,
            };

            if action_buffer.len() < chars_per_action {
                continue;
            }

            let a = match (gs.phase(), action_buffer.as_str()) {
                (EPhase::DealHands, x) => EAction::from(Card::from(x)),
                (EPhase::DealFaceUp, x) => EAction::from(Card::from(x)),
                (EPhase::Pickup | EPhase::ChooseTrump, "P") => EAction::Pass,
                (EPhase::Pickup, "T") => EAction::Pickup,
                (EPhase::Discard, x) => EAction::from(Card::from(x)),
                (EPhase::ChooseTrump, x) => match x {
                    "S" => EAction::Spades,
                    "C" => EAction::Clubs,
                    "H" => EAction::Hearts,
                    "D" => EAction::Diamonds,
                    _ => panic!("invalid suit: {}", x),
                },
                (EPhase::Play, x) => EAction::from(Card::from(x)),
                _ => panic!(
                    "invalid action: {} for phase: {:?}",
                    action_buffer,
                    gs.phase()
                ),
            };
            actions.clear();
            gs.legal_actions(&mut actions);
            assert!(
                actions.contains(&Action::from(a)),
                "Found an illegal action:
                    value: {}
                    str: {}
                    parsed action: {}
                    EAction: {}
                    legal actions: {:?}
                    gs: {}
                    debug: {:?}",
                value,
                action_buffer,
                Action::from(a),
                a,
                actions.iter().map(|x| EAction::from(*x)).collect_vec(),
                gs,
                gs
            );

            gs.apply_action(a.into());
            action_buffer.clear();
        }

        gs
    }
}
