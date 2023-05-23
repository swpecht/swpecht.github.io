use std::fmt::Display;

use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use crate::{
    actions,
    algorithms::ismcts::ResampleFromInfoState,
    game::{Action, Game, GameState, Player},
    istate::IStateKey,
};

use self::{
    actions::{Card, EAction, Suit},
    deck::{CardLocation, Deck},
};

pub(super) const CARDS_PER_HAND: usize = 5;

pub mod actions;
mod deck;
pub mod terminators;

pub struct Euchre {}
impl Euchre {
    pub fn new_state() -> EuchreGameState {
        EuchreGameState {
            num_players: 4,
            cur_player: 0,
            trump: Suit::Clubs, // Default to one for now
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EuchreGameState {
    num_players: usize,
    trump: Suit,
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

#[derive(Debug, Clone, PartialEq, Eq, Copy, Serialize, Deserialize)]
enum EPhase {
    DealHands,
    DealFaceUp,
    Pickup,
    /// The dealer has been told to pickup the trump suit
    Discard,
    ChooseTrump,
    Play,
}

impl EuchreGameState {
    fn apply_action_deal_hands(&mut self, a: Action) {
        let card = EAction::from(a).card();

        if self.deck[card] != CardLocation::None {
            panic!(
                "attempted to deal {} which has already been dealt to {:?}",
                card, self.deck[card]
            )
        }
        self.deck[card] = self.cur_player.into();

        if (self.key.len() + 1) % CARDS_PER_HAND == 0 {
            self.cur_player = (self.cur_player + 1) % self.num_players
        }

        if self.key.len() == 19 {
            self.phase = EPhase::DealFaceUp;
        }
    }

    fn apply_action_deal_face_up(&mut self, a: Action) {
        if let EAction::DealFaceUp { c } = a.into() {
            if self.deck[c] != CardLocation::None {
                panic!(
                    "attempting to deal a card that was already dealt: {}, {:?}",
                    c, self.deck
                );
            }
            self.deck[c] = CardLocation::FaceUp;
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
                }
                self.cur_player = (self.cur_player + 1) % self.num_players;
            }
            EAction::Pickup => {
                self.trump_caller = self.cur_player;
                self.trump = self.face_up().suit();
                self.cur_player = 3; // dealers turn
                self.phase = EPhase::Discard;
            }
            _ => panic!("invalid action"),
        }
    }

    fn apply_action_choose_trump(&mut self, a: Action) {
        let a = EAction::from(a);
        match a {
            EAction::Clubs => self.trump = Suit::Clubs,
            EAction::Spades => self.trump = Suit::Spades,
            EAction::Hearts => self.trump = Suit::Hearts,
            EAction::Diamonds => self.trump = Suit::Diamonds,
            EAction::Pass => {}
            _ => panic!("invalid action"),
        };

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
        if self.deck[discard] != CardLocation::Player3 {
            panic!(
                "attempting to discard a card not in dealers hand: {}\n{:?}",
                discard, self.deck
            )
        }
        self.deck[discard] = CardLocation::None; // dealer

        let pickup = self.face_up();
        self.deck[pickup] = CardLocation::Player3;

        self.cur_player = 0;
        self.phase = EPhase::Play
    }

    fn apply_action_play(&mut self, a: Action) {
        let card = EAction::from(a).card();
        self.deck[card] = CardLocation::None;
        self.cards_played += 1;

        // Set acting player based on who won last trick
        // We can't use the trick_over function, since we need to accounts for the action that
        // hasn't yet been pushed to the action history. To accounts for this we add a +1 to key.len()
        let trick_over = self.cards_played % 4 == 0;
        // trick is over and played at least one card
        if trick_over && self.cards_played > 0 {
            let trick = self.get_last_trick(card);
            let starter = (self.cur_player + 1) % self.num_players;
            let winner = self.evaluate_trick(&trick, starter);
            self.cur_player = winner;

            // save the trick winner for later
            let trick = self.cards_played / 4 - 1;
            self.trick_winners[trick] = winner;
            self.tricks_won[winner % 2] += 1;
        } else {
            self.cur_player = (self.cur_player + 1) % self.num_players;
        }
    }

    /// Determine if current trick is over (all 4 players have played)
    /// Also returns true if none have played
    fn is_trick_over(&self) -> bool {
        self.cards_played % 4 == 0
    }

    /// Gets last trick with a as the final action of the trick
    fn get_last_trick(&self, card: Card) -> [Card; 4] {
        let sidx = self.key.len() - 3;
        let mut trick = [Card::NS; 4];
        for (i, t) in trick.iter_mut().enumerate().take(3) {
            *t = EAction::from(self.key[sidx + i]).card();
        }
        trick[3] = card;

        trick
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
        for (i, &loc) in self.deck.iter().enumerate() {
            if loc == CardLocation::None {
                actions.push(
                    EAction::DealPlayer {
                        c: (i as u8).into(),
                    }
                    .into(),
                );
            }
        }
    }

    fn legal_actions_deal_face_up(&self, actions: &mut Vec<Action>) {
        for (i, &loc) in self.deck.iter().enumerate() {
            if loc == CardLocation::None {
                actions.push(
                    EAction::DealFaceUp {
                        c: (i as u8).into(),
                    }
                    .into(),
                );
            }
        }
    }

    /// Can choose any trump except for the one from the faceup card
    /// For the dealer they aren't able to pass.
    fn legal_actions_choose_trump(&self, actions: &mut Vec<Action>) {
        // Dealer can't pass
        if self.cur_player != 3 {
            actions.push(EAction::Pass.into())
        }

        let face_up = self.face_up().suit();
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
        // If they are the first to act on a trick then can play any card in hand
        if self.is_trick_over() {
            for (i, &loc) in self.deck.iter().enumerate() {
                if loc == player_loc {
                    let c = (i as u8).into();
                    actions.push(EAction::Play { c }.into());
                }
            }
            return;
        }

        let leading_card = self.get_leading_card();
        let leading_suit = self.get_suit(leading_card);
        for (i, &loc) in self.deck.iter().enumerate() {
            // We check if the player has the card before the suit to avoid the more
            // expensive get_suit call
            let c = (i as u8).into();
            if loc == player_loc && self.get_suit(c) == leading_suit {
                actions.push(EAction::Play { c }.into());
            }
        }

        if actions.is_empty() {
            // no suit, can play any card
            for (i, &loc) in self.deck.iter().enumerate() {
                if loc == player_loc {
                    let c = (i as u8).into();
                    actions.push(EAction::Play { c }.into());
                }
            }
        }
    }

    /// Returns the player who won the trick
    fn evaluate_trick(&self, cards: &[Card], trick_starter: Player) -> Player {
        assert_eq!(cards.len(), 4); // only support 4 players

        let mut winner = 0;
        let mut winning_card = cards[0];
        let mut winning_suit = self.get_suit(cards[0]);
        for (i, &c) in cards.iter().enumerate() {
            let suit = self.get_suit(c);
            // Player can't win if not following suit or playing trump
            // The winning suit can only ever be trump or the lead suit
            if suit != winning_suit && suit != self.trump {
                continue;
            }

            // Simple case where we don't need to worry about weird trump scoring
            if suit == winning_suit
                && suit != self.trump
                && self.get_card_value(c) > self.get_card_value(winning_card)
            {
                winner = i;
                winning_card = c;
                winning_suit = suit;
                continue;
            }

            // Play trump over lead suit
            if suit == self.trump && winning_suit != self.trump {
                winner = i;
                winning_card = c;
                winning_suit = suit;
                continue;
            }

            // Handle trump scoring. Need to differentiate the left and right
            if suit == self.trump && winning_suit == self.trump {
                let winning_card_value = self.get_card_value(winning_card);
                let cur_card_value = self.get_card_value(c);
                if cur_card_value > winning_card_value {
                    winner = i;
                    winning_card = c;
                    winning_suit = suit;
                    continue;
                }
            }
        }

        (trick_starter + winner) % self.num_players
    }

    /// Gets the suit of a given card. Accounts for the weird scoring of the trump suit
    /// if in the playing phase of the game
    fn get_suit(&self, c: Card) -> Suit {
        let mut suit = c.suit();

        let is_jack = (c == Card::JC) || (c == Card::JS) || (c == Card::JD) || (c == Card::JH);
        if !is_jack {
            return suit;
        }

        // Correct the jack if in play phase
        if self.phase() == EPhase::Play {
            suit = match (c, self.trump) {
                (Card::JC, Suit::Spades) => Suit::Spades,
                (Card::JS, Suit::Clubs) => Suit::Clubs,
                (Card::JH, Suit::Diamonds) => Suit::Diamonds,
                (Card::JD, Suit::Hearts) => Suit::Hearts,
                _ => suit,
            }
        }
        suit
    }

    /// Returns a relative value for cards. The absolute values are meaningyless
    /// but can be used to compare card values of the same suit. It accounts for
    /// left and right jack.
    fn get_card_value(&self, card: Card) -> usize {
        let rank = card.rank();

        if self.phase() != EPhase::Play {
            return rank as usize;
        }

        match (self.trump, card) {
            (Suit::Clubs, Card::JC) => 100,
            (Suit::Clubs, Card::JS) => 99,
            (Suit::Spades, Card::JS) => 100,
            (Suit::Spades, Card::JC) => 99,
            (Suit::Hearts, Card::JH) => 100,
            (Suit::Hearts, Card::JD) => 99,
            (Suit::Diamonds, Card::JD) => 100,
            (Suit::Diamonds, Card::JH) => 99,
            _ => rank as usize,
        }
    }

    fn update_keys(&mut self, a: Action) {
        self.key.push(a);
    }

    fn phase(&self) -> EPhase {
        self.phase
    }

    fn face_up(&self) -> Card {
        // read the value from the deck
        // if it's not there, we're probably calling this to rewind, look through the
        // action history to find it
        for (i, loc) in self.deck.iter().enumerate() {
            if *loc == CardLocation::FaceUp {
                return (i as u8).into();
            }
        }

        for a in self.key {
            if let EAction::DealFaceUp { c } = EAction::from(a) {
                return c;
            }
        }

        panic!("couldn't find a face up card in deck or action history")
    }
}

impl Display for EuchreGameState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let key = &self.key();
        let mut first_play = None;

        for i in 0..key.len() {
            let a = EAction::from(key[i]);
            write!(f, "{}", a).unwrap();

            if matches!(a, EAction::Play { c: _ }) && first_play.is_none() {
                first_play = Some(i);
            }

            let append_pipe = match a {
                EAction::DealPlayer { c: _ } => (i + 1) % 5 == 0,
                EAction::DealFaceUp { c: _ } => true,
                EAction::Pickup => true,
                EAction::Clubs | EAction::Diamonds | EAction::Hearts | EAction::Spades => true,
                EAction::Play { c: _ } => {
                    (i - first_play.unwrap() + 1) % 4 == 0 && i != first_play.unwrap()
                }

                EAction::Discard { c: _ } => true,
                EAction::Pass => false,
            };
            if append_pipe {
                write!(f, "|").unwrap();
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
                actions.append(&mut vec![EAction::Pass.into(), EAction::Pickup.into()])
            }
            EPhase::Discard => {
                // Dealer can discard any card
                for (i, loc) in self.deck.iter().enumerate() {
                    if *loc == CardLocation::Player3 {
                        let c = (i as u8).into();
                        actions.push(EAction::Discard { c }.into());
                    }
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

        // no points, didn't win most tricks
        if self.tricks_won[team] < self.tricks_won[(team + 1) % 2] {
            0.0
        } else if self.tricks_won[team] == 5 {
            2.0
        } else if self.trump_caller % 2 == team {
            1.0
        } else {
            //euchred them
            2.0
        }
    }

    fn istate_key(&self, player: Player) -> IStateKey {
        let mut istate = IStateKey::default();

        for (p, a) in self.play_order.iter().zip(self.key.iter()) {
            let is_visible = match EAction::from(*a) {
                EAction::Pickup => true,
                EAction::Pass => true,
                EAction::Clubs => true,
                EAction::Spades => true,
                EAction::Hearts => true,
                EAction::Diamonds => true,
                EAction::DealPlayer { c: _ } => player == *p,
                EAction::DealFaceUp { c: _ } => true,
                EAction::Discard { c: _ } => player == 3, // dealer can see
                EAction::Play { c: _ } => true,
            };

            if is_visible {
                istate.push(*a)
            }
        }
        istate.sort_range(0, CARDS_PER_HAND.min(istate.len()));

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

        r.push_str(&format!("{}{}", self.trump_caller, self.trump));

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

        r
    }

    fn is_terminal(&self) -> bool {
        self.cards_played == 20
        // Check if the scores are already decided: see if have taken a trick in defence
        || self.tricks_won[0] > 0 && self.tricks_won[1] >= 3
        || self.tricks_won[0] >= 3 && self.tricks_won[1] > 0
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
        let applied_action = EAction::from(self.key.pop());

        // fix the trick winner counts
        if self.cards_played > 0 && self.cards_played % 4 == 0 {
            let trick = self.cards_played / 4 - 1;
            let last_winner = self.trick_winners[trick];
            self.trick_winners[trick] = 0; // reset it
            self.tricks_won[last_winner % 2] -= 1;
        }

        match applied_action {
            EAction::Pass => {
                // did we just undo the last pickup action?
                if self.key.len() == 20 + 1 + 3 {
                    self.phase = EPhase::Pickup;
                }
            }
            EAction::Clubs | EAction::Spades | EAction::Hearts | EAction::Diamonds => {
                self.phase = EPhase::ChooseTrump;
                // return to defaults
                self.trump_caller = 0;
                self.trump = Suit::Clubs;
            }
            EAction::Pickup => {
                self.phase = EPhase::Pickup;
                // return to defaults
                self.trump_caller = 0;
                self.trump = Suit::Clubs;
            }
            EAction::DealPlayer { c } => {
                self.deck[c] = CardLocation::None;
                self.phase = EPhase::DealHands
            }
            EAction::DealFaceUp { c } => {
                self.deck[c] = CardLocation::None;
                self.phase = EPhase::DealFaceUp;
            }
            EAction::Discard { c } => {
                let face_up = self.face_up();
                self.deck[face_up] = CardLocation::FaceUp; // card is face up again
                self.deck[c] = CardLocation::Player3;
                self.phase = EPhase::Discard;
            }
            EAction::Play { c } => {
                self.deck[c] = self.cur_player.into();
                self.cards_played -= 1;
            }
        }
    }
}

/// Resample from info state method for euchre
///
/// This method discards a RANDOM card for the dealer if a discard is required.
/// It's not yet clear what impact this has on the results of downstream algorithms
impl ResampleFromInfoState for EuchreGameState {
    fn resample_from_istate<T: rand::Rng>(&self, player: Player, rng: &mut T) -> Self {
        let mut player_chance = [Card::AC; 5];

        for (i, c) in player_chance.iter_mut().enumerate() {
            *c = EAction::from(self.key[player * CARDS_PER_HAND + i]).card();
        }

        if self.phase() == EPhase::Play
            || self.phase() == EPhase::DealHands
            || self.phase() == EPhase::DealFaceUp
        {
            todo!(
                "implement resampling for play and post discard phases: {:?}",
                self
            );
        }

        let mut ngs = Euchre::new_state();
        let mut chance_iter = player_chance.iter();
        let face_up_chance = EAction::from(self.key[20]).card(); // 21st card dealt

        for i in 0..self.key.len() {
            if i >= player * CARDS_PER_HAND && i < player * CARDS_PER_HAND + CARDS_PER_HAND {
                // the player chance node
                ngs.apply_action(
                    EAction::DealPlayer {
                        c: *chance_iter.next().unwrap(),
                    }
                    .into(),
                );
            } else if i == 20 {
                // the faceup chance node
                ngs.apply_action(EAction::DealFaceUp { c: face_up_chance }.into());
            } else if ngs.is_chance_node() {
                assert!(ngs.cur_player() != player);

                // other player chance node
                let mut actions = actions!(ngs);
                actions.shuffle(rng);
                for a in actions {
                    let card = EAction::from(a).card();
                    if !player_chance.contains(&card) && card != face_up_chance {
                        // can't deal same card
                        ngs.apply_action(a);
                        break;
                    }
                }
            } else {
                // public history gets repeated
                ngs.apply_action(self.key[i]);
            }
        }
        ngs
    }
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
        game::euchre::{actions::Card, EAction, EPhase, Euchre, Suit},
    };

    use super::GameState;

    #[test]
    fn euchre_test_phases_choose_trump() {
        let mut s = Euchre::new_state();

        assert_eq!(s.phase(), EPhase::DealHands);
        for i in 0..20 {
            s.apply_action(EAction::DealPlayer { c: Card::from(i) }.into());
        }

        assert_eq!(s.phase(), EPhase::DealFaceUp);
        s.apply_action(EAction::DealFaceUp { c: Card::from(20) }.into());

        assert_eq!(s.phase(), EPhase::Pickup);
        assert!(!s.is_chance_node());
        for i in 0..4 {
            assert_eq!(s.cur_player, i);
            s.apply_action(EAction::Pass.into());
        }

        assert_eq!(s.phase(), EPhase::ChooseTrump);
        assert_eq!(s.cur_player, 0);
        s.apply_action(EAction::Pass.into());
        s.apply_action(EAction::Diamonds.into());
        assert_eq!(s.cur_player, 0);

        assert_eq!(s.phase(), EPhase::Play);
    }

    #[test]
    fn euchre_test_phases_pickup() {
        let mut s = Euchre::new_state();

        assert_eq!(s.phase(), EPhase::DealHands);
        for i in 0..20 {
            s.apply_action(EAction::DealPlayer { c: i.into() }.into());
        }

        assert_eq!(s.phase(), EPhase::DealFaceUp);
        s.apply_action(EAction::DealFaceUp { c: 20.into() }.into());

        assert_eq!(s.phase(), EPhase::Pickup);
        assert!(!s.is_chance_node());
        for _ in 0..3 {
            s.apply_action(EAction::Pass.into());
        }
        s.apply_action(EAction::Pickup.into());

        assert_eq!(s.phase(), EPhase::Discard);
        s.apply_action(EAction::Discard { c: Card::from(19) }.into());

        assert_eq!(s.phase(), EPhase::Play);
        assert_eq!(s.cur_player, 0);
    }

    #[test]
    fn euchre_test_legal_actions() {
        let mut gs = Euchre::new_state();

        for i in 0..20 {
            gs.apply_action(EAction::DealPlayer { c: i.into() }.into());
            let legal = actions!(gs);
            for j in 0..i + 1 {
                assert!(!legal.contains(&EAction::DealPlayer { c: j.into() }.into()));
            }
        }

        // Deal the face up card
        gs.apply_action(EAction::DealFaceUp { c: 21.into() }.into());
        assert_eq!(gs.face_up(), 21.into());

        assert_eq!(
            actions!(gs),
            vec![EAction::Pass.into(), EAction::Pickup.into()]
        );

        gs.apply_action(EAction::Pickup.into());
        // Cards in dealers hand
        assert_eq!(
            actions!(gs)
                .iter()
                .map(|x| EAction::from(*x).to_string())
                .collect_vec(),
            vec!["QH", "KH", "AH", "9D", "TD"]
        );
        assert_eq!(gs.phase(), EPhase::Discard);
        gs.apply_action(EAction::Discard { c: Card::QH }.into());

        // Cards player 0s hand
        assert_eq!(gs.phase(), EPhase::Play);
        assert_eq!(
            actions!(gs)
                .iter()
                .map(|x| EAction::from(*x).to_string())
                .collect_vec(),
            vec!["9C", "TC", "JC", "QC", "KC"]
        );

        gs.apply_action(EAction::Play { c: Card::NC }.into());
        // Player 1 must follow suit
        assert_eq!(
            actions!(gs)
                .iter()
                .map(|x| EAction::from(*x).to_string())
                .collect_vec(),
            vec!["AC"]
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
        for i in 1..21 {
            s.apply_action(EAction::DealPlayer { c: i.into() }.into());
        }

        s.apply_action(EAction::DealFaceUp { c: Card::NC }.into()); // Deal the 9 face up
        s.apply_action(EAction::Pickup.into());
        s.apply_action(EAction::Discard { c: 20.into() }.into());
        assert_eq!(s.trump, Suit::Clubs);
        assert_eq!(s.phase(), EPhase::Play);
        // Jack of spades is now a club since it's trump
        assert_eq!(s.get_suit(Card::JS), Suit::Clubs);
        assert_eq!(s.get_suit(Card::TS), Suit::Spades);
    }

    #[test]
    fn euchre_test_istate() {
        let mut gs = Euchre::new_state();
        // Deal the cards
        for i in 0..20 {
            gs.apply_action(EAction::DealPlayer { c: i.into() }.into());
        }

        assert_eq!(gs.istate_string(0), "9CTCJCQCKC");
        assert_eq!(gs.istate_string(1), "AC9STSJSQS");
        assert_eq!(gs.istate_string(2), "KSAS9HTHJH");
        assert_eq!(gs.istate_string(3), "QHKHAH9DTD");

        gs.apply_action(EAction::DealFaceUp { c: 20.into() }.into());
        assert_eq!(gs.istate_string(0), "9CTCJCQCKC|JD|");
        assert_eq!(gs.istate_string(1), "AC9STSJSQS|JD|");
        assert_eq!(gs.istate_string(2), "KSAS9HTHJH|JD|");
        assert_eq!(gs.istate_string(3), "QHKHAH9DTD|JD|");

        let mut new_s = gs.clone(); // for alternative pickup parsing

        gs.apply_action(EAction::Pickup.into());
        assert_eq!(gs.istate_string(0), "9CTCJCQCKC|JD|T|0D");

        // Dealer discards the QH
        assert_eq!(gs.istate_string(3), "QHKHAH9DTD|JD|T|0D");
        gs.apply_action(EAction::Discard { c: Card::QH }.into());
        assert_eq!(gs.istate_string(3), "QHKHAH9DTD|JD|T|0D|QH");

        for _ in 0..4 {
            let a = actions!(gs)[0];
            gs.apply_action(a);
        }
        assert_eq!(gs.istate_string(0), "9CTCJCQCKC|JD|T|0D|9CACKSKH");
        assert_eq!(gs.istate_string(1), "AC9STSJSQS|JD|T|0D|9CACKSKH");
        assert_eq!(gs.istate_string(2), "KSAS9HTHJH|JD|T|0D|9CACKSKH");
        assert_eq!(gs.istate_string(3), "QHKHAH9DTD|JD|T|0D|QH|9CACKSKH");
        assert_eq!(gs.cur_player(), 1);

        while !gs.is_terminal() {
            let a = actions!(gs)[0];
            gs.apply_action(a);
            gs.istate_string(0);
        }
        assert_eq!(gs.evaluate(0), 0.0);
        assert_eq!(gs.evaluate(1), 2.0);
        assert_eq!(gs.evaluate(2), 0.0);
        assert_eq!(gs.evaluate(3), 2.0);

        // Different calling path
        for _ in 0..5 {
            new_s.apply_action(EAction::Pass.into());
        }
        new_s.apply_action(EAction::Hearts.into());
        assert_eq!(new_s.istate_string(0), "9CTCJCQCKC|JD|PPPPPH|1H");
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

        for _ in 0..1000 {
            let mut s = Euchre::new_state();

            while s.is_chance_node() {
                let a = ra.step(&s);
                s.apply_action(a);
            }

            while !s.is_terminal() {
                for p in 0..s.num_players() {
                    let original_istate = s.istate_key(p);
                    for _ in 0..100 {
                        let sampled_state = s.resample_from_istate(p, &mut rng);
                        let sampled_key = sampled_state.istate_key(p);
                        assert_eq!(sampled_key, original_istate)
                    }
                }

                let a = ra.step(&s);
                s.apply_action(a);
            }
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
}
