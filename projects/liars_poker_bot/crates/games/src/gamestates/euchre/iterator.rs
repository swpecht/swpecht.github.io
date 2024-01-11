use tinyvec::ArrayVec;

use crate::istate::IStateKey;

use super::{actions::EAction, EPhase, EuchreGameState};

use EAction::*;
const CARDS: [EAction; 24] = [
    NC, TC, JC, QC, KC, AC, NS, TS, JS, QS, KS, AS, NH, TH, JH, QH, KH, AH, ND, TD, JD, QD, KD, AD,
];

const SPADES: [EAction; 6] = [NS, TS, JS, QS, KS, AS];

const MAX_ACTIONS: usize = 24;

pub struct EuchreIsomorphicIStateIterator {
    stack: Vec<EuchreIState>,
    is_max_depth: fn(&EuchreGameState) -> bool,
}

impl EuchreIsomorphicIStateIterator {
    pub fn new(is_max_depth: fn(&EuchreGameState) -> bool) -> Self {
        let stack = vec![EuchreIState::default()];
        Self {
            stack,
            is_max_depth,
        }
    }

    fn next_unfiltered(&mut self) -> Option<EuchreIState> {
        let state = self.stack.pop()?;
        if !(state.phase() == EPhase::Pickup) {
            let mut actions = ArrayVec::new();
            state.legal_actions(&mut actions);
            for a in actions {
                let mut ns = state;
                ns.apply_action(a);
                self.stack.push(ns);
            }
        }

        Some(state)
    }
}

impl Iterator for EuchreIsomorphicIStateIterator {
    type Item = IStateKey;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(state) = self.next_unfiltered() {
            // Don't want to return the chance nodes
            if state.is_valid() && !matches!(state.phase(), EPhase::DealHands | EPhase::DealFaceUp)
            {
                return Some(state.key());
            }
        }

        None
    }
}

/// Helper struct for enumerating euchre istates
#[derive(Default, Clone, Copy)]
struct EuchreIState {
    actions: ArrayVec<[EAction; 20]>,
}

impl EuchreIState {
    /// Uses the resampling logic to check if the current istate is valid
    fn is_valid(&self) -> bool {
        // todo!()
        true
    }

    fn apply_action(&mut self, a: EAction) {
        self.actions.push(a)
    }

    fn pop(&mut self) -> Option<EAction> {
        self.actions.pop()
    }

    fn legal_actions(&self, actions: &mut ArrayVec<[EAction; MAX_ACTIONS]>) {
        actions.clear();
        match self.phase() {
            EPhase::DealHands => self.legal_actions_deal_hand(actions),
            // only spades can be face up
            EPhase::DealFaceUp => self.legal_actions_deal_face_up(actions),
            EPhase::Pickup => todo!(),
            EPhase::Discard => {} // no valid actions from discard
            EPhase::ChooseTrump => todo!(),
            EPhase::Play => todo!(),
        }
    }

    /// Only allow dealing cards > cards that were previously dealt
    fn legal_actions_deal_hand(&self, actions: &mut ArrayVec<[EAction; 24]>) {
        actions.set_len(CARDS.len());
        actions.copy_from_slice(&CARDS);

        if let Some(max_dealt) = self.actions.iter().max() {
            actions.retain(|x| x > max_dealt);
        }
    }

    /// Return all undealt spades
    fn legal_actions_deal_face_up(&self, actions: &mut ArrayVec<[EAction; 24]>) {
        actions.set_len(SPADES.len());
        actions.copy_from_slice(&SPADES);
        actions.retain(|x| !self.actions.contains(x));
    }

    fn phase(&self) -> EPhase {
        if self.actions.len() < 5 {
            return EPhase::DealHands;
        } else if self.actions.len() == 5 {
            return EPhase::DealFaceUp;
        } else if *self.actions.last().unwrap() == EAction::Pickup {
            return EPhase::Discard;
        } else if self.actions.len() >= 6 {
            return EPhase::Pickup;
        }

        // todo: how to handle discard marker case

        todo!()
    }

    fn key(&self) -> IStateKey {
        let mut key = IStateKey::default();
        for a in self.actions {
            key.push(a.into());
        }
        key
    }
}

/// Do this is stages
///
/// Is there a way to see what the valid actions are for extending a given istate? We can use the logic to check if resampling is possible?
/// Use the "search for deal" function
/// Then we can just try all possible euchre actions and see which ones are valid?
///
/// for a in all_actions {
///     istate.append(a)
///     if istate.is_valid() { // re-sample logic
///         save(istate)
///         find_all(istate)
///     }
///     istate.pop(a)
/// }
///
/// Can use the EuchreIState function for this, rather than needing to construct a gamestate -- we can pull the constraints from it
/// should be pretty simple since only doing the first round of play
///
/// Start with an empty one, then slowly append -- might need to re-create some of the logic for phase changes, but should be minimal
///
/// Make a naive gamestate iterator -- for bluff, and kuhn poker, just go over all actions
/// Then make a euchre specific one that has all the optimizations, base it on validating istates using the re-sample logic
///
/// Consider re-writing based on the sudoku solver example -- constraint propogation
///
/// Re-use the isomorphism logic from euchre istates here to check if they are isomorphic
pub fn find_child_istates() {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_euchre_deal_istates() {
        let iterator = EuchreIsomorphicIStateIterator::new(|x| x.phase() == EPhase::Pickup);
        // todo: find the right number
        // 201_894
        assert_eq!(iterator.count(), 100);
        todo!()
    }
}
