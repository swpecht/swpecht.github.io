use std::{collections::HashSet, f64::consts::E, fmt::Display};

use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use crate::{
    actions,
    algorithms::ismcts::ResampleFromInfoState,
    game::{Action, Game, GameState, Player},
    istate::IStateKey,
};

use self::{
    actions::{Card, EAction, Suit, CARD_PER_SUIT},
    deck::{CardLocation, Deck},
    parser::EuchreParser,
};

const NUM_CARDS: usize = 24;
pub(super) const CARDS_PER_HAND: usize = 5;

pub mod actions;
mod deck;
mod parser;

pub struct Euchre {}
impl Euchre {
    pub fn new_state() -> EuchreGameState {
        let hands: [HashSet<Action>; 4] = Default::default();

        EuchreGameState {
            num_players: 4,
            hands,
            cur_player: 0,
            trump: Suit::Clubs, // Default to one for now
            trump_caller: 0,
            first_played: None,
            discard: None,
            trick_winners: [0; 5],
            key: IStateKey::default(),
            parser: EuchreParser::default(),
            play_order: Vec::new(),
            deck: Deck::default(),
            history: Vec::new(),
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
    /// Holds the cards for each player in the game
    hands: [HashSet<Action>; 4],
    trump: Suit,
    trump_caller: usize,
    cur_player: usize,
    /// index of the game key where the first played card is
    /// used to make looking up tricks easier
    first_played: Option<usize>,
    /// index of the discard action in the game key if one occured
    discard: Option<usize>,
    /// keep track of who has won tricks to avoid re-computing
    trick_winners: [Player; 5],
    key: IStateKey,
    parser: EuchreParser,
    play_order: Vec<Player>, // tracker of who went in what order. Last item is the current player
    deck: Deck,
    /// Running history of what each player did
    history: Vec<(EAction, Player)>,
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
        self.hands[self.cur_player].insert(a);
        assert!(self.hands[self.cur_player].len() <= 5);

        if self.hands[self.cur_player].len() == CARDS_PER_HAND {
            self.cur_player = (self.cur_player + 1) % self.num_players
        }
    }

    fn apply_action_deal_face_up(&mut self, a: Action) {
        let card = EAction::from(a).card();
        self.deck[card] = CardLocation::FaceUp;
        self.cur_player = 0;
    }

    fn apply_action_pickup(&mut self, a: Action) {
        match EAction::from(a) {
            EAction::Pass => {
                self.cur_player = (self.cur_player + 1) % self.num_players;
            }
            EAction::Pickup => {
                self.trump_caller = self.cur_player;
                self.trump = self.face_up().suit();
                self.cur_player = 3; // dealers turn
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
        }
    }

    /// Can only be done by the dealer (player 3)
    fn apply_action_discard(&mut self, a: Action) {
        let discard = EAction::from(a).card();
        assert!(self.deck[discard] == CardLocation::Player3);
        self.deck[discard] = CardLocation::None; // dealer

        let pickup = self.face_up();
        self.deck[pickup] = CardLocation::Player3;

        self.cur_player = 0;
    }

    fn apply_action_play(&mut self, a: Action) {
        if self.first_played.is_none() {
            self.first_played = Some(self.key.len());
        }

        self.hands[self.cur_player].remove(&a);

        // Set acting player based on who won last trick
        let trick_over = self.is_trick_over();
        let num_cards = self.hands[0].len();
        // trick is over and played at least one card
        if trick_over && num_cards < 5 {
            let card = EAction::from(a).card();
            let trick = self.get_last_trick(card);
            let starter = (self.cur_player + 1) % self.num_players;
            let winner = self.evaluate_trick(&trick, starter);
            self.cur_player = winner;

            // save the trick winner for later
            let trick = ((self.key.len() + 1 - self.first_played.unwrap()) / 4) - 1;
            self.trick_winners[trick] = winner;
        } else {
            self.cur_player = (self.cur_player + 1) % self.num_players;
        }
    }

    /// Determine if current trick is over (all 4 players have played)
    /// Also returns true if none have played
    fn is_trick_over(&self) -> bool {
        // if no one has played yet
        if self.first_played.is_none() {
            return true;
        }

        (self.key.len() - self.first_played.unwrap() + 1) % 4 == 0
    }

    /// Gets last trick with a as the final action of the trick
    fn get_last_trick(&self, card: Card) -> [Card; 4] {
        if !self.is_trick_over() {
            panic!("cannot get trick unless the trick is over");
        }

        let sidx = self.key.len() - 3;
        let mut trick = [Card::NS; 4];
        for i in 0..3 {
            trick[i] = EAction::from(self.key[sidx + i]).card();
        }
        trick[3] = card;

        trick
    }

    /// Get the card that started the current trick
    fn get_leading_card(&self) -> Card {
        if self.phase() != EPhase::Play {
            panic!("tried to get leading card of trick at invalid time")
        }

        let min_hand = self.hands.iter().map(|x| x.len()).min().unwrap();
        let cards_played = self.hands.iter().filter(|&x| x.len() == min_hand).count();
        EAction::from(self.key[self.key.len() - cards_played]).card()
    }

    fn legal_actions_dealing(&self, actions: &mut Vec<Action>) {
        for (c, &loc) in &self.deck {
            if loc == CardLocation::None {
                actions.push(EAction::DealPlayer { c }.into());
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
        let player_loc = match self.cur_player {
            0 => CardLocation::Player0,
            1 => CardLocation::Player1,
            2 => CardLocation::Player2,
            3 => CardLocation::Player3,
            _ => panic!("invalid player: {}", self.cur_player),
        };

        // If they are the first to act on a trick then can play any card in hand
        if self.is_trick_over() {
            for (c, loc) in &self.deck {
                if *loc == player_loc {
                    actions.push(EAction::Play { c }.into());
                }
            }
            return;
        }

        let leading_card = self.get_leading_card();
        let leading_suit = self.get_suit(leading_card);
        for (c, loc) in &self.deck {
            if self.get_suit(c) == leading_suit && *loc == player_loc {
                actions.push(EAction::Play { c }.into());
            }
        }

        if actions.is_empty() {
            // no suit, can play any card
            for (c, loc) in &self.deck {
                if *loc == player_loc {
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
        self.parser.consume(a);
    }

    fn phase(&self) -> EPhase {
        match self.parser.history[self.parser.history.len() - 1] {
            parser::EuchreParserState::DealPlayers(_) => EPhase::DealHands,
            parser::EuchreParserState::DealFaceUp => EPhase::DealFaceUp,
            parser::EuchreParserState::Discard => EPhase::Discard,
            parser::EuchreParserState::PickupChoice(_) => EPhase::Pickup,
            parser::EuchreParserState::CallChoice(_) => EPhase::ChooseTrump,
            parser::EuchreParserState::Play(_) => EPhase::Play,
            parser::EuchreParserState::Terminal => EPhase::Play,
        }
    }

    fn face_up(&self) -> Card {
        // read the value from the deck
        // if it's not there, we're probably calling this to rewind, look through the
        // action history to find it
        todo!()
    }
}

impl Display for EuchreGameState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let key = &self.key;
        let mut n = write_phase_deal_hands(f, key, 0);

        if n < key.len() {
            n = write_phase_pickup_call(f, key, n);
        }
        if n < key.len() {
            write!(f, "|").unwrap();
            write_phase_play(f, key, n);
        }
        write!(f, "")
    }
}

/// Returns the index after the last written index
fn write_phase_deal_hands(f: &mut std::fmt::Formatter<'_>, key: &IStateKey, start: usize) -> usize {
    for i in start..20 {
        if i == key.len() {
            return i;
        }

        if i != start && (i - start) % 5 == 0 {
            write!(f, "|").unwrap()
        }
        write!(f, "{}", EAction::from(key[i])).unwrap();
    }

    let face_up_index = start + 20;
    if key.len() >= face_up_index {
        write!(f, "|").unwrap();
        write!(f, "{}", EAction::from(key[face_up_index])).unwrap();
    }

    write!(f, "|").unwrap();
    start + 21
}

fn write_phase_pickup_call(
    f: &mut std::fmt::Formatter<'_>,
    key: &IStateKey,
    start: usize,
) -> usize {
    for i in start..start + 4 {
        if i == key.len() {
            return i;
        }

        let a = EAction::from(key[i]);
        write!(f, "{}", a).unwrap();
        if a == EAction::Pickup {
            // handle discard
            write!(f, "|").unwrap();
            write!(f, "{}", EAction::from(key[i + 1])).unwrap();
            return i + 2;
        }
    }

    write!(f, "|").unwrap();
    for i in start + 4..start + 8 {
        if i == key.len() {
            return i;
        }

        let a = EAction::from(key[i]);
        write!(f, "{}", a).unwrap();
        if a == EAction::Clubs
            || a == EAction::Diamonds
            || a == EAction::Spades
            || a == EAction::Hearts
        {
            return i + 1;
        }
    }

    panic!("invalid pickup and call phase")
}

fn write_phase_play(f: &mut std::fmt::Formatter<'_>, key: &IStateKey, start: usize) -> usize {
    for i in start..key.len() {
        if i != start && (i - start) % 4 == 0 {
            write!(f, "|").unwrap()
        }
        write!(f, "{}", EAction::from(key[i])).unwrap();
    }

    key.len()
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
            EPhase::DealHands | EPhase::DealFaceUp => self.legal_actions_dealing(actions),
            EPhase::Pickup => {
                actions.append(&mut vec![EAction::Pass.into(), EAction::Pickup.into()])
            }
            EPhase::Discard => {
                for c in &self.hands[3] {
                    actions.push(*c)
                }
            } // Dealer can discard any card
            EPhase::ChooseTrump => self.legal_actions_choose_trump(actions),
            EPhase::Play => self.legal_actions_play(actions),
        };
    }

    fn evaluate(&self, p: Player) -> f64 {
        if !self.is_terminal() {
            panic!("evaluate called on non-terminal gamestate");
        }

        let mut won_tricks = [0; 2];
        for i in 0..self.trick_winners.len() {
            let winner = self.trick_winners[i] % 2;
            won_tricks[winner] += 1;
        }

        let team = p % 2;

        // no points, didn't win most tricks
        if won_tricks[team] < won_tricks[(team + 1) % 2] {
            0.0
        } else if won_tricks[team] == 5 {
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
        for (i, s) in self.parser.history.iter().enumerate().take(self.key.len()) {
            if s.is_visible(player) {
                istate.push(self.key[i])
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
        if let Some(first_play) = self.first_played {
            return self.key.len() - first_play == 20;
        }
        false
    }

    fn is_chance_node(&self) -> bool {
        self.key.len() < self.num_players * CARDS_PER_HAND + 1 // deals + face up card
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
            let end_sort = sorted_key.len().min((p + 1) * CARDS_PER_HAND);
            sorted_key.sort_range(p * CARDS_PER_HAND, end_sort);
            if (p + 1) * CARDS_PER_HAND + 1 > sorted_key.len() {
                break;
            }
        }

        sorted_key
    }

    fn undo(&mut self) {
        self.cur_player = self.play_order.pop().unwrap();
        self.parser.undo();
        let applied_action = EAction::from(self.key.pop());

        // We check the current state the game is expecting, that's what the last actions
        // was
        match (self.parser[self.parser.len() - 1], applied_action) {
            (parser::EuchreParserState::DealPlayers(_), _) => {
                self.hands[self.cur_player].remove(&applied_action.into());
            }
            (parser::EuchreParserState::DealFaceUp, _) => {}
            (parser::EuchreParserState::Discard, _) => {
                let face_up = self.face_up();
                self.deck[face_up] = CardLocation::FaceUp; // card is face up again
                self.deck[applied_action.card()] = CardLocation::Player3;
            }
            (parser::EuchreParserState::PickupChoice(_), EAction::Pickup) => {
                self.trump = Suit::Clubs; // default value
                self.trump_caller = 0;
            }
            (parser::EuchreParserState::PickupChoice(_), EAction::Pass) => {}
            (parser::EuchreParserState::PickupChoice(_), _) => {
                self.trump = Suit::Clubs; // default value
                self.trump_caller = 0;
            }
            (parser::EuchreParserState::CallChoice(_), _) => {
                todo!()
            }
            (parser::EuchreParserState::Play(0), _) => {
                self.first_played = None;
                self.hands[self.cur_player].insert(applied_action.into());
            }
            (parser::EuchreParserState::Play(x), _) if (x + 1) % 4 == 0 => {
                // end of trick
                self.trick_winners[x / 5] = 0;
                self.hands[self.cur_player].insert(applied_action.into());
            }
            (parser::EuchreParserState::Play(_), _) => {
                self.hands[self.cur_player].insert(applied_action.into());
            }
            (parser::EuchreParserState::Terminal, _) => todo!(),
        };
    }
}

/// Resample from info state method for euchre
///
/// This method discards a RANDOM card for the dealer if a discard is required.
/// It's not yet clear what impact this has on the results of downstream algorithms
impl ResampleFromInfoState for EuchreGameState {
    fn resample_from_istate<T: rand::Rng>(&self, player: Player, rng: &mut T) -> Self {
        let mut player_chance = [Action(99); 5];

        for (i, c) in player_chance.iter_mut().enumerate() {
            *c = self.key[player * CARDS_PER_HAND + i];
        }

        if self.phase() == EPhase::Play {
            todo!("implement resampling for play phases")
        }

        let mut ngs = Euchre::new_state();
        let mut chance_iter = player_chance.iter();
        let face_up_chance = self.key[20]; // 21st card dealt

        for i in 0..self.key.len() {
            if i >= player * CARDS_PER_HAND && i < player * CARDS_PER_HAND + CARDS_PER_HAND {
                // the player chance node
                ngs.apply_action(*chance_iter.next().unwrap());
            } else if i == 20 {
                // the faceup chance node
                ngs.apply_action(face_up_chance);
            } else if ngs.is_chance_node() {
                assert!(ngs.cur_player() != player);

                // other player chance node
                let mut actions = actions!(ngs);
                actions.shuffle(rng);
                for a in actions {
                    if !player_chance.contains(&a) && a != face_up_chance {
                        // can't deal same card
                        ngs.apply_action(a);
                        break;
                    }
                }
            } else if self.discard.is_some() && i == self.discard.unwrap() {
                // handle the discard case, need a strategy for how dealer will discard here
                // and what the proper way for determing the discard is
                // If there is discard, need to have a card get dealt to the dealer that isn't played
                todo!("need to properly implement discard handling")
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
