use tinyvec::ArrayVec;

use crate::{istate::IStateKey, translate_istate};

use super::{
    actions::{EAction, Suit},
    ismorphic::{normalize_euchre_istate, EuchreNormalizer},
    EPhase,
};

use EAction::*;
const CARDS: [EAction; 24] = [
    NC, TC, JC, QC, KC, AC, NS, TS, JS, QS, KS, AS, NH, TH, JH, QH, KH, AH, ND, TD, JD, QD, KD, AD,
];

const SPADES: [EAction; 6] = [NS, TS, JS, QS, KS, AS];

const MAX_ACTIONS: usize = 24;

#[derive(Clone)]
pub struct EuchreIsomorphicIStateIterator {
    stack: Vec<EuchreIState>,
    max_cards_played: usize,
}

impl EuchreIsomorphicIStateIterator {
    pub fn new(max_cards_played: usize) -> Self {
        if max_cards_played > 4 {
            panic!("only support istates for the first trick");
        }

        let stack = vec![EuchreIState::default()];
        Self {
            stack,
            max_cards_played,
        }
    }

    fn next_unfiltered(&mut self) -> Option<EuchreIState> {
        let state = self.stack.pop()?;

        // Special case to populate discard states, these are always present even if 0 cards played
        if state.actions.last() == Some(&EAction::Pickup) {
            let mut ns = state;
            ns.apply_action(EAction::DiscardMarker);
            self.stack.push(ns);
        }

        if !(state.cards_played() > self.max_cards_played && matches!(state.phase(), EPhase::Play))
        {
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
                let key = state.key();
                let norm_key = normalize_euchre_istate(&key);

                // skip returning anything not in isomorphic form
                if key == norm_key {
                    return Some(state.key());
                }
            }
        }

        None
    }
}

impl ExactSizeIterator for EuchreIsomorphicIStateIterator {
    fn len(&self) -> usize {
        todo!()
    }
}

pub struct EuchreIsomorphicIStates {
    max_cards_played: usize,
}

impl EuchreIsomorphicIStates {
    pub fn new(max_cards_played: usize) -> Self {
        Self { max_cards_played }
    }
}

impl IntoIterator for EuchreIsomorphicIStates {
    type Item = IStateKey;

    type IntoIter = EuchreIsomorphicIStateIterator;

    fn into_iter(self) -> Self::IntoIter {
        EuchreIsomorphicIStateIterator::new(self.max_cards_played)
    }
}

/// Helper struct for enumerating euchre istates
#[derive(Default, Clone, Copy)]
struct EuchreIState {
    actions: ArrayVec<[EAction; 20]>,
}

impl EuchreIState {
    pub fn new(history: &[EAction]) -> Self {
        let mut actions = ArrayVec::new();
        for &a in history.iter() {
            actions.push(a);
        }
        Self { actions }
    }

    /// Uses the resampling logic to check if the current istate is valid -- or only return actions which can be valid, tbd
    /// the istate logic will be more robust and avoid some extra invalid states, tbd if perf tradeoff is worth it
    fn is_valid(&self) -> bool {
        // todo!()
        true
    }

    fn apply_action(&mut self, a: EAction) {
        if self.actions.len() >= 20 {
            panic!(
                "attempting to create an istate larger than storage: {:?}\nPhase: {:?}",
                self.actions,
                self.phase()
            )
        }
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
            EPhase::Pickup => self.legal_actions_pickup(actions),
            EPhase::Discard => self.legal_actions_discard(actions),
            EPhase::ChooseTrump => self.legal_actions_choose_trump(actions),
            EPhase::Play => self.legal_actions_play(actions),
        }
    }

    /// Only allow dealing cards > cards that were previously dealt
    fn legal_actions_deal_hand(&self, actions: &mut ArrayVec<[EAction; MAX_ACTIONS]>) {
        actions.set_len(CARDS.len());
        actions.copy_from_slice(&CARDS);

        if let Some(max_dealt) = self.actions.iter().max() {
            actions.retain(|x| x > max_dealt);
        }
    }

    /// Return all undealt spades
    fn legal_actions_deal_face_up(&self, actions: &mut ArrayVec<[EAction; MAX_ACTIONS]>) {
        actions.set_len(SPADES.len());
        actions.copy_from_slice(&SPADES);
        actions.retain(|x| !self.actions.contains(x));
    }

    fn legal_actions_pickup(&self, actions: &mut ArrayVec<[EAction; MAX_ACTIONS]>) {
        actions.push(EAction::Pass);
        actions.push(EAction::Pickup);
    }

    fn legal_actions_discard(&self, actions: &mut ArrayVec<[EAction; MAX_ACTIONS]>) {
        // Can discard any of the cards in hand or the face up card
        for card in &self.actions[0..6] {
            actions.push(*card);
        }
    }

    fn legal_actions_choose_trump(&self, actions: &mut ArrayVec<[EAction; MAX_ACTIONS]>) {
        // Can only pass if we're not the dealer on the last time around
        if self.actions.iter().filter(|&&x| x == EAction::Pass).count() <= 7 {
            actions.push(EAction::Pass);
        }

        // Can't call the face up suit
        let face_up = self.actions[5].card().suit();
        if face_up != Suit::Spades {
            actions.push(EAction::Spades);
        }
        if face_up != Suit::Clubs {
            actions.push(EAction::Clubs);
        }
        if face_up != Suit::Hearts {
            actions.push(EAction::Hearts);
        }
        if face_up != Suit::Diamonds {
            actions.push(EAction::Diamonds);
        }
    }

    /// Returns the legal actions for playing
    fn legal_actions_play(&self, actions: &mut ArrayVec<[EAction; MAX_ACTIONS]>) {
        // Can play any card that's not in our hand or the face up card
        for card in CARDS {
            if !self.actions[0..6].contains(&card) {
                actions.push(card);
            }
        }
    }

    fn phase(&self) -> EPhase {
        if self.actions.len() < 5 {
            return EPhase::DealHands;
        } else if self.actions.len() == 5 {
            return EPhase::DealFaceUp;
        } else if *self.actions.last().unwrap() == EAction::DiscardMarker {
            return EPhase::Discard;
        } else if self.actions.contains(&EAction::Pickup)
            || self.actions.contains(&EAction::Clubs)
            || self.actions.contains(&EAction::Spades)
            || self.actions.contains(&EAction::Hearts)
            || self.actions.contains(&EAction::Diamonds)
                && self.actions.last().unwrap() != &EAction::DiscardMarker
        {
            return EPhase::Play;
        } else if self.actions.len() >= 6 && self.actions.len() < 10 {
            return EPhase::Pickup;
        } else if self.actions.len() >= 10 {
            return EPhase::ChooseTrump;
        }

        panic!(
            "invalid state: {:?}",
            translate_istate!(self.actions, EAction)
        )
    }

    fn key(&self) -> IStateKey {
        let mut key = IStateKey::default();
        for a in self.actions {
            // don't include the discard marker
            if a != EAction::DiscardMarker {
                key.push(a.into());
            }
        }

        // add the discard marker back if it's the last action
        if *self.actions.last().unwrap() == EAction::DiscardMarker {
            key.push(EAction::DiscardMarker.into());
        }

        key
    }

    fn cards_played(&self) -> usize {
        use EAction::*;
        self.actions
            .iter()
            .position(|x| matches!(x, Pickup | Spades | Clubs | Hearts | Diamonds))
            .map(|x| self.actions.len() - x)
            .unwrap_or(0)
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
    use rand::{seq::IteratorRandom, thread_rng};
    use tinyvec::array_vec;

    use crate::translate_istate;

    use super::*;

    #[test]
    fn test_euchre_deal_istates() {
        // let mut iterator = EuchreIsomorphicIStateIterator::new(0);
        // assert!(iterator.any(|x| *x.last().unwrap() == EAction::DiscardMarker.into()));

        // use EAction::*;
        // let istate = EuchreIState::new(&[NC, NS, KS, TD, JD, TS, Pickup, DiscardMarker]);
        // assert_eq!(translate_istate!(istate.key(), EAction), vec![]);

        // let mut actions = ArrayVec::new();
        // istate.legal_actions(&mut actions);
        // assert_eq!(actions, array_vec!());

        let iterator = EuchreIsomorphicIStateIterator::new(1);

        for state in iterator.clone().choose_multiple(&mut thread_rng(), 100) {
            println!("{:?}", translate_istate!(state, EAction))
        }

        // todo: find the right number
        // 201_894
        assert_eq!(iterator.count(), 100);
        todo!()
    }
}
