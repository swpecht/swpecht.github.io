use std::fmt::Display;

use itertools::Itertools;

use serde::{Deserialize, Serialize};

use crate::{
    istate::IStateKey,
    {Action, Game, GameState, Player},
};

use self::{
    actions::{Card, EAction, Suit, CLUBS_MASK, DIAMONDS_MASK, HEART_MASK, SPADES_MASK},
    deck::{CardLocation, Deck, Hand},
    isomorphic::iso_deck,
};

pub(super) const CARDS_PER_HAND: usize = 5;

pub mod actions;
mod deck;
pub mod isomorphic;
pub mod iterator;
mod parser;
pub mod processors;
pub mod resample;
pub mod util;

pub struct Euchre {}
impl Euchre {
    pub fn new_state() -> EuchreGameState {
        EuchreGameState {
            num_players: 4,
            cur_player: 0,
            trump: None,
            trump_caller: 0,
            trick_winners: [0; 5],
            tricks_won: [0; 2],
            key: IStateKey::default(),
            play_order: Vec::new(),
            deck: Deck::default(),
            cards_played: 0,
            phase: EPhase::DealHands,
            going_alone: false,
        }
    }

    pub fn game() -> Game<EuchreGameState> {
        Game {
            new: Box::new(|| -> EuchreGameState { Self::new_state() }),
            max_players: 2,
            max_actions: 24, // 1 for each card dealt
        }
    }
}

/// We use Rc for the starting hand information since these values rarely change
/// and are consistent across all children of the given state
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct EuchreGameState {
    num_players: usize,
    trump: Option<Suit>,
    trump_caller: usize,
    cur_player: usize,
    /// keep track of who has won tricks to avoid re-computing
    trick_winners: [Player; 5],
    tricks_won: [u8; 2],
    key: IStateKey,
    play_order: Vec<Player>, // tracker of who went in what order. Last item is the current player
    deck: Deck,
    cards_played: usize,
    phase: EPhase,
    going_alone: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Copy, Serialize, Deserialize, Hash, Default)]
pub enum EPhase {
    #[default]
    DealHands,
    DealFaceUp,
    Pickup,
    /// The dealer has been told to pickup the trump suit
    Discard,
    ChooseTrump,
    /// Trump caller decides whether to go alone
    Alone,
    Play,
}

impl EuchreGameState {
    /// Returns players bid actions if they have taken them so far
    pub fn bids(&self) -> [Option<EAction>; 8] {
        let mut bids = [None; 8];
        let key = self.key();
        for (a, b) in key
            .iter()
            .skip(21)
            .take(8)
            .map(|x| EAction::from(*x))
            .zip(bids.iter_mut())
        {
            use EAction::*;
            match a {
                Pass | Pickup | Alone | Spades | Clubs | Diamonds | Hearts => *b = Some(a),
                _ => break,
            }
        }

        bids
    }

    pub fn trump_caller(&self) -> Option<Player> {
        match self.phase() {
            EPhase::Play | EPhase::Discard | EPhase::Alone => Some(self.trump_caller),
            _ => None,
        }
    }

    pub fn going_alone(&self) -> bool {
        self.going_alone
    }

    /// Number of players per trick. Always 4 — when going alone the sitting-out
    /// partner plays a Pass sentinel on their turn so the rotation stays uniform.
    pub fn players_per_trick(&self) -> usize {
        4
    }

    /// Returns the partner of the trump caller who sits out when going alone
    pub fn sitting_out_player(&self) -> Option<Player> {
        if self.going_alone {
            Some((self.trump_caller + 2) % 4)
        } else {
            None
        }
    }

    /// Simple 1-step rotation. The sitting-out partner still takes their turn
    /// but plays a Pass sentinel; no skipping.
    fn next_player(&self, player: Player) -> Player {
        (player + 1) % self.num_players
    }

    /// Returns true if the bidding phase just ended
    pub fn bidding_ended(&self) -> bool {
        self.phase() == EPhase::Discard || self.phase() == EPhase::Alone
    }

    /// Returns true if Play has just begun — phase has flipped from Alone to
    /// Play, but no card has been played yet. Used by the server to pause
    /// exactly once at the bidding→play boundary instead of pausing at each
    /// intermediate bidding sub-phase.
    pub fn play_phase_entered(&self) -> bool {
        self.phase() == EPhase::Play && self.cards_played == 0
    }

    /// Returns true if a trick is over. Returns false if a trick hasn't started yet
    pub fn is_trick_over(&self) -> bool {
        self.cards_played.is_multiple_of(4) && self.cards_played > 0
    }

    /// Returns the starter and the 4 entries of the most recently completed trick.
    /// Each entry is `Some(card)` for a real play or `None` for a sitting-out
    /// partner's Pass sentinel.
    pub fn last_trick(&self) -> Option<(Player, Vec<Option<Card>>)> {
        if self.cards_played < 4 {
            return None;
        }

        let cards_played_in_cur_trick = self.cards_played % 4;

        let sidx = self.key.len() - cards_played_in_cur_trick - 4;
        let mut trick = Vec::with_capacity(4);
        for i in 0..4 {
            trick.push(Self::action_to_trick_card(self.key[sidx + i]));
        }

        let trick_starter = self.play_order[sidx];

        Some((trick_starter, trick))
    }

    fn action_to_trick_card(a: Action) -> Option<Card> {
        match EAction::from(a) {
            EAction::Pass => None,
            ea => Some(ea.card()),
        }
    }

    /// Return all cards currently in a players hand
    pub fn get_hand(&self, player: Player) -> Vec<Card> {
        let player_loc = player.into();
        let hand = self.deck.get_all(player_loc);
        hand.cards()
    }

    pub fn trick_score(&self) -> [u8; 2] {
        self.tricks_won
    }

    /// Returns trump and who called it
    pub fn trump(&self) -> Option<(Suit, Player)> {
        self.trump.map(|suit| (suit, self.trump_caller))
    }

    /// Return the card played by the player for the current trick
    pub fn played_card(&self, player: Player) -> Option<Card> {
        self.deck.played(player)
    }

    /// Returns the displayed face up card, if it exists
    pub fn displayed_face_up_card(&self) -> Option<Card> {
        self.deck.face_up()
    }

    /// Returns the non-chance node history
    pub fn history(&self) -> Vec<(Player, EAction)> {
        if self.is_chance_node() {
            return Vec::new();
        }

        self.play_order
            .iter()
            .zip(self.key())
            .map(|(p, a)| (*p, EAction::from(a)))
            .collect_vec()
    }

    fn apply_action_deal_hands(&mut self, a: Action) {
        let card = EAction::from(a).card();

        if self.deck.get(card) != CardLocation::None {
            panic!(
                "attempted to deal {} which has already been dealt to {:?}",
                card,
                self.deck.get(card)
            )
        }
        self.deck.set(card, self.cur_player.into());

        if (self.key.len() + 1).is_multiple_of(CARDS_PER_HAND) {
            self.cur_player = (self.cur_player + 1) % self.num_players
        }

        if self.key.len() == 19 {
            self.phase = EPhase::DealFaceUp;
        }
    }

    fn apply_action_deal_face_up(&mut self, a: Action) {
        use EAction::*;
        if matches!(
            a.into(),
            NC | TC
                | JC
                | QC
                | KC
                | AC
                | NS
                | TS
                | JS
                | QS
                | KS
                | AS
                | NH
                | TH
                | JH
                | QH
                | KH
                | AH
                | ND
                | TD
                | JD
                | QD
                | KD
                | AD
        ) {
            let c = EAction::from(a).card();
            if self.deck.get(c) != CardLocation::None {
                panic!(
                    "attempting to deal a card that was already dealt: {}, {:?}",
                    c, self.deck
                );
            }
            self.deck.set(c, CardLocation::FaceUp);
            self.cur_player = 0;
            self.phase = EPhase::Pickup;
            return;
        }
        panic!("invalid deal face up action: {:?}", a)
    }

    fn apply_action_pickup(&mut self, a: Action) {
        match EAction::from(a) {
            EAction::Pass => {
                if self.cur_player == 3 {
                    self.phase = EPhase::ChooseTrump;
                    let face_up = self.face_up();
                    self.deck.set(
                        face_up.expect("can't call faceup before deal finished"),
                        CardLocation::None,
                    );
                }
                self.cur_player = (self.cur_player + 1) % self.num_players;
            }
            EAction::Pickup => {
                self.trump_caller = self.cur_player;
                let face_up = self
                    .face_up()
                    .expect("can't call faceup before deal finished");
                self.trump = Some(face_up.suit());
                self.cur_player = 3; // dealers turn
                self.deck.set(face_up, CardLocation::Player3);

                self.phase = EPhase::Discard;
            }
            _ => panic!(
                "invalid action, attempted to play {} during pickup phase",
                a
            ),
        }
    }

    fn apply_action_choose_trump(&mut self, a: Action) {
        let a = EAction::from(a);
        self.trump = match a {
            EAction::Clubs => Some(Suit::Clubs),
            EAction::Spades => Some(Suit::Spades),
            EAction::Hearts => Some(Suit::Hearts),
            EAction::Diamonds => Some(Suit::Diamonds),
            EAction::Pass => None,
            _ => panic!("invalid action"),
        };

        let face_up = self
            .face_up()
            .expect("can't call faceup before deal finished");
        if let Some(trump) = self.trump {
            // can't call the face up card as trump
            assert!(face_up.suit() != trump);
        }

        if a == EAction::Pass {
            self.cur_player += 1;
        } else {
            self.trump_caller = self.cur_player;
            self.cur_player = self.trump_caller;
            self.phase = EPhase::Alone;
        }
    }

    /// Can only be done by the dealer (player 3)
    fn apply_action_discard(&mut self, a: Action) {
        let discard = EAction::from(a).card();
        assert_eq!(
            self.deck.get(discard),
            CardLocation::Player3,
            "attempting to discard a card not in dealers hand: {}\n{:?}",
            discard,
            self.deck
        );
        self.deck.set(discard, CardLocation::None); // dealer
        self.cur_player = self.trump_caller;
        self.phase = EPhase::Alone;
    }

    fn apply_action_alone(&mut self, a: Action) {
        match EAction::from(a) {
            EAction::Alone => {
                self.going_alone = true;
            }
            EAction::Pass => {
                self.going_alone = false;
            }
            _ => panic!("invalid action during alone phase"),
        }
        // Eldest hand always leads the first trick; the sit-out partner (if any)
        // will play a Pass sentinel when their turn comes around.
        self.cur_player = 0;
        self.phase = EPhase::Play;
    }

    fn apply_action_play(&mut self, a: Action) {
        let ea = EAction::from(a);
        let played_card = match ea {
            EAction::Pass => {
                // Sit-out partner's sentinel play. Must actually be the sit-out player.
                assert_eq!(
                    self.sitting_out_player(),
                    Some(self.cur_player),
                    "Pass during Play is only legal for the sitting-out partner"
                );
                None
            }
            _ => {
                let card = ea.card();
                assert!(
                    self.deck.get_all(self.cur_player.into()).contains(card),
                    "Attempted to play card not in players hand"
                );
                // track the cards in play for isomorphic key
                self.deck.play(card, self.cur_player).unwrap();
                Some(card)
            }
        };

        self.cards_played += 1;

        let trick_over = self.cards_played.is_multiple_of(4);
        if trick_over {
            let trick = self
                .last_trick_with_entry(played_card)
                .expect("trick should be complete");
            let starter = self.next_player(self.cur_player);
            let winner = self.evaluate_trick(&trick, starter);
            self.cur_player = winner;

            // save the trick winner for later
            let trick_idx = self.cards_played / 4 - 1;
            self.trick_winners[trick_idx] = winner;
            self.tricks_won[winner % 2] += 1;

            // clear the played cards
            for i in 0..self.num_players {
                if let Some(c) = self.deck.played(i) {
                    self.deck.set(c, CardLocation::None);
                }
            }
        } else {
            self.cur_player = self.next_player(self.cur_player);
        }
    }

    /// Determine if current trick is over (all players have played)
    /// Also returns true if none have played
    fn is_start_of_trick(&self) -> bool {
        self.cards_played.is_multiple_of(4)
    }

    /// Build the 4-entry trick just completed using `final_entry` as the final play.
    /// Entries are `None` for sitting-out Pass sentinels.
    fn last_trick_with_entry(&self, final_entry: Option<Card>) -> Option<[Option<Card>; 4]> {
        if self.phase() != EPhase::Play || self.cards_played < 4 {
            return None;
        }

        let sidx = self.key.len() - 3;
        Some([
            Self::action_to_trick_card(self.key[sidx]),
            Self::action_to_trick_card(self.key[sidx + 1]),
            Self::action_to_trick_card(self.key[sidx + 2]),
            final_entry,
        ])
    }

    fn legal_actions_dealing(&self, actions: &mut Vec<Action>) {
        for c in self.deck.get_all(CardLocation::None) {
            let ea: EAction = c.into();
            actions.push(ea.into());
        }
    }

    fn legal_actions_deal_face_up(&self, actions: &mut Vec<Action>) {
        for c in self.deck.get_all(CardLocation::None) {
            let ea: EAction = c.into();
            actions.push(ea.into());
        }
    }

    /// Can choose any trump except for the one from the faceup card
    /// For the dealer they aren't able to pass.
    fn legal_actions_choose_trump(&self, actions: &mut Vec<Action>) {
        let face_up = self
            .face_up()
            .expect("can't call faceup before deal finished")
            .suit();
        if face_up != Suit::Spades {
            actions.push(EAction::Spades.into());
        }
        if face_up != Suit::Clubs {
            actions.push(EAction::Clubs.into());
        }
        if face_up != Suit::Hearts {
            actions.push(EAction::Hearts.into());
        }
        if face_up != Suit::Diamonds {
            actions.push(EAction::Diamonds.into());
        }

        // Dealer can't pass
        if self.cur_player != 3 {
            actions.push(EAction::Pass.into())
        }
    }

    /// Needs to consider following suit if possible
    /// Can only play cards from hand
    fn legal_actions_play(&self, actions: &mut Vec<Action>) {
        // Sitting-out partner plays a Pass sentinel on their turn.
        if Some(self.cur_player) == self.sitting_out_player() {
            actions.push(EAction::Pass.into());
            return;
        }

        let player_loc = self.cur_player.into();
        let hand = self.deck.get_all(player_loc);
        // If they are the first to act on a trick then can play any card in hand
        if self.is_start_of_trick() {
            push_hand_as_actions(hand, actions);
            return;
        }

        // The leading card is the first real card in this trick (skipping any
        // sit-out Pass sentinels that may have preceded it).
        let Some(leading_card) = self.leading_card_of_current_trick() else {
            // No real cards played yet this trick (only sentinels). Lead is open.
            push_hand_as_actions(hand, actions);
            return;
        };
        let leading_suit = self.get_suit(leading_card);
        let suit_mask = suit_mask(leading_suit, self.trump);

        let suited_hand = suit_mask & hand;
        if suited_hand.is_empty() {
            // no suit, can play any card
            push_hand_as_actions(hand, actions);
        } else {
            push_hand_as_actions(suited_hand, actions);
        }
    }

    /// First real card of the current trick, if any. Skips Pass sentinels.
    fn leading_card_of_current_trick(&self) -> Option<Card> {
        if self.phase() != EPhase::Play {
            return None;
        }
        let played_in_trick = self.cards_played % 4;
        if played_in_trick == 0 {
            return None;
        }
        let start = self.key.len() - played_in_trick;
        for i in 0..played_in_trick {
            if let Some(c) = Self::action_to_trick_card(self.key[start + i]) {
                return Some(c);
            }
        }
        None
    }

    /// Maps a trick position index to the actual player. With the Pass-sentinel
    /// rotation the mapping is a straight modular add.
    fn trick_position_to_player(&self, trick_starter: Player, position: usize) -> Player {
        (trick_starter + position) % self.num_players
    }

    /// Returns the player who won the trick. Pass-sentinel entries are ignored.
    fn evaluate_trick(&self, cards: &[Option<Card>], trick_starter: Player) -> Player {
        use Card::*;
        let (left, right) = match self.trump.unwrap() {
            Suit::Clubs => (JS, JC),
            Suit::Spades => (JC, JS),
            Suit::Hearts => (JD, JH),
            Suit::Diamonds => (JH, JD),
        };

        // right always wins
        if let Some(winner) = cards.iter().position(|c| *c == Some(right)) {
            return self.trick_position_to_player(trick_starter, winner);
        }

        // if no right, left always wins
        if let Some(winner) = cards.iter().position(|c| *c == Some(left)) {
            return self.trick_position_to_player(trick_starter, winner);
        }

        // otherwise we can just evaluate by rank. Build the Hand mask directly via OR — no
        // need to collect into an intermediate Vec just to feed Hand::from(&[Card]).
        let mut card_mask = Hand::default();
        for c in cards.iter().filter_map(|c| *c) {
            card_mask.add(c);
        }
        let trump_mask = suit_mask(self.trump.unwrap(), self.trump);

        let trumps = card_mask & trump_mask;
        if !trumps.is_empty() {
            let highest_card = trumps.highest().unwrap();
            let pos = cards
                .iter()
                .position(|c| *c == Some(highest_card))
                .unwrap();
            return self.trick_position_to_player(trick_starter, pos);
        }

        // First non-None card determines the lead suit.
        let leading_card = cards.iter().filter_map(|c| *c).next().unwrap();
        let leading_suit = self.get_suit(leading_card);
        let leading_mask = suit_mask(leading_suit, self.trump);
        let follow_suits = card_mask & leading_mask;
        let highest_card = follow_suits.highest().unwrap();
        let pos = cards
            .iter()
            .position(|c| *c == Some(highest_card))
            .unwrap();
        self.trick_position_to_player(trick_starter, pos)
    }

    /// Gets the suit of a given card. Accounts for the weird scoring of the trump suit
    /// if in the playing phase of the game
    fn get_suit(&self, c: Card) -> Suit {
        let mut suit = c.suit();

        let is_jack = (c == Card::JC) || (c == Card::JS) || (c == Card::JD) || (c == Card::JH);
        if !is_jack {
            return suit;
        }

        // Correct the jack if in play or discard phase
        if self.phase() == EPhase::Play || self.phase() == EPhase::Discard {
            suit = match (c, self.trump.unwrap()) {
                (Card::JC, Suit::Spades) => Suit::Spades,
                (Card::JS, Suit::Clubs) => Suit::Clubs,
                (Card::JH, Suit::Diamonds) => Suit::Diamonds,
                (Card::JD, Suit::Hearts) => Suit::Hearts,
                _ => suit,
            }
        }
        suit
    }

    fn update_keys(&mut self, a: Action) {
        self.key.push(a);
    }

    pub fn phase(&self) -> EPhase {
        self.phase
    }

    /// Return the face up card for the game if it has been dealt yet.
    ///
    /// This returns a card even if the face up card has since been picked up or discarded
    pub fn face_up(&self) -> Option<Card> {
        // read the value from the deck
        // if it's not there, we're probably calling this to rewind, look through the
        // action history to find it
        let displayed_face_up = self.displayed_face_up_card();
        if displayed_face_up.is_some() {
            return displayed_face_up;
        }

        // 21st action will be the face up action
        if self.key.len() >= 21 {
            return Some(EAction::from(self.key[20]).card());
        }

        None
    }

    /// Returns the number of future tricks each team is guaranteed to win
    fn future_tricks(&self) -> (u8, u8) {
        // let mut highest_card_owners = Vec::new();
        // for i in 0..3 {
        //     let owner = self.deck.highest_card(i);
        //     if let Some(o) = owner {
        //         highest_card_owners.push(o);
        //         break;
        //     }
        // }

        // todo!("update to support automatically scoring highest card");
        (self.tricks_won[0], self.tricks_won[1])
    }

    /// Returns the score for team 0 based on tricks won for each team
    fn score(&self, tricks0: u8, tricks1: u8) -> f64 {
        // needs to be a winner
        assert!(tricks0 >= 3 || tricks1 >= 3);
        assert_eq!(self.phase(), EPhase::Play);

        let team_0_call = self.trump_caller.is_multiple_of(2);
        let march_score = if self.going_alone { 4.0 } else { 2.0 };
        match (tricks0, tricks1, team_0_call) {
            (5, 0, _) => march_score,
            (0, 5, _) => -march_score,
            (3 | 4, _, true) => 1.0,
            (3 | 4, _, false) => 2.0,
            (_, 3 | 4, true) => -2.0,
            (_, 3 | 4, false) => -1.0,
            _ => panic!(
                "invalid trick state to call score: {}, {}",
                tricks0, tricks1
            ),
        }
    }
}

impl Display for EuchreGameState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let key = &self.key();
        let mut first_play = None;
        let mut is_last_take = false;
        let mut in_alone_phase = false;

        for i in 0..key.len() {
            let a = EAction::from(key[i]);
            write!(f, "{}", a).unwrap();

            let in_play = first_play.is_some();
            let append_pipe = match a {
                // dealing cards
                _ if i < 20 => (i + 1) % 5 == 0,
                // faceup
                _ if i == 20 => true,
                EAction::Pickup => true,
                EAction::Clubs | EAction::Diamonds | EAction::Hearts | EAction::Spades => {
                    in_alone_phase = true;
                    true
                }
                // discard action
                _ if i > 20 && is_last_take => {
                    in_alone_phase = true;
                    true
                }
                // alone decision
                EAction::Alone => {
                    first_play = Some(i + 1);
                    in_alone_phase = false;
                    true
                }
                // pass in alone phase (declining to go alone)
                EAction::Pass if in_alone_phase => {
                    first_play = Some(i + 1);
                    in_alone_phase = false;
                    true
                }
                // Pass sentinel during Play (sit-out partner). Falls through to
                // the trick-boundary check.
                EAction::Pass if in_play => {
                    let fp = first_play.unwrap();
                    ((i - fp + 1) % 4 == 0) && (i != fp)
                }
                EAction::Pass => false,
                EAction::DiscardMarker => false,
                // everything else is Play
                _ => ((i - first_play.unwrap() + 1) % 4 == 0) && (i != first_play.unwrap()),
            };
            if append_pipe {
                write!(f, "|").unwrap();
            }

            if is_last_take {
                is_last_take = false;
            }

            if a == EAction::Pickup {
                is_last_take = true;
            }
        }

        write!(f, "")
    }
}

impl GameState for EuchreGameState {
    fn apply_action(&mut self, a: Action) {
        self.play_order.push(self.cur_player);
        match self.phase() {
            EPhase::DealHands => self.apply_action_deal_hands(a),
            EPhase::DealFaceUp => self.apply_action_deal_face_up(a),
            EPhase::Pickup => self.apply_action_pickup(a),
            EPhase::ChooseTrump => self.apply_action_choose_trump(a),
            EPhase::Discard => self.apply_action_discard(a),
            EPhase::Alone => self.apply_action_alone(a),
            EPhase::Play => self.apply_action_play(a),
        }
        self.update_keys(a);
    }

    fn legal_actions(&self, actions: &mut Vec<Action>) {
        actions.clear();

        match self.phase() {
            EPhase::DealHands => self.legal_actions_dealing(actions),
            EPhase::DealFaceUp => self.legal_actions_deal_face_up(actions),
            EPhase::Pickup => {
                // Sorted in order of lowest action to hightest
                actions.append(&mut vec![EAction::Pickup.into(), EAction::Pass.into()])
            }
            EPhase::Discard => {
                // Dealer can discard any card
                for c in self.deck.get_all(CardLocation::Player3).cards() {
                    actions.push(EAction::from(c).into());
                }
            }
            EPhase::ChooseTrump => self.legal_actions_choose_trump(actions),
            EPhase::Alone => {
                actions.push(EAction::Alone.into());
                actions.push(EAction::Pass.into());
            }
            EPhase::Play => self.legal_actions_play(actions),
        };
    }

    fn evaluate(&self, p: Player) -> f64 {
        if !self.is_terminal() {
            panic!("evaluate called on non-terminal gamestate");
        }

        let team = p % 2;
        let future_tricks = self.future_tricks();
        if team == 0 {
            self.score(future_tricks.0, future_tricks.1)
        } else {
            -self.score(future_tricks.0, future_tricks.1)
        }
    }

    fn istate_key(&self, player: Player) -> IStateKey {
        let mut istate = IStateKey::default();

        let mut is_last_pickup = false;
        for (i, (p, a)) in self.play_order.iter().zip(self.key.iter()).enumerate() {
            let ea = EAction::from(*a);
            let is_visible = match EAction::from(*a) {
                EAction::DiscardMarker => false,
                _ if i < 20 => player == *p, // dealing hand
                // don't show the discard if we're not player 3
                _ if (is_last_pickup && player != 3) => false,
                _ => true,
            };

            if is_visible {
                istate.push(*a)
            }

            is_last_pickup = false;
            if ea == EAction::Pickup {
                is_last_pickup = true;
            }
        }
        istate.sort_range(0, CARDS_PER_HAND.min(istate.len()));

        // Push a bogus action to the end to show that this is a discard istate rather than player 1 going
        if player == 3 && self.phase == EPhase::Discard {
            istate.push(EAction::DiscardMarker.into())
        }

        istate
    }

    fn istate_string(&self, player: Player) -> String {
        let istate = self.istate_key(player);

        // Full game state:
        // 9CTCJCKCKS|KH|PPPPPPCP|3H|ASTSKSAC|9C9HTDQC|JD9DTCJH|JSKCQHQD|KDADXXXX|
        let mut r = String::new();

        if self.phase() == EPhase::DealHands {
            todo!("don't yet support istates during dealing phase");
        }

        for i in 0..5 {
            let a = istate[i];
            let s = EAction::from(a).to_string();
            r.push_str(&s);
        }

        if self.phase() == EPhase::DealFaceUp {
            return r;
        }

        // Face up card
        let a = istate[5];

        r.push('|');
        let s = EAction::from(a).to_string();
        r.push_str(&s);
        r.push('|');

        // Pickup round and calling round
        let mut pickup_called = false;
        let mut num_pickups = 0;
        for i in 6..(istate.len()).min(6 + 4) {
            let a = istate[i];
            let a = EAction::from(a);
            let s = a.to_string();
            r.push_str(&s);
            num_pickups += 1;

            if a == EAction::Pickup {
                pickup_called = true;
            }

            if a != EAction::Pass {
                break;
            }
        }

        if self.phase() == EPhase::Pickup {
            return r;
        }

        let mut num_calls = 0;
        if !pickup_called {
            for i in 10..(istate.len()).min(6 + 4 + 4) {
                let a = istate[i];
                let a = EAction::from(a);
                let s = a.to_string();
                r.push_str(&s);
                num_calls += 1;

                if a != EAction::Pass {
                    break;
                }
            }

            if self.phase() == EPhase::ChooseTrump {
                return r;
            }
        }

        r.push('|');

        r.push_str(&format!("{}{}", self.trump_caller, self.trump.unwrap()));

        if self.phase() == EPhase::Discard {
            return r;
        }

        // If the dealer, show the discarded card if that happened
        let mut extra_offset = 0;
        if player == 3 && pickup_called {
            r.push('|');
            let a = istate[6 + num_pickups];

            let d = EAction::from(a).to_string();
            r.push_str(&d);
            extra_offset = 1;
        }

        // Alone decision (L or P)
        let alone_offset = 6 + num_pickups + num_calls + extra_offset;
        if alone_offset < istate.len() {
            let alone_action = EAction::from(istate[alone_offset]);
            if alone_action == EAction::Alone || (alone_action == EAction::Pass && self.phase != EPhase::Alone) {
                r.push_str(&alone_action.to_string());
                extra_offset += 1;
            }
        }

        if self.phase() == EPhase::Alone {
            return r;
        }

        // populate play data
        let ppt = self.players_per_trick();
        let mut turn = 0;
        let mut i = 6 + num_pickups + num_calls + extra_offset;
        while i < istate.len() {
            if turn % ppt == 0 {
                r.push('|');
            }

            let a = istate[i];
            let c = EAction::from(a).to_string();

            r.push_str(&c);
            turn += 1;
            i += 1;
        }

        if turn % ppt == 0 {
            r.push('|');
        }

        r
    }

    fn is_terminal(&self) -> bool {
        let future_tricks = self.future_tricks();

        self.cards_played == 20
        // Check if the scores are already decided: see if have taken a trick in defence
        || (future_tricks.0 > 0 && future_tricks.1 >= 3)
        || (future_tricks.0 >= 3 && future_tricks.1 > 0)
    }

    fn is_chance_node(&self) -> bool {
        self.phase == EPhase::DealHands || self.phase == EPhase::DealFaceUp
    }

    fn num_players(&self) -> usize {
        self.num_players
    }

    fn cur_player(&self) -> Player {
        self.cur_player
    }

    fn key(&self) -> IStateKey {
        let mut sorted_key = self.key;
        for p in 0..self.num_players {
            let start_sort = p * CARDS_PER_HAND;
            let end_sort = sorted_key.len().min((p + 1) * CARDS_PER_HAND);
            sorted_key.sort_range(start_sort, end_sort - start_sort);
            if (p + 1) * CARDS_PER_HAND + 1 > sorted_key.len() {
                break;
            }
        }

        sorted_key
    }

    fn undo(&mut self) {
        self.cur_player = self.play_order.pop().unwrap();
        let action_number = self.key.len();
        let applied_action = EAction::from(self.key.pop());

        // fix the trick winner counts
        if self.cards_played > 0 && self.cards_played.is_multiple_of(4) {
            let trick = self.cards_played / 4 - 1;
            let last_winner = self.trick_winners[trick];
            self.trick_winners[trick] = 0; // reset it
            self.tricks_won[last_winner % 2] -= 1;
        }

        match applied_action {
            // dealing player cards
            _ if action_number <= 20 => {
                let c = applied_action.card();
                self.deck.set(c, CardLocation::None);
                self.phase = EPhase::DealHands
            }
            // dealing face up card
            _ if action_number == 21 => {
                let c = applied_action.card();
                self.deck.set(c, CardLocation::None);
                self.phase = EPhase::DealFaceUp;
            }

            EAction::Alone => {
                self.going_alone = false;
                self.phase = EPhase::Alone;
            }

            EAction::Pass => {
                // Play-phase Pass (sit-out sentinel): undo the play step.
                if self.phase == EPhase::Play && self.cards_played > 0 {
                    self.cards_played -= 1;
                    if self.cards_played % 4 == 3 {
                        // Just undid the trick-completing play. Restore the 3 preceding
                        // plays (skipping any sentinels) to the table.
                        for (a, p) in self
                            .key
                            .iter()
                            .rev()
                            .take(3)
                            .zip(self.play_order.iter().rev().take(3))
                        {
                            let ea = EAction::from(*a);
                            if ea != EAction::Pass {
                                self.deck.set(ea.card(), CardLocation::Played(*p));
                            }
                        }
                    }
                }
                // Alone-phase Pass (decline going alone).
                else if self.phase == EPhase::Play && self.cards_played == 0 {
                    self.phase = EPhase::Alone;
                }
                // Pickup 4th-pass → ChooseTrump transition.
                else if self.key.len() == 20 + 1 + 3 {
                    self.phase = EPhase::Pickup;
                    let face_up = self
                        .face_up()
                        .expect("can't call faceup before deal finished");
                    self.deck.set(face_up, CardLocation::FaceUp);
                }
                // Otherwise: plain bidding/choose-trump pass, no phase change.
            }
            EAction::Clubs | EAction::Spades | EAction::Hearts | EAction::Diamonds => {
                self.phase = EPhase::ChooseTrump;
                // return to defaults
                self.trump_caller = 0;
                self.trump = None;
            }
            EAction::Pickup => {
                self.phase = EPhase::Pickup;
                // return to defaults
                self.trump_caller = 0;
                self.trump = None;
                let face_up = self
                    .face_up()
                    .expect("can't call faceup before deal finished");
                self.deck.set(face_up, CardLocation::FaceUp);
            }

            // If last action was pickup, we're discarding
            _ if self.key.last() == Some(&Action::from(EAction::Pickup)) => {
                let c = applied_action.card();
                self.deck.set(c, CardLocation::Player3);
                self.phase = EPhase::Discard;
            }
            EAction::DiscardMarker => {
                panic!("discard marker should never be in interanl game istate")
            }
            // Play-phase card action.
            _ => {
                let c = applied_action.card();

                self.cards_played -= 1;
                // put the old cards back on the table if trick just ended
                if self.cards_played % 4 == 3 {
                    self.deck.set(c, self.cur_player.into());

                    for (a, p) in self
                        .key
                        .iter()
                        .rev()
                        .take(3)
                        .zip(self.play_order.iter().rev().take(3))
                    {
                        let ea = EAction::from(*a);
                        if ea != EAction::Pass {
                            self.deck.set(ea.card(), CardLocation::Played(*p));
                        }
                    }
                } else {
                    self.deck
                        .unplay(c, self.cur_player)
                        .unwrap_or_else(|_| panic!("failed to unplay card: {}, gs: {}", c, self));
                }
            }
        }
    }

    fn transposition_table_hash(&self) -> Option<crate::istate::IsomorphicHash> {
        if self.phase != EPhase::Play {
            return None;
        }

        if !self.is_start_of_trick() && self.cards_played > 0 {
            // only cache values at the start of the trick
            return None;
        }

        // Inlined FxHash-style mix. DefaultHasher (SipHasher) was showing up at ~4% of total
        // CPU time; this call is on the alpha_beta hot path and all inputs together are well
        // under 64 bytes, so a minimal multiply-xor mix gives the same collision resistance
        // we need (just a cache key, not a security hash) for a fraction of the cost.
        const K: u64 = 0x517cc1b727220a95;
        #[inline(always)]
        fn mix(h: u64, x: u64) -> u64 {
            (h.rotate_left(5) ^ x).wrapping_mul(K)
        }

        let iso_deck = iso_deck(self.deck, self.trump);
        let d0 = ((iso_deck[0] as u64) << 32) | (iso_deck[1] as u64);
        let d1 = ((iso_deck[2] as u64) << 32) | (iso_deck[3] as u64);
        // Pack tricks_won (2 × u8), calling_team (1 bit), going_alone (1 bit), cur_player
        // (2 bits) into a single u64 word.
        let calling_team = (self.trump_caller & 1) as u64;
        let tail: u64 = (self.tricks_won[0] as u64)
            | ((self.tricks_won[1] as u64) << 8)
            | (calling_team << 16)
            | ((self.going_alone as u64) << 17)
            | ((self.cur_player as u64 & 0b11) << 18);

        let mut h: u64 = 0;
        h = mix(h, d0);
        h = mix(h, d1);
        h = mix(h, tail);
        Some(h)
    }
}

/// Push every card in a hand as an `Action` directly via bit iteration. Each `Action`
/// stores the bit-index of the corresponding `EAction` discriminant, so we can produce
/// it from the hand mask without going through `Card → EAction → Action`, both of which
/// pay an extra `trailing_zeros` round-trip.
#[inline]
fn push_hand_as_actions(hand: Hand, actions: &mut Vec<Action>) {
    let mut mask = hand.raw_mask();
    while mask != 0 {
        let bit = mask.trailing_zeros() as u8;
        mask &= mask - 1; // clear lowest set bit
        actions.push(Action(bit));
    }
}

/// Returns a mask for filtering hands for all cards of a given suit
pub(super) fn suit_mask(suit: Suit, trump: Option<Suit>) -> Hand {
    let mut mask = match suit {
        Suit::Clubs => CLUBS_MASK,
        Suit::Spades => SPADES_MASK,
        Suit::Hearts => HEART_MASK,
        Suit::Diamonds => DIAMONDS_MASK,
    };

    if let Some(t) = trump {
        use Card::*;
        use Suit::*;
        match (suit, t) {
            (Clubs, Clubs) => mask |= JS.mask(),
            (Clubs, Spades) => mask &= !JC.mask(),
            (Clubs, _) => {}
            (Spades, Clubs) => mask &= !JS.mask(),
            (Spades, Spades) => mask |= JC.mask(),
            (Spades, _) => {}
            (Hearts, Hearts) => mask |= JD.mask(),
            (Hearts, Diamonds) => mask &= !JH.mask(),
            (Hearts, _) => {}
            (Diamonds, Hearts) => mask &= !JD.mask(),
            (Diamonds, Diamonds) => mask |= JH.mask(),
            (Diamonds, _) => {}
        }
    }

    Hand::from_mask(mask)
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, vec};

    use itertools::Itertools;
    use rand::{rng, rngs::StdRng, seq::IndexedRandom, RngExt, SeedableRng};

    use crate::{
        actions,
        gamestates::euchre::{actions::Card, deck::CARDS, EAction, EPhase, Euchre, Suit},
        resample::ResampleFromInfoState,
    };

    use super::{EuchreGameState, GameState};

    #[test]
    fn euchre_test_phases_choose_trump() {
        let mut s = Euchre::new_state();

        assert_eq!(s.phase(), EPhase::DealHands);
        use Card::*;
        let cards_to_deal = [
            NC, TC, JC, QC, KC, AC, NS, TS, JS, QS, KS, AS, NH, TH, JH, QH, KH, AH, ND, TD,
        ];

        for c in cards_to_deal {
            s.apply_action(EAction::from(c).into());
        }

        assert_eq!(s.phase(), EPhase::DealFaceUp);
        s.apply_action(EAction::JD.into());

        assert_eq!(s.phase(), EPhase::Pickup);
        assert!(!s.is_chance_node());
        for i in 0..4 {
            assert_eq!(s.cur_player, i);
            s.apply_action(EAction::Pass.into());
        }

        assert_eq!(s.phase(), EPhase::ChooseTrump);
        assert_eq!(s.cur_player, 0);
        s.apply_action(EAction::Pass.into());
        s.apply_action(EAction::Clubs.into());
        assert_eq!(s.cur_player, 1); // player 1 called clubs, now decides alone

        assert_eq!(s.phase(), EPhase::Alone);
        s.apply_action(EAction::Pass.into()); // decline to go alone

        assert_eq!(s.phase(), EPhase::Play);
        assert_eq!(s.cur_player, 0);
    }

    #[test]
    fn euchre_test_phases_pickup() {
        let mut s = Euchre::new_state();

        assert_eq!(s.phase(), EPhase::DealHands);

        use Card::*;
        let cards_to_deal = [
            NC, TC, JC, QC, KC, AC, NS, TS, JS, QS, KS, AS, NH, TH, JH, QH, KH, AH, ND, TD,
        ];

        for c in cards_to_deal {
            s.apply_action(EAction::from(c).into());
        }

        assert_eq!(s.phase(), EPhase::DealFaceUp);
        s.apply_action(EAction::from(Card::JD).into());

        assert_eq!(s.phase(), EPhase::Pickup);
        assert!(!s.is_chance_node());
        for _ in 0..3 {
            s.apply_action(EAction::Pass.into());
        }
        s.apply_action(EAction::Pickup.into());

        assert_eq!(s.phase(), EPhase::Discard);
        s.apply_action(EAction::QH.into());

        assert_eq!(s.phase(), EPhase::Alone);
        assert_eq!(s.cur_player, 3); // dealer (player 3) called pickup, decides alone
        s.apply_action(EAction::Pass.into()); // decline to go alone

        assert_eq!(s.phase(), EPhase::Play);
        assert_eq!(s.cur_player, 0);
    }

    #[test]
    fn euchre_test_legal_actions() {
        let mut gs = Euchre::new_state();

        for (i, c) in CARDS.iter().enumerate().take(20) {
            gs.apply_action(EAction::from(*c).into());
            let legal = actions!(gs);
            for j in CARDS.iter().take(i) {
                assert!(!legal.contains(&EAction::from(*j).into()));
            }
        }

        // Deal the face up card
        gs.apply_action(EAction::from(Card::QD).into());
        assert_eq!(gs.face_up().unwrap(), Card::QD);

        assert_eq!(
            actions!(gs),
            vec![EAction::Pickup.into(), EAction::Pass.into()]
        );

        gs.apply_action(EAction::Pickup.into());
        // Cards in dealers hand, including face up card
        assert_eq!(
            actions!(gs)
                .iter()
                .map(|x| EAction::from(*x).to_string())
                .collect_vec(),
            vec!["Qh", "Kh", "Ah", "9d", "Td", "Qd"]
        );
        assert_eq!(gs.phase(), EPhase::Discard);
        gs.apply_action(EAction::QH.into());

        // Alone decision
        assert_eq!(gs.phase(), EPhase::Alone);
        assert_eq!(
            actions!(gs),
            vec![EAction::Alone.into(), EAction::Pass.into()]
        );
        gs.apply_action(EAction::Pass.into());

        // Cards player 0s hand
        assert_eq!(gs.phase(), EPhase::Play);
        assert_eq!(
            actions!(gs)
                .iter()
                .map(|x| EAction::from(*x).to_string())
                .collect_vec(),
            vec!["9s", "Ts", "Js", "Qs", "Ks"],
            "gs: {}",
            gs
        );

        gs.apply_action(EAction::NS.into());
        // Player 1 must follow suit
        assert_eq!(
            actions!(gs)
                .iter()
                .map(|x| EAction::from(*x).to_string())
                .collect_vec(),
            vec!["As"],
            "gs: {}",
            gs
        );

        let gs = EuchreGameState::from("TcQs9hJdQd|QcThJhKhKd|AcTsAhTdAd|9cKc9sKsQh|Jc|T|Kc|P|QdKd");
        let actions = actions!(gs);
        assert_eq!(gs.cur_player(), 2);
        use EAction::*;
        assert_eq!(
            actions.into_iter().map(EAction::from).collect_vec(),
            vec![TD, AD]
        );
    }

    #[test]
    fn euchre_test_suit() {
        let mut s = Euchre::new_state();

        assert_eq!(s.get_suit(Card::NC), Suit::Clubs);
        // Jack of spades is still a spade
        assert_eq!(s.get_suit(Card::JS), Suit::Spades);
        assert_eq!(s.get_suit(Card::TS), Suit::Spades);

        // Deal the cards
        use Card::*;
        let cards_to_deal = [
            TC, JC, QC, KC, AC, NS, TS, JS, QS, KS, AS, NH, TH, JH, QH, KH, AH, ND, TD, JD,
        ];
        for c in cards_to_deal {
            s.apply_action(EAction::from(c).into());
        }

        s.apply_action(EAction::NC.into()); // Deal the 9 face up
        s.apply_action(EAction::Pickup.into());
        s.apply_action(EAction::from(Card::TD).into());
        assert_eq!(s.trump, Some(Suit::Clubs));
        assert_eq!(s.phase(), EPhase::Alone);
        s.apply_action(EAction::Pass.into()); // decline alone
        assert_eq!(s.phase(), EPhase::Play);
        // Jack of spades is now a club since it's trump
        assert_eq!(s.get_suit(Card::JS), Suit::Clubs);
        assert_eq!(s.get_suit(Card::TS), Suit::Spades);
    }

    #[test]
    fn euchre_test_istate() {
        let mut gs = EuchreGameState::from("9cTcJcQcKc|Ac9sTsJdQs|KsAs9hThJh|QhKhAh9dTd");

        assert_eq!(gs.istate_string(0), "9cTcJcQcKc");
        assert_eq!(gs.istate_string(1), "9sTsQsAcJd");
        assert_eq!(gs.istate_string(2), "KsAs9hThJh");
        assert_eq!(gs.istate_string(3), "QhKhAh9dTd");

        gs.apply_action(EAction::from(Card::JS).into());
        assert_eq!(gs.istate_string(0), "9cTcJcQcKc|Js|");
        assert_eq!(gs.istate_string(1), "9sTsQsAcJd|Js|");
        assert_eq!(gs.istate_string(2), "KsAs9hThJh|Js|");
        assert_eq!(gs.istate_string(3), "QhKhAh9dTd|Js|");

        let mut new_s = gs.clone(); // for alternative pickup parsing

        gs.apply_action(EAction::Pickup.into());
        assert_eq!(gs.istate_string(0), "9cTcJcQcKc|Js|T|0S");

        // Dealer discards the QH
        assert_eq!(gs.istate_string(3), "QhKhAh9dTd|Js|T|0S");
        gs.apply_action(EAction::QH.into());
        // After discard, now in Alone phase
        assert_eq!(gs.phase(), EPhase::Alone);

        // Alone decision
        assert_eq!(gs.phase(), EPhase::Alone);
        gs.apply_action(EAction::Pass.into()); // decline to go alone

        for _ in 0..4 {
            let a = actions!(gs)[0];
            gs.apply_action(a);
        }
        assert_eq!(gs.istate_string(0), "9cTcJcQcKc|Js|T|0SP|9cAcKsJs|");
        assert_eq!(gs.istate_string(1), "9sTsQsAcJd|Js|T|0SP|9cAcKsJs|");
        assert_eq!(gs.istate_string(2), "KsAs9hThJh|Js|T|0SP|9cAcKsJs|");
        assert_eq!(gs.istate_string(3), "QhKhAh9dTd|Js|T|0S|QhP|9cAcKsJs|");
        assert_eq!(gs.cur_player(), 3);

        while !gs.is_terminal() {
            let a = actions!(gs)[0];
            gs.apply_action(a);
            gs.istate_string(0);
        }
        assert_eq!(gs.evaluate(0), -2.0);
        assert_eq!(gs.evaluate(1), 2.0);
        assert_eq!(gs.evaluate(2), -2.0);
        assert_eq!(gs.evaluate(3), 2.0);

        // Different calling path
        for _ in 0..5 {
            new_s.apply_action(EAction::Pass.into());
        }
        new_s.apply_action(EAction::Hearts.into());
        assert_eq!(new_s.phase(), EPhase::Alone);
        assert_eq!(new_s.istate_string(0), "9cTcJcQcKc|Js|PPPPPH|1H");
        new_s.apply_action(EAction::Pass.into()); // decline alone
        assert_eq!(new_s.istate_string(0), "9cTcJcQcKc|Js|PPPPPH|1HP|");
    }

    #[test]
    fn euchre_test_unique_istate() {
        let mut actions = Vec::new();
        for _ in 0..1000 {
            let mut s = Euchre::new_state();
            let mut istates = HashSet::new();
            while s.is_chance_node() {
                actions.clear();
                s.legal_actions(&mut actions);
                let a = actions.choose(&mut rng()).unwrap();
                s.apply_action(*a);
            }

            istates.insert(s.istate_string(s.cur_player));
            while !s.is_terminal() {
                s.legal_actions(&mut actions);
                let a = actions.choose(&mut rng()).unwrap();
                s.apply_action(*a);
                let istate = s.istate_string(s.cur_player);
                assert!(!istates.contains(&istate));
                istates.insert(istate);
            }
        }
    }

    #[test]
    fn euchre_test_resample_from_istate() {
        let mut rng = rng();
        let mut actions = Vec::new();

        for _ in 0..100 {
            let mut s = Euchre::new_state();

            while s.is_chance_node() {
                s.legal_actions(&mut actions);
                let a = actions.choose(&mut rng).unwrap();
                s.apply_action(*a);
            }

            while !s.is_terminal() {
                for p in 0..s.num_players() {
                    let original_istate = s.istate_key(p);
                    for _ in 0..10 {
                        let sampled_state = s.resample_from_istate(p, &mut rng);
                        let sampled_key = sampled_state.istate_key(p);
                        assert_eq!(sampled_key, original_istate)
                    }
                }

                s.legal_actions(&mut actions);
                let a = actions.choose(&mut rng).unwrap();
                s.apply_action(*a);
            }
        }

        for _ in 0..100 {
            // this is a hard case where the dealer discards a card and doesn't follow suit because of it
            let gs =
        EuchreGameState::from("AcTsThTdJd|QcJs9hKh9d|Kc9sAsQdAd|9cTcJcQsJh|Ks|PPPT|Tc|P|Td9dAdJh|QdJcJdKh|QsTsJs9s|9hAs9cTh|KcKs");
            gs.resample_from_istate(2, &mut rng);

            let gs =
        EuchreGameState::from("9cTcAc9s9d|Jc9hJhTdKd|TsQsKsJdQd|QcKcAsQhAd|Js|PPPT|Qh|P|9dKdJdAd|QcAcJcQd|9hTsJs9c|As9sJhQs|KcTc");
            gs.resample_from_istate(2, &mut rng);
        }
    }

    #[test]
    fn test_undo_euchre() {
        let mut rng: StdRng = SeedableRng::seed_from_u64(0);
        for _ in 0..1000 {
            let mut gs = Euchre::new_state();

            while !gs.is_terminal() {
                let actions = actions!(gs);
                assert!(!actions.is_empty());
                let a = actions.choose(&mut rng).unwrap();
                let mut ngs = gs.clone();
                ngs.apply_action(*a);
                ngs.undo();
                assert_eq!(ngs, gs);
                gs.apply_action(*a);
            }
        }
    }

    #[test]
    fn test_euchre_resample_from_istate_deterministic() {
        let gs = EuchreGameState::from("9cJcQcTsTd|KcKsQhKh9d|TcAcQsAsTh|Js9hAhQdAd|Kd|PT|Js|P|");
        let sampled = gs.resample_from_istate(
            gs.cur_player(),
            &mut StdRng::seed_from_u64(42),
        );

        for _ in 0..100 {
            assert_eq!(
                gs.resample_from_istate(
                    gs.cur_player(),
                    &mut StdRng::seed_from_u64(42),
                ),
                sampled
            )
        }
    }

    // ==================== Going Alone Tests ====================

    #[test]
    fn test_going_alone_pickup_path() {
        use Card::*;
        let cards_to_deal = [
            NC, TC, JC, QC, KC, AC, NS, TS, JS, QS, KS, AS, NH, TH, JH, QH, KH, AH, ND, TD,
        ];
        let mut s = Euchre::new_state();
        for c in cards_to_deal {
            s.apply_action(EAction::from(c).into());
        }
        s.apply_action(EAction::JD.into()); // face up

        // Player 0 picks up
        s.apply_action(EAction::Pickup.into());
        assert_eq!(s.phase(), EPhase::Discard);

        // Dealer discards
        s.apply_action(EAction::QH.into());
        assert_eq!(s.phase(), EPhase::Alone);
        assert_eq!(s.cur_player, 0); // trump caller decides

        // Go alone
        s.apply_action(EAction::Alone.into());
        assert_eq!(s.phase(), EPhase::Play);
        assert!(s.going_alone());
        // Sentinel rotation keeps 4 players per trick; sit-out plays Pass.
        assert_eq!(s.players_per_trick(), 4);
        assert_eq!(s.sitting_out_player(), Some(2)); // partner of player 0

        // Player 0 leads the first trick.
        assert_eq!(s.cur_player, 0);
    }

    #[test]
    fn test_going_alone_choose_trump_path() {
        use Card::*;
        let cards_to_deal = [
            NC, TC, JC, QC, KC, AC, NS, TS, JS, QS, KS, AS, NH, TH, JH, QH, KH, AH, ND, TD,
        ];
        let mut s = Euchre::new_state();
        for c in cards_to_deal {
            s.apply_action(EAction::from(c).into());
        }
        s.apply_action(EAction::JD.into()); // face up

        // All pass in pickup
        for _ in 0..4 {
            s.apply_action(EAction::Pass.into());
        }
        assert_eq!(s.phase(), EPhase::ChooseTrump);

        // Player 1 calls clubs
        s.apply_action(EAction::Pass.into()); // player 0 passes
        s.apply_action(EAction::Clubs.into()); // player 1 calls
        assert_eq!(s.phase(), EPhase::Alone);
        assert_eq!(s.cur_player, 1); // player 1 decides alone

        s.apply_action(EAction::Alone.into());
        assert!(s.going_alone());
        assert_eq!(s.sitting_out_player(), Some(3)); // partner of player 1
        assert_eq!(s.players_per_trick(), 4);

        // Player 0 starts play.
        assert_eq!(s.cur_player, 0);
    }

    #[test]
    fn test_decline_alone() {
        use Card::*;
        let cards_to_deal = [
            NC, TC, JC, QC, KC, AC, NS, TS, JS, QS, KS, AS, NH, TH, JH, QH, KH, AH, ND, TD,
        ];
        let mut s = Euchre::new_state();
        for c in cards_to_deal {
            s.apply_action(EAction::from(c).into());
        }
        s.apply_action(EAction::JD.into());

        // Player 0 picks up, dealer discards, declines alone
        s.apply_action(EAction::Pickup.into());
        s.apply_action(EAction::QH.into());
        s.apply_action(EAction::Pass.into()); // decline alone

        assert!(!s.going_alone());
        assert_eq!(s.players_per_trick(), 4);
        assert_eq!(s.sitting_out_player(), None);
        assert_eq!(s.phase(), EPhase::Play);
    }

    #[test]
    fn test_alone_player_rotation_sits_out_partner() {
        // Build a going-alone game via parser
        // Player 3 picks up, discards Ah (the face-up), goes alone. Partner is player 1.
        let gs = EuchreGameState::from(
            "KcTsJsQsAd|9cTcAcKsAs|ThKh9dJdKd|JcJhQhAhQd|Qc|PPPT|Ah|L|",
        );
        assert!(gs.going_alone());
        assert_eq!(gs.sitting_out_player(), Some(1));
        // Sentinel rotation: always 4 players per trick.
        assert_eq!(gs.players_per_trick(), 4);

        // Player 0 leads.
        assert_eq!(gs.cur_player(), 0);

        // Player 0 plays Kc; next player is 1 (the sit-out partner).
        let mut gs = gs;
        gs.apply_action(EAction::KC.into());
        assert_eq!(gs.cur_player(), 1);
        // The only legal action for the sit-out player is a Pass sentinel.
        let mut actions = Vec::new();
        gs.legal_actions(&mut actions);
        assert_eq!(actions, vec![EAction::Pass.into()]);
        gs.apply_action(EAction::Pass.into());
        assert_eq!(gs.cur_player(), 2);
    }

    #[test]
    fn test_alone_trick_detection_3_real_cards_plus_sentinel() {
        // Going-alone trick contains 3 real plays + 1 Pass sentinel for the
        // sitting-out partner.
        let gs = EuchreGameState::from(
            "KcTsJsQsAd|9cTcAcKsAs|ThKh9dJdKd|JcJhQhAhQd|Qc|PPPT|Ah|L|AdPKdQd",
        );
        assert!(gs.going_alone());
        // 4 plays (including one sentinel) = 1 trick complete.
        assert!(gs.is_trick_over());
        assert_eq!(gs.tricks_won[0] + gs.tricks_won[1], 1);
    }

    #[test]
    fn test_alone_terminal_at_20_plays() {
        // Full going-alone game: 5 tricks × 4 plays = 20 play actions (with
        // 5 sit-out Pass sentinels mixed in).
        let mut rng: StdRng = SeedableRng::seed_from_u64(42);
        for _ in 0..100 {
            let mut gs = Euchre::new_state();
            while gs.is_chance_node() {
                let mut actions = Vec::new();
                gs.legal_actions(&mut actions);
                let a = actions.choose(&mut rng).unwrap();
                gs.apply_action(*a);
            }

            let mut went_alone = false;
            while !gs.is_terminal() {
                let mut actions = Vec::new();
                gs.legal_actions(&mut actions);
                let a = if gs.phase() == EPhase::Alone {
                    went_alone = true;
                    EAction::Alone.into()
                } else {
                    *actions.choose(&mut rng).unwrap()
                };
                gs.apply_action(a);
            }

            if went_alone {
                // 5 tricks × 4 plays = 20 max; early termination can end sooner
                // once the outcome is decided.
                assert!(
                    gs.cards_played <= 20,
                    "alone game plays: {}",
                    gs.cards_played
                );
                // Any completed trick has exactly 4 play entries (3 real + 1 sentinel).
                assert!(gs.cards_played.is_multiple_of(4) || gs.cards_played == 20);
            }
        }
    }

    #[test]
    fn test_alone_scoring_loner_march() {
        // Team 0 goes alone and wins all 5 tricks = 4 points
        let mut rng: StdRng = SeedableRng::seed_from_u64(100);
        let mut found_loner_march = false;

        for _ in 0..10000 {
            let mut gs = Euchre::new_state();
            while gs.is_chance_node() {
                let mut actions = Vec::new();
                gs.legal_actions(&mut actions);
                gs.apply_action(*actions.choose(&mut rng).unwrap());
            }

            // Play randomly but force alone
            while !gs.is_terminal() {
                let mut actions = Vec::new();
                gs.legal_actions(&mut actions);
                let a = if gs.phase() == EPhase::Alone {
                    EAction::Alone.into()
                } else {
                    *actions.choose(&mut rng).unwrap()
                };
                gs.apply_action(a);
            }

            if gs.going_alone() && gs.tricks_won[gs.trump_caller % 2] == 5 {
                let caller_team = gs.trump_caller % 2;
                let score = gs.evaluate(caller_team);
                assert_eq!(score, 4.0, "loner march should be 4 points, got {}", score);
                found_loner_march = true;
                break;
            }
        }
        assert!(found_loner_march, "should find at least one loner march in 10000 games");
    }

    #[test]
    fn test_alone_scoring_euchred() {
        // When going alone and losing, defense gets 2 points
        let mut rng: StdRng = SeedableRng::seed_from_u64(200);
        let mut found_euchre = false;

        for _ in 0..10000 {
            let mut gs = Euchre::new_state();
            while gs.is_chance_node() {
                let mut actions = Vec::new();
                gs.legal_actions(&mut actions);
                gs.apply_action(*actions.choose(&mut rng).unwrap());
            }

            while !gs.is_terminal() {
                let mut actions = Vec::new();
                gs.legal_actions(&mut actions);
                let a = if gs.phase() == EPhase::Alone {
                    EAction::Alone.into()
                } else {
                    *actions.choose(&mut rng).unwrap()
                };
                gs.apply_action(a);
            }

            if gs.going_alone() {
                let caller_team = gs.trump_caller % 2;
                let defense_team = (caller_team + 1) % 2;
                if gs.tricks_won[defense_team] >= 3 && gs.tricks_won[caller_team] > 0 {
                    // Defense wins = euchre. Defense gets 2 points.
                    let defense_score = gs.evaluate(defense_team);
                    assert_eq!(defense_score, 2.0, "euchre should give defense 2 points");
                    found_euchre = true;
                    break;
                }
            }
        }
        assert!(found_euchre, "should find at least one euchre in 10000 games");
    }

    #[test]
    fn test_alone_legal_actions() {
        use Card::*;
        let cards_to_deal = [
            NC, TC, JC, QC, KC, AC, NS, TS, JS, QS, KS, AS, NH, TH, JH, QH, KH, AH, ND, TD,
        ];
        let mut s = Euchre::new_state();
        for c in cards_to_deal {
            s.apply_action(EAction::from(c).into());
        }
        s.apply_action(EAction::JD.into());
        s.apply_action(EAction::Pickup.into());
        s.apply_action(EAction::QH.into());

        assert_eq!(s.phase(), EPhase::Alone);
        let legal = actions!(s);
        assert_eq!(legal, vec![EAction::Alone.into(), EAction::Pass.into()]);
    }

    #[test]
    fn test_alone_undo() {
        use Card::*;
        let cards_to_deal = [
            NC, TC, JC, QC, KC, AC, NS, TS, JS, QS, KS, AS, NH, TH, JH, QH, KH, AH, ND, TD,
        ];
        let mut s = Euchre::new_state();
        for c in cards_to_deal {
            s.apply_action(EAction::from(c).into());
        }
        s.apply_action(EAction::JD.into());
        s.apply_action(EAction::Pickup.into());
        s.apply_action(EAction::QH.into());

        let before_alone = s.clone();
        s.apply_action(EAction::Alone.into());
        assert!(s.going_alone());
        s.undo();
        assert_eq!(s, before_alone);

        let before_pass = s.clone();
        s.apply_action(EAction::Pass.into());
        assert!(!s.going_alone());
        s.undo();
        assert_eq!(s, before_pass);
    }

    #[test]
    fn test_alone_parser_roundtrip() {
        // Pickup path going alone
        let gs1 = EuchreGameState::from(
            "KcTsJsQsAd|9cTcAcKsAs|ThKh9dJdKd|JcJhQhAhQd|Qc|PPPT|Ah|L|"
        );
        assert!(gs1.going_alone());
        assert_eq!(gs1.phase(), EPhase::Play);

        // Pickup path declining alone
        let gs2 = EuchreGameState::from(
            "KcTsJsQsAd|9cTcAcKsAs|ThKh9dJdKd|JcJhQhAhQd|Qc|PPPT|Ah|P|"
        );
        assert!(!gs2.going_alone());
        assert_eq!(gs2.phase(), EPhase::Play);
    }

    #[test]
    fn test_alone_full_game_playthrough() {
        // Play many full games with going alone to ensure no panics
        let mut rng: StdRng = SeedableRng::seed_from_u64(42);
        let mut alone_games = 0;

        for _ in 0..1000 {
            let mut gs = Euchre::new_state();
            while !gs.is_terminal() {
                let mut actions = Vec::new();
                gs.legal_actions(&mut actions);
                assert!(!actions.is_empty(), "no legal actions at: {}", gs);

                let a = if gs.phase() == EPhase::Alone {
                    // 50% chance of going alone
                    if rng.random_range(0..2) == 0 {
                        alone_games += 1;
                        EAction::Alone.into()
                    } else {
                        EAction::Pass.into()
                    }
                } else {
                    *actions.choose(&mut rng).unwrap()
                };
                gs.apply_action(a);
            }

            // Verify terminal state is valid
            assert!(gs.is_terminal());
            let score = gs.evaluate(0);
            if gs.going_alone() {
                assert!(
                    score == 4.0 || score == 1.0 || score == -2.0
                        || score == -4.0 || score == -1.0 || score == 2.0,
                    "unexpected alone score: {} for game: {}",
                    score,
                    gs
                );
            }
        }
        assert!(alone_games > 100, "should have played many alone games");
    }
}
