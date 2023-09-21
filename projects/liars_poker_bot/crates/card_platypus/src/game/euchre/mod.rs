use std::{
    collections::hash_map::DefaultHasher,
    fmt::Display,
    hash::{Hash, Hasher},
};

use anyhow::Context;
use itertools::Itertools;

use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use crate::{
    algorithms::ismcts::ResampleFromInfoState,
    alloc::Pool,
    game::{Action, Game, GameState, Player},
    istate::IStateKey,
};

use self::{
    actions::{Card, EAction, Suit, CLUBS_MASK, DIAMONDS_MASK, HEART_MASK, SPADES_MASK},
    deck::{CardLocation, Deck, Hand},
    ismorphic::iso_deck,
};

pub(super) const CARDS_PER_HAND: usize = 5;

pub mod actions;
mod deck;
pub mod ismorphic;
mod parser;
pub mod processors;

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
}

#[derive(Debug, Clone, PartialEq, Eq, Copy, Serialize, Deserialize, Hash)]
pub enum EPhase {
    DealHands,
    DealFaceUp,
    Pickup,
    /// The dealer has been told to pickup the trump suit
    Discard,
    ChooseTrump,
    Play,
}

impl EuchreGameState {
    pub fn trump_caller(&self) -> Option<Player> {
        match self.phase() {
            EPhase::Play | EPhase::Discard => Some(self.trump_caller),
            _ => None,
        }
    }

    /// Returns true if the bidding phase just ended
    pub fn bidding_ended(&self) -> bool {
        self.phase() == EPhase::Discard
            || (self.phase() == EPhase::Play
                && self.cards_played == 0
                // don't want to go into this phase twice if we discarded
                && !self.history().iter().any(|(_, a)| *a == EAction::Pickup))
    }

    /// Returns true if a trick is over. Returns false if a trick hasn't started yet
    pub fn is_trick_over(&self) -> bool {
        self.cards_played % 4 == 0 && self.cards_played > 0
    }

    pub fn last_bid(&self, player: Player) -> Option<EAction> {
        use EAction::*;
        let x = self
            .history()
            .iter()
            .rev()
            .find(|(p, a)| {
                *p == player && matches!(*a, Pickup | Pass | Clubs | Spades | Hearts | Diamonds)
            })
            .cloned();

        if let Some((_, a)) = x {
            Some(a)
        } else {
            None
        }
    }

    pub fn last_trick(&self) -> Option<(Player, [Card; 4])> {
        if self.cards_played < 4 {
            return None;
        }

        let cards_played_in_cur_trick = self.cards_played % 4;

        let sidx = self.key.len() - cards_played_in_cur_trick - 4;
        let mut trick = [Card::NS; 4];
        for (i, t) in trick.iter_mut().enumerate() {
            *t = EAction::from(self.key[sidx + i]).card();
        }

        let trick_starter = self.play_order[sidx];

        Some((trick_starter, trick))
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

        if (self.key.len() + 1) % CARDS_PER_HAND == 0 {
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
            _ => panic!("invalid action"),
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
            self.cur_player = 0;
            self.phase = EPhase::Play
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
        self.cur_player = 0;
        self.phase = EPhase::Play
    }

    fn apply_action_play(&mut self, a: Action) {
        let card = EAction::from(a).card();
        assert!(
            self.deck.get_all(self.cur_player.into()).contains(card),
            "Attempted to play card not in players hand"
        );

        // track the cards in play for isomorphic key
        self.deck.play(card, self.cur_player).unwrap();
        self.cards_played += 1;

        // Set acting player based on who won last trick
        // We can't use the trick_over function, since we need to accounts for the action that
        // hasn't yet been pushed to the action history. To accounts for this we add a +1 to key.len()
        let trick_over = self.cards_played % 4 == 0;
        // trick is over and played at least one card
        if trick_over && self.cards_played > 0 {
            let trick = self.last_trick_with_card(card).unwrap();
            let starter = (self.cur_player + 1) % self.num_players;
            let winner = self.evaluate_trick(&trick, starter);
            self.cur_player = winner;

            // save the trick winner for later
            let trick = self.cards_played / 4 - 1;
            self.trick_winners[trick] = winner;
            self.tricks_won[winner % 2] += 1;

            // clear the played cards
            for i in 0..self.num_players {
                if let Some(c) = self.deck.played(i) {
                    self.deck.set(c, CardLocation::None);
                }
            }
        } else {
            self.cur_player = (self.cur_player + 1) % self.num_players;
        }
    }

    /// Determine if current trick is over (all 4 players have played)
    /// Also returns true if none have played
    fn is_start_of_trick(&self) -> bool {
        self.cards_played % 4 == 0
    }

    /// Gets last trick with a as the final action of the trick
    fn last_trick_with_card(&self, card: Card) -> Option<[Card; 4]> {
        if self.phase() != EPhase::Play {
            return None;
        }

        if self.cards_played < 4 {
            return None;
        }

        let sidx = self.key.len() - 3;
        let mut trick = [Card::NS; 4];
        for (i, t) in trick.iter_mut().enumerate().take(3) {
            *t = EAction::from(self.key[sidx + i]).card();
        }
        trick[3] = card;

        Some(trick)
    }

    /// Get the card that started the current trick
    fn get_leading_card(&self) -> Card {
        if self.phase() != EPhase::Play {
            panic!("tried to get leading card of trick at invalid time")
        }

        let cards_played_in_trick = self.cards_played % 4;
        if cards_played_in_trick == 0 {
            panic!()
        }
        EAction::from(self.key[self.key.len() - cards_played_in_trick]).card()
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
        // Dealer can't pass
        if self.cur_player != 3 {
            actions.push(EAction::Pass.into())
        }

        let face_up = self
            .face_up()
            .expect("can't call faceup before deal finished")
            .suit();
        if face_up != Suit::Clubs {
            actions.push(EAction::Clubs.into());
        }
        if face_up != Suit::Spades {
            actions.push(EAction::Spades.into());
        }
        if face_up != Suit::Hearts {
            actions.push(EAction::Hearts.into());
        }
        if face_up != Suit::Diamonds {
            actions.push(EAction::Diamonds.into());
        }
    }

    /// Needs to consider following suit if possible
    /// Can only play cards from hand
    fn legal_actions_play(&self, actions: &mut Vec<Action>) {
        let player_loc = self.cur_player.into();
        let hand = self.deck.get_all(player_loc);
        // If they are the first to act on a trick then can play any card in hand
        if self.is_start_of_trick() {
            for c in hand {
                actions.push(EAction::from(c).into());
            }
            return;
        }

        let leading_card = self.get_leading_card();
        let leading_suit = self.get_suit(leading_card);
        let suit_mask = suit_mask(leading_suit, self.trump);

        if (suit_mask & hand).is_empty() {
            // no suit, can play any card
            for c in hand {
                actions.push(EAction::from(c).into());
            }
        } else {
            let suited_hand = suit_mask & hand;
            for c in suited_hand {
                actions.push(EAction::from(c).into());
            }
        }
    }

    /// Returns the player who won the trick
    fn evaluate_trick(&self, cards: &[Card], trick_starter: Player) -> Player {
        use Card::*;
        let (left, right) = match self.trump.unwrap() {
            Suit::Clubs => (JS, JC),
            Suit::Spades => (JC, JS),
            Suit::Hearts => (JD, JH),
            Suit::Diamonds => (JH, JD),
        };

        // right always wins
        if let Some(winner) = cards.iter().position(|c| *c == right) {
            return (trick_starter + winner) % self.num_players;
        }

        // if no right, left always wins
        if let Some(winner) = cards.iter().position(|c| *c == left) {
            return (trick_starter + winner) % self.num_players;
        }

        // otherwise we can just evaluate by rank
        let card_mask = Hand::from(cards);
        let trump_mask = suit_mask(self.trump.unwrap(), self.trump);

        let trumps = card_mask & trump_mask;
        if !trumps.is_empty() {
            let highest_card = trumps.highest().unwrap();
            return (trick_starter + cards.iter().position(|c| *c == highest_card).unwrap())
                % self.num_players;
        }

        let leading_suit = self.get_suit(cards[0]);
        let leading_mask = suit_mask(leading_suit, self.trump);
        let follow_suits = card_mask & leading_mask;
        let highest_card = follow_suits.highest().unwrap();
        return (trick_starter + cards.iter().position(|c| *c == highest_card).unwrap())
            % self.num_players;
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

        let team_0_call = self.trump_caller % 2 == 0;
        match (tricks0, tricks1, team_0_call) {
            (5, 0, _) => 2.0,
            (0, 5, _) => -2.0,
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

        for i in 0..key.len() {
            let a = EAction::from(key[i]);
            write!(f, "{}", a).unwrap();

            let append_pipe = match a {
                // dealing cards
                _ if i < 20 => (i + 1) % 5 == 0,
                // faceup
                _ if i == 20 => true,
                EAction::Pickup => true,
                EAction::Clubs | EAction::Diamonds | EAction::Hearts | EAction::Spades => {
                    first_play = Some(i + 1);
                    true
                }
                // discard action
                _ if i > 20 && is_last_take => {
                    first_play = Some(i + 1);
                    true
                }
                EAction::Pass => false,
                EAction::DiscardMarker => false,
                // everything else is Play
                _ => ((i - first_play.unwrap() + 1) % 4 == 0) && (i != first_play.unwrap()),
                // _ => false,
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
            -1.0 * self.score(future_tricks.0, future_tricks.1)
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
        if player == 3 && pickup_called {
            r.push('|');
            let a = istate[6 + num_pickups];

            let d = EAction::from(a).to_string();
            r.push_str(&d);
        }

        // populate play data
        let mut turn = 0;
        let mut i = if player == 3 && pickup_called {
            6 + num_pickups + num_calls + 1 // pickups + discard + 1 to get first play
        } else {
            6 + num_pickups + num_calls
        };
        while i < istate.len() {
            if turn % 4 == 0 {
                r.push('|');
            }

            let a = istate[i];
            let c = EAction::from(a).to_string();

            r.push_str(&c);
            turn += 1;
            i += 1;
        }

        if turn % 4 == 0 {
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
        if self.cards_played > 0 && self.cards_played % 4 == 0 {
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

            EAction::Pass => {
                // did we just undo the last pickup action?
                if self.key.len() == 20 + 1 + 3 {
                    self.phase = EPhase::Pickup;
                    let face_up = self
                        .face_up()
                        .expect("can't call faceup before deal finished");
                    self.deck.set(face_up, CardLocation::FaceUp);
                }
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
            // all that's left are play actions
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
                        .take(self.cards_played % 4)
                        .zip(self.play_order.iter().rev().take(self.cards_played % 4))
                    {
                        let c = EAction::from(*a).card();
                        self.deck.set(c, CardLocation::Played(*p));
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

        let mut hasher = DefaultHasher::new();
        let iso_deck = iso_deck(self.deck, self.trump);
        iso_deck.hash(&mut hasher);

        self.tricks_won.hash(&mut hasher);
        let calling_team = self.trump_caller % 2;
        calling_team.hash(&mut hasher);

        // Necessary for search stability of the open hand solver
        // Previous attempts with just the team being hashed don't work.
        // This is because when we hash hands at the start of tricks, it is not
        // clear from any other state who should be the first to act
        self.cur_player.hash(&mut hasher);

        Some(hasher.finish())
    }
}

/// Resample from info state method for euchre
///
/// This method discards a RANDOM card for the dealer if a discard is required.
/// It's not yet clear what impact this has on the results of downstream algorithms
impl ResampleFromInfoState for EuchreGameState {
    fn resample_from_istate<T: rand::Rng>(&self, player: Player, rng: &mut T) -> Self {
        if self.phase() == EPhase::DealHands || self.phase() == EPhase::DealFaceUp {
            panic!("don't yet support resampling of deal phase gamestates")
        }

        // Masks that track which cards players are allowed to have
        let mut allowed_cards = [Hand::all_cards(); 4];
        let mut known_cards = [Hand::default(); 4];
        let face_up = self.face_up().unwrap();
        let key = self.key();

        // collect the played cards from all players
        self.key
            .iter()
            .zip(self.play_order.iter())
            .skip(key.len() - self.cards_played)
            .map(|(a, p)| (EAction::from(*a).card(), p))
            .for_each(|(c, p)| known_cards[*p].add(c));

        // remove the face up card from every player
        allowed_cards.iter_mut().for_each(|p| p.remove(face_up));

        // and ensure the dealer isn't dealt the face up card, even if they played it
        known_cards[3].remove(face_up);

        // collect the players own dealt cards
        self.key
            .iter()
            .zip(self.play_order.iter())
            .take(20)
            .map(|(a, p)| (EAction::from(*a).card(), p))
            .filter(|(_, p)| **p == player)
            .for_each(|(c, _)| known_cards[player].add(c));

        // Remove a suit from allowed cards if player didn't previously follow suit
        let offset = key.len() - self.cards_played;
        for t in 0..5 {
            let trick_start = offset + t * 4;
            let lead = key.get(trick_start).map(|x| EAction::from(*x).card());
            if lead.is_none() {
                break;
            }

            let lead_player = self.play_order[trick_start];
            let lead_suit = self.get_suit(lead.unwrap());

            for i in 1..3 {
                if let Some(played_card) =
                    key.get(trick_start + i).map(|x| EAction::from(*x).card())
                {
                    let played_suit = self.get_suit(played_card);
                    if played_suit != lead_suit {
                        let suit_cards = suit_mask(lead_suit, self.trump);
                        allowed_cards[(lead_player + i) % 4].remove_all(suit_cards);
                    }
                }
            }
        }

        // remove the known cards for all players
        let mut all_known = Hand::default();
        known_cards.iter().for_each(|x| all_known.add_all(*x));
        allowed_cards
            .iter_mut()
            .for_each(|x| x.remove_all(all_known));

        // ensure constraints give us enough cards to deal
        // otherwise it's unsolvable
        assert!(
            known_cards
                .iter()
                .zip(allowed_cards.iter())
                // only check the first 3 players, player 4 can be unsolvable given then can discard a card
                .take(3)
                .all(|(k, a)| k.len() + a.len() >= 5),
            "Constraints aren't solvable. \nknown cards: {:?}\nallowed cards: {:?}",
            known_cards,
            allowed_cards
        );

        let mut ngs = Euchre::new_state();
        let mut pool = Pool::new(Vec::new);
        assert!(
            search_for_deal(&mut ngs, known_cards, allowed_cards, face_up, 0, rng, &mut pool),
            "Failed to find a valid deal for resample of {} for {}\nknown cards: {:?}\nallowed cards: {:?}",
            self,
            player, known_cards, allowed_cards
        );
        let mut actions = Vec::new();

        // deal the face up
        ngs.apply_action(EAction::from(face_up).into());

        // apply the non-deal actions
        let mut is_last_pickup = false;
        for a in &self.key()[21..] {
            // handle the discard case. We randomly select a card to discard that isn't seen later. TBD
            // what impact this random discarding has on the sampling because an actual player would not discard a
            // card randomly.
            //
            // If it's not a discard, we apply the actions in the order we saw them.
            // discard is the only private action after deal phase
            if is_last_pickup && player != 3 {
                assert_eq!(ngs.cur_player(), 3);

                let played_cards = self
                    .key
                    .iter()
                    .skip(key.len() - self.cards_played)
                    .map(|a| EAction::from(*a).card())
                    .collect_vec();

                ngs.legal_actions(&mut actions);
                actions.shuffle(rng);
                for da in actions.iter().map(|x| EAction::from(*x)) {
                    let card = da.card();
                    if !played_cards.contains(&card) {
                        ngs.apply_action(da.into());
                        break;
                    }
                }
            } else {
                ngs.apply_action(*a);
            }

            is_last_pickup = false;
            if EAction::from(*a) == EAction::Pickup {
                is_last_pickup = true;
            }
        }

        ngs
    }
}

/// Searches the game tree for a deal that meets all constraints
fn search_for_deal<T: rand::Rng>(
    gs: &mut EuchreGameState,
    known: [Hand; 4],
    allowed: [Hand; 4],
    face_up: Card,
    depth: usize,
    rng: &mut T,
    pool: &mut Pool<Vec<Action>>,
) -> bool {
    if !meets_constraints(gs, known, allowed) {
        return false;
    }

    if depth == 20 {
        return true;
    }

    let mut actions = pool.detach();
    gs.legal_actions(&mut actions);
    actions.shuffle(rng);

    // move a known card to the front if one exists
    let cur_player = gs.cur_player();
    let idx = actions
        .iter()
        .map(|x| EAction::from(*x).card())
        .position(|x| known[cur_player].contains(x))
        .unwrap_or(0);
    actions.swap(0, idx);

    // filter out illegal moves
    actions.retain(|x| {
        let c = EAction::from(*x).card();
        known[cur_player].contains(c) || allowed[cur_player].contains(c)
    });

    // We're in a situation where the dealer needs to be dealt a card
    // they will ultimately discard
    //
    // We can give them any remaining card and skip constraint checking
    if actions.is_empty() && depth == 19 {
        assert_eq!(gs.cur_player, 3);
        gs.legal_actions(&mut actions);
        // don't deal the faceup card
        actions.retain(|x| EAction::from(*x).card() != face_up);
        let a = actions.choose(rng).unwrap();
        gs.apply_action(*a);
        pool.attach(actions);
        return true;
    }

    for a in actions.iter() {
        gs.apply_action(*a);
        if !search_for_deal(gs, known, allowed, face_up, depth + 1, rng, pool) {
            gs.undo()
        } else {
            pool.attach(actions);
            return true;
        }
    }

    pool.attach(actions);
    false
}

fn meets_constraints(gs: &EuchreGameState, known: [Hand; 4], allowed: [Hand; 4]) -> bool {
    for p in 0..4 {
        let hand = gs.deck.get_all(p.into());
        let all_allowed = hand
            .into_iter()
            .all(|c| allowed[p].contains(c) || known[p].contains(c));
        if !all_allowed {
            return false;
        }

        let dealt_known = hand.into_iter().filter(|c| known[p].contains(*c)).count();
        let dealt_known_first = dealt_known == hand.len() || dealt_known == known[p].len();
        if !dealt_known_first {
            return false;
        }
    }

    true
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
    use rand::{rngs::StdRng, seq::SliceRandom, thread_rng, SeedableRng};

    use crate::{
        actions,
        agents::{Agent, RandomAgent},
        algorithms::ismcts::ResampleFromInfoState,
        game::euchre::{actions::Card, deck::CARDS, EAction, EPhase, Euchre, Suit},
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
        assert_eq!(s.cur_player, 0);

        assert_eq!(s.phase(), EPhase::Play);
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

        // Cards player 0s hand
        assert_eq!(gs.phase(), EPhase::Play);
        assert_eq!(
            actions!(gs)
                .iter()
                .map(|x| EAction::from(*x).to_string())
                .collect_vec(),
            vec!["9c", "Tc", "Jc", "Qc", "Kc"],
            "gs: {}",
            gs
        );

        gs.apply_action(EAction::NC.into());
        // Player 1 must follow suit
        assert_eq!(
            actions!(gs)
                .iter()
                .map(|x| EAction::from(*x).to_string())
                .collect_vec(),
            vec!["Ac"],
            "gs: {}",
            gs
        );

        let gs = EuchreGameState::from("TcQs9hJdQd|QcThJhKhKd|AcTsAhTdAd|9cKc9sKsQh|Jc|T|Kc|QdKd");
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
        assert_eq!(s.phase(), EPhase::Play);
        // Jack of spades is now a club since it's trump
        assert_eq!(s.get_suit(Card::JS), Suit::Clubs);
        assert_eq!(s.get_suit(Card::TS), Suit::Spades);
    }

    #[test]
    fn euchre_test_istate() {
        let mut gs = EuchreGameState::from("9cTcJcQcKc|Ac9sTsJdQs|KsAs9hThJh|QhKhAh9dTd");

        assert_eq!(gs.istate_string(0), "9cTcJcQcKc");
        assert_eq!(gs.istate_string(1), "Ac9sTsQsJd");
        assert_eq!(gs.istate_string(2), "KsAs9hThJh");
        assert_eq!(gs.istate_string(3), "QhKhAh9dTd");

        gs.apply_action(EAction::from(Card::JS).into());
        assert_eq!(gs.istate_string(0), "9cTcJcQcKc|Js|");
        assert_eq!(gs.istate_string(1), "Ac9sTsQsJd|Js|");
        assert_eq!(gs.istate_string(2), "KsAs9hThJh|Js|");
        assert_eq!(gs.istate_string(3), "QhKhAh9dTd|Js|");

        let mut new_s = gs.clone(); // for alternative pickup parsing

        gs.apply_action(EAction::Pickup.into());
        assert_eq!(gs.istate_string(0), "9cTcJcQcKc|Js|T|0S");

        // Dealer discards the QH
        assert_eq!(gs.istate_string(3), "QhKhAh9dTd|Js|T|0S");
        gs.apply_action(EAction::QH.into());
        assert_eq!(gs.istate_string(3), "QhKhAh9dTd|Js|T|0S|Qh|");

        for _ in 0..4 {
            let a = actions!(gs)[0];
            gs.apply_action(a);
        }
        assert_eq!(gs.istate_string(0), "9cTcJcQcKc|Js|T|0S|9cAcKsJs|");
        assert_eq!(gs.istate_string(1), "Ac9sTsQsJd|Js|T|0S|9cAcKsJs|");
        assert_eq!(gs.istate_string(2), "KsAs9hThJh|Js|T|0S|9cAcKsJs|");
        assert_eq!(gs.istate_string(3), "QhKhAh9dTd|Js|T|0S|Qh|9cAcKsJs|");
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
        assert_eq!(new_s.istate_string(0), "9cTcJcQcKc|Js|PPPPPH|1H|");
    }

    #[test]
    fn euchre_test_unique_istate() {
        let mut ra = RandomAgent::new();

        for _ in 0..1000 {
            let mut s = Euchre::new_state();
            let mut istates = HashSet::new();
            while s.is_chance_node() {
                let a = ra.step(&s);
                s.apply_action(a);
            }

            istates.insert(s.istate_string(s.cur_player));
            while !s.is_terminal() {
                let a = ra.step(&s);
                s.apply_action(a);
                let istate = s.istate_string(s.cur_player);
                assert!(!istates.contains(&istate));
                istates.insert(istate);
            }
        }
    }

    #[test]
    fn euchre_test_resample_from_istate() {
        let mut ra = RandomAgent::new();
        let mut rng = thread_rng();

        for _ in 0..100 {
            let mut s = Euchre::new_state();

            while s.is_chance_node() {
                let a = ra.step(&s);
                s.apply_action(a);
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

                let a = ra.step(&s);
                s.apply_action(a);
            }
        }

        for _ in 0..100 {
            // this is a hard case where the dealer discards a card and doesn't follow suit because of it
            let gs =
        EuchreGameState::from("AcTsThTdJd|QcJs9hKh9d|Kc9sAsQdAd|9cTcJcQsJh|Ks|PPPT|Tc|Td9dAdJh|QdJcJdKh|QsTsJs9s|9hAs9cTh|KcKs");
            gs.resample_from_istate(2, &mut thread_rng());

            let gs =
        EuchreGameState::from("9cTcAc9s9d|Jc9hJhTdKd|TsQsKsJdQd|QcKcAsQhAd|Js|PPPT|Qh|9dKdJdAd|QcAcJcQd|9hTsJs9c|As9sJhQs|KcTc");
            gs.resample_from_istate(2, &mut thread_rng());
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
        let gs = EuchreGameState::from("9cJcQcTsTd|KcKsQhKh9d|TcAcQsAsTh|Js9hAhQdAd|Kd|PT|Js|");
        let rng: StdRng = SeedableRng::seed_from_u64(42);
        let sampled = gs.resample_from_istate(gs.cur_player(), &mut rng.clone());

        for _ in 0..100 {
            assert_eq!(
                gs.resample_from_istate(gs.cur_player(), &mut rng.clone()),
                sampled
            )
        }
    }
}
