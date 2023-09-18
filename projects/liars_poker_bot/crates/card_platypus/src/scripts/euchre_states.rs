use std::{collections::HashSet, default};

use card_platypus::{
    game::{euchre::actions::EAction, Action},
    istate::IStateKey,
};

use itertools::Itertools;
use EAction::*;
const FACE_UP: &[EAction] = &[NS, TS, JS, QS, KS, AS];
const DEAL: &[EAction] = &[
    NC, TC, JC, QC, KC, AC, NS, TS, JS, QS, KS, AS, NH, TH, JH, QH, KH, AH, ND, TD, JD, QD, KD, AD,
];

#[derive(PartialEq, PartialOrd, Default, Clone, Copy, Debug)]
enum Termination {
    #[default]
    Deal,
    Pickup,
    Suit,
    Discard,
    Play {
        cards: usize,
    },
}

struct IStateBuilder {
    actions: Vec<EAction>,
    phase: Vec<Termination>,
}

impl IStateBuilder {
    fn undo(&mut self) {
        self.actions.pop();
        self.phase.pop();
    }

    fn apply(&mut self, action: EAction) {
        // make sure dealing cards in order
        if self.actions.len() > 1 && self.actions.len() < 6 {
            assert!(
                Action::from(*self.actions.last().unwrap()) < Action::from(action),
                "{:?}, {}",
                self.actions,
                action
            );
        }

        self.actions.push(action);
        use Termination::*;
        match (self.phase(), self.actions.len()) {
            (Deal, 0..=5) => self.phase.push(Deal),
            (Deal, 6) => self.phase.push(Pickup),
            (Pickup, _) if action == EAction::Pickup => self.phase.push(Discard),
            (Pickup, 7..=9) => self.phase.push(Pickup),
            (Pickup, 10) => self.phase.push(Suit),
            (Discard, _) => self.phase.push(Play { cards: 1 }),
            (Suit, _) if [Spades, Clubs, Hearts, Diamonds].contains(&action) => {
                self.phase.push(Play { cards: 1 })
            }
            (Suit, 11..=13) => self.phase.push(Suit),
            (_, _) => panic!(
                "Invalid state: {}, {:?}, {:?}",
                action, self.phase, self.actions
            ),
        };
    }

    fn istate(&self) -> IStateKey {
        if self.actions.len() < 6 {
            panic!("trying to generate istate on an incomplete deal")
        }

        let mut istate = IStateKey::default();
        let mut deal = self.actions[1..6]
            .iter()
            .map(|a| Action::from(*a))
            .collect_vec();
        deal.sort();
        deal.into_iter().for_each(|a| istate.push(a));

        istate.push(self.actions[0].into());

        if self.actions.len() == 6 {
            return istate;
        }

        for a in &self.actions[6..] {
            istate.push((*a).into());
        }

        if *self.actions.last().unwrap() == EAction::Pickup {
            istate.push(EAction::DiscardMarker.into());
        }

        istate
    }

    fn legal_actions(&self) -> Vec<EAction> {
        match self.phase() {
            Termination::Deal if self.actions.is_empty() => FACE_UP.to_vec(),
            Termination::Deal if self.actions.len() <= 5 => self.legal_actions_deal(),
            Termination::Pickup => vec![Pickup, Pass],
            Termination::Discard => self.actions[0..6].to_vec(),
            Termination::Suit => {
                // dealer is forced to choose a card
                if self.actions.len() < 13 {
                    vec![Pass, Clubs, Spades, Hearts, Diamonds]
                } else {
                    vec![Clubs, Spades, Hearts, Diamonds]
                }
            }
            _ => todo!(),
        }
    }

    fn legal_actions_deal(&self) -> Vec<EAction> {
        let mut deal_actions = DEAL.to_vec();
        deal_actions.retain(|x| !self.actions.contains(x));
        deal_actions.retain(|x| {
            self.actions[1..]
                .iter()
                .all(|b| Action::from(*b) < (*x).into())
        });
        deal_actions.sort_by_key(|x| Action::from(*x));
        deal_actions
    }

    fn phase(&self) -> Termination {
        *self.phase.last().unwrap()
    }
}

impl Default for IStateBuilder {
    fn default() -> Self {
        Self {
            actions: Default::default(),
            phase: vec![Termination::Deal],
        }
    }
}

fn generate_euchre_states(
    builder: &mut IStateBuilder,
    istates: &mut HashSet<IStateKey>,
    termination: Termination,
) {
    if builder.phase() >= termination {
        return;
    }

    // don't generate istates during the deal
    if builder.phase() > Termination::Deal {
        istates.insert(builder.istate());
    }

    let actions = builder.legal_actions();
    for a in actions {
        builder.apply(a);
        generate_euchre_states(builder, istates, termination);
        builder.undo();
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use card_platypus::game::euchre::actions::EAction;
    use itertools::Itertools;

    use crate::scripts::euchre_states::Termination;

    use super::{generate_euchre_states, IStateBuilder};

    #[test]
    fn test_istate_count_deal() {
        let mut builder = IStateBuilder::default();
        let mut istates = HashSet::new();
        generate_euchre_states(&mut builder, &mut istates, Termination::Pickup);
        // Should 6 (spades) * 23 choose 5 (hand)
        // assert_eq!(istates.len(), 201_894);

        let mut builder = IStateBuilder::default();
        let mut istates = HashSet::new();
        generate_euchre_states(&mut builder, &mut istates, Termination::Play { cards: 1 });
        assert_eq!(istates.len(), 201_894 * 12);

        // let mut builder = IStateBuilder::default();
        // let mut istates = HashSet::new();
        // generate_euchre_states(&mut builder, &mut istates, Termination::Play { cards: 4 });
        // assert_eq!(istates.len(), 201_894 * 12);
        // istates
        //     .iter()
        //     .take(100)
        //     .for_each(|x| println!("{:?}", x.iter().map(|a| EAction::from(*a)).collect_vec()));

        todo!("discard not visible to all players");
    }
}
