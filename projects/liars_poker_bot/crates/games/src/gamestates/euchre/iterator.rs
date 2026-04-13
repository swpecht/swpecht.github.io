use itertools::Itertools;
use tinyvec::ArrayVec;

use crate::istate::IStateKey;

use super::{
    actions::{EAction, Suit, ALL_CARDS},
    isomorphic::normalize_euchre_istate,
    EPhase,
};

use EAction::*;
const SPADES: u32 = NS as u32 | TS as u32 | JS as u32 | QS as u32 | KS as u32 | AS as u32;
const NON_CARD_ACTIONS: u32 = Pass as u32
    | Pickup as u32
    | Alone as u32
    | DiscardMarker as u32
    | Spades as u32
    | Clubs as u32
    | Hearts as u32
    | Diamonds as u32;

/// Mask of set of actions
#[derive(Default, Clone, Copy)]
struct ActionSet(u32);

impl ActionSet {
    pub fn from_mask(mask: u32) -> Self {
        ActionSet(mask)
    }

    pub fn add(&mut self, a: EAction) {
        self.0 |= a as u32;
    }

    pub fn remove(&mut self, a: EAction) {
        self.0 &= !(a as u32);
    }

    pub fn contains(&self, a: EAction) -> bool {
        self.0 & a as u32 > 0
    }

    pub fn pop(&mut self) -> Option<EAction> {
        if self.0.count_ones() == 0 {
            return None;
        }

        let a = EAction::from(1 << self.0.trailing_zeros());
        self.remove(a);
        Some(a)
    }

    pub fn and(self, other: Self) -> Self {
        ActionSet(self.0 & other.0)
    }

    /// Removes all items lower than the highest set bit in other
    pub fn remove_lower(&mut self, other: ActionSet) {
        let max = 32 - other.0.leading_zeros();
        self.0 &= !0 << max;
    }
}

impl Iterator for ActionSet {
    type Item = EAction;

    fn next(&mut self) -> Option<Self::Item> {
        self.pop()
    }
}

#[derive(Clone)]
pub struct EuchreIsomorphicIStateIterator {
    stack: Vec<EuchreIState>,
    max_cards_played: usize,
    face_up_cards: ActionSet,
}

impl EuchreIsomorphicIStateIterator {
    pub fn new(max_cards_played: usize) -> Self {
        EuchreIsomorphicIStateIterator::with_face_up(max_cards_played, &[NS, TS, JS, QS, KS, AS])
    }

    /// Return an iterator that only includes the provided face up cards, useful for sharding as
    /// deals with different face up cards are independent
    pub fn with_face_up(max_cards_played: usize, face_up_cards: &[EAction]) -> Self {
        if max_cards_played > 4 {
            panic!("only support istates for the first trick. see notes for assumptions that break if go to second trick");
        }

        assert!(
            face_up_cards
                .iter()
                .all(|x| x.card().suit() == Suit::Spades),
            "must provide only spades as face up cards"
        );

        let mut cards = face_up_cards.to_vec();
        cards.sort();
        cards.dedup();
        assert_eq!(
            cards.len(),
            face_up_cards.len(),
            "duplicate cards cannot be provided for face up cards"
        );

        let mut face_up_set = ActionSet::default();
        face_up_cards.iter().for_each(|c| face_up_set.add(*c));

        let stack = vec![EuchreIState::default()];
        Self {
            stack,
            max_cards_played,
            face_up_cards: face_up_set,
        }
    }

    fn next_unfiltered(&mut self) -> Option<EuchreIState> {
        let state = loop {
            let candidate = self.stack.pop()?;

            // Special case to populate discard states, these are always present even if 0 cards played
            if candidate.actions.last() == Some(&EAction::Pickup) {
                // Dealer view: push the DiscardMarker child (later expands into the
                // dealer's discard + alone decision + play sequence).
                let mut ns = candidate;
                ns.apply_action(EAction::DiscardMarker);
                self.stack.push(ns);

                // Non-dealer trump caller view: the discard card is hidden, so the
                // istate goes straight from Pickup to the alone decision. Push both
                // alone branches so the indexer covers post-Alone Play istates for
                // the trump caller.
                let mut ns = candidate;
                ns.apply_action(EAction::Alone);
                self.stack.push(ns);
                let mut ns = candidate;
                ns.apply_action(EAction::Pass);
                self.stack.push(ns);
            }

            // special case to populate plays for dealer state if going to 4 players
            // need this since the state would otherwise be skipped below
            if candidate.has_discard_action
                && self.max_cards_played >= 4
                && candidate.cards_played() < self.max_cards_played
            {
                let actions = candidate.legal_actions();
                for a in actions {
                    let mut ns = candidate;
                    ns.apply_action(a);
                    self.stack.push(ns);
                }
            }

            // todo: figure out how to handle the discard children
            let is_alone = candidate.phase() == EPhase::Alone;
            let skip = (matches!(candidate.phase(), EPhase::Play)
                && candidate.cards_played() >= self.max_cards_played)
                // we don't need to expand discard action states unless going to 4 cards played
                || (candidate.has_discard_action && self.max_cards_played < 4 && !is_alone)
                || (candidate.has_discard_action && candidate.cards_played() < 4 && !is_alone);

            if !skip {
                break candidate;
            }
        };

        // Don't expand all states, this help avoid some pressure on allocator
        let expand_istate = (self.max_cards_played == 0 // Always expand if 0 cards played since we want to get the discard states
            || state.cards_played() < self.max_cards_played) // otherwise we only expand if the child state won't be more than the max cards played
            // Post-Pickup states are expanded only via the DiscardMarker special case
            // above. Running the regular expansion would create malformed Alone-phase
            // children (no marker, has_discard_action=false) that duplicate keys
            // produced through the marker path.
            && state.actions.last() != Some(&EAction::Pickup);

        if expand_istate {
            let actions = state.legal_actions();
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
            // skip istates no in the face up card set
            if let Some(face_up) = state.actions.get(5) {
                if !self.face_up_cards.contains(*face_up) {
                    continue;
                }
            }

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

/// Helper struct for enumerating euchre istates
#[derive(Clone, Copy)]
struct EuchreIState {
    actions: ArrayVec<[EAction; 32]>,
    /// Mask of all the played actions
    played_actions: ActionSet,
    /// Mask of all unplayed cards
    undealt_cards: ActionSet,
    // tracks if this is a dealer istate that has a discard action
    has_discard_action: bool,
    // explicitly track the current phase
    cur_phase: EPhase,
}

impl Default for EuchreIState {
    fn default() -> Self {
        Self {
            actions: Default::default(),
            played_actions: Default::default(),
            undealt_cards: ActionSet::from_mask(ALL_CARDS),
            has_discard_action: Default::default(),
            cur_phase: EPhase::DealHands,
        }
    }
}

impl EuchreIState {
    /// Uses the resampling logic to check if the current istate is valid -- or only return actions which can be valid, tbd
    /// the istate logic will be more robust and avoid some extra invalid states, tbd if perf tradeoff is worth it
    fn is_valid(&self) -> bool {
        // todo!()
        true
    }

    /// Count Pass actions that occurred during the Pickup phase
    fn pickup_pass_count(&self) -> usize {
        self.actions
            .iter()
            .rev()
            .take_while(|a| **a == EAction::Pass)
            .count()
    }

    fn apply_action(&mut self, a: EAction) {
        if self.actions.len() >= 32 {
            panic!(
                "attempting to create an istate larger than storage: {:?}\nPhase: {:?}",
                self.actions,
                self.phase()
            )
        }

        // Remove the discard marker
        if self
            .actions
            .last()
            .map_or(false, |x| matches!(x, EAction::DiscardMarker))
        {
            self.actions.pop();
            self.has_discard_action = true;
        }

        // Update phase based on the action and current phase
        self.cur_phase = match (self.cur_phase, a) {
            (EPhase::DealHands, _) if self.actions.len() == 4 => EPhase::DealFaceUp,
            (EPhase::DealHands, _) => EPhase::DealHands,
            (EPhase::DealFaceUp, _) => EPhase::Pickup,
            (EPhase::Pickup, EAction::Pass) if self.pickup_pass_count() == 3 => EPhase::ChooseTrump, // 4th pass
            (EPhase::Pickup, EAction::Pass) => EPhase::Pickup,
            (EPhase::Pickup, EAction::Pickup) => EPhase::Discard,
            (EPhase::Discard, EAction::DiscardMarker) => EPhase::Discard, // marker, phase unchanged
            // Non-dealer trump caller's view: discard card is hidden, so applying
            // Alone/Pass directly from a post-Pickup state means the alone decision
            // has been made and we're entering Play.
            (EPhase::Discard, EAction::Alone) => EPhase::Play,
            (EPhase::Discard, EAction::Pass) => EPhase::Play,
            (EPhase::Discard, _) => EPhase::Alone, // discard card → alone decision (dealer view)
            (EPhase::ChooseTrump, EAction::Pass) => EPhase::ChooseTrump,
            (EPhase::ChooseTrump, _) => EPhase::Alone, // suit call → alone decision
            (EPhase::Alone, _) => EPhase::Play,
            (EPhase::Play, _) => EPhase::Play,
            _ => panic!("unexpected action {:?} in phase {:?}", a, self.cur_phase),
        };

        self.played_actions.add(a);
        self.undealt_cards.remove(a); // ok if not a card actions since removing
        self.actions.push(a)
    }

    fn legal_actions(&self) -> ActionSet {
        match self.phase() {
            EPhase::DealHands => self.legal_actions_deal_hand(),
            // only spades can be face up
            EPhase::DealFaceUp => self.legal_actions_deal_face_up(),
            EPhase::Pickup => self.legal_actions_pickup(),
            EPhase::Discard => self.legal_actions_discard(),
            EPhase::ChooseTrump => self.legal_actions_choose_trump(),
            EPhase::Alone => ActionSet::from_mask(EAction::Alone as u32 | EAction::Pass as u32),
            EPhase::Play => self.legal_actions_play(),
        }
    }

    /// Only allow dealing cards > cards that were previously dealt
    fn legal_actions_deal_hand(&self) -> ActionSet {
        let mut actions = self.undealt_cards;
        let dealt_cards = self.played_actions.and(ActionSet::from_mask(ALL_CARDS));
        actions.remove_lower(dealt_cards);
        actions
    }

    /// Return all undealt spades
    fn legal_actions_deal_face_up(&self) -> ActionSet {
        let spades = ActionSet::from_mask(SPADES);
        spades.and(self.undealt_cards)
    }

    fn legal_actions_pickup(&self) -> ActionSet {
        ActionSet::from_mask(EAction::Pass as u32 | EAction::Pickup as u32)
    }

    fn legal_actions_discard(&self) -> ActionSet {
        let mut actions = ActionSet::default();
        // Can discard any of the cards in hand or the face up card
        for card in &self.actions[0..6] {
            actions.add(*card);
        }
        actions
    }

    fn legal_actions_choose_trump(&self) -> ActionSet {
        let mut actions = ActionSet::default();
        // Can only pass if we're not the dealer on the last time around
        if self.actions.iter().filter(|&&x| x == EAction::Pass).count() <= 7 {
            actions.add(EAction::Pass);
        }

        // Can't call the face up suit
        let face_up = self.actions[5].card().suit();
        if face_up != Suit::Spades {
            actions.add(EAction::Spades);
        }
        if face_up != Suit::Clubs {
            actions.add(EAction::Clubs);
        }
        if face_up != Suit::Hearts {
            actions.add(EAction::Hearts);
        }
        if face_up != Suit::Diamonds {
            actions.add(EAction::Diamonds);
        }

        actions
    }

    /// Returns the legal actions for playing
    fn legal_actions_play(&self) -> ActionSet {
        self.undealt_cards
    }

    fn phase(&self) -> EPhase {
        // Special case: DiscardMarker hasn't been consumed yet
        if self
            .actions
            .last()
            .is_some_and(|x| matches!(x, EAction::DiscardMarker))
        {
            return EPhase::Discard;
        }
        self.cur_phase
    }

    fn key(&self) -> IStateKey {
        IStateKey::copy_from_slice(&self.actions.iter().map(|x| x.into()).collect_vec())
    }

    fn cards_played(&self) -> usize {
        // counts all the cards that have been seen. Since each card can only be seen when dealt, played, or discarded (which is a dealt card), we
        // can subtrace 6 to see how many played cards there are
        (self.played_actions.0 & !NON_CARD_ACTIONS)
            .count_ones()
            .max(6) as usize
            - 6
    }
}

#[cfg(test)]
mod tests {

    use rand::{seq::IteratorRandom, rng};

    use crate::translate_istate;

    use super::*;

    #[test]
    fn test_euchre_istate_iterator() {
        let iterator = EuchreIsomorphicIStateIterator::with_face_up(1, &[EAction::NS]);

        for state in iterator.clone().sample(&mut rng(), 100) {
            println!("{:?}", translate_istate!(state, EAction))
        }

        use EAction::*;

        // Validate states are generated for both 0 and 1 cards played
        let iterator = EuchreIsomorphicIStateIterator::with_face_up(0, &[EAction::NS]);
        assert!(iterator.count() > 0, "should produce states for 0 cards played");

        let iterator = EuchreIsomorphicIStateIterator::with_face_up(1, &[EAction::NS]);
        assert!(iterator.count() > 0, "should produce states for 1 card played");

        // Validate overall counts
        let iterator = EuchreIsomorphicIStateIterator::with_face_up(0, &[EAction::NS]);
        let c0 = iterator.count();
        let iterator = EuchreIsomorphicIStateIterator::new(0);
        let ca = iterator.count();
        let iterator = EuchreIsomorphicIStateIterator::with_face_up(1, &[EAction::NS]);
        let c1 = iterator.count();
        eprintln!("c0={} ca={} c1={}", c0, ca, c1);
        assert_eq!(ca, c0 * 6);
    }
}
