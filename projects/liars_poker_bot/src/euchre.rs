use std::fmt::{Display, Write};

use crate::{
    game::{arrayvec::SortedArrayVec, Action, Game, GameState, Player},
    istate::IStateKey,
};

const JACK: usize = 2;
const CARD_PER_SUIT: usize = 6;
const NUM_CARDS: usize = 24;

pub struct Euchre {}
impl Euchre {
    pub fn new_state() -> EuchreGameState {
        let keys = [IStateKey::new(); 4];
        let hands = [SortedArrayVec::<5>::new(); 4];

        EuchreGameState {
            num_players: 4,
            hands: hands,
            is_chance_node: true,
            is_terminal: false,
            phase: EPhase::DealHands,
            cur_player: 0,
            trump: Suit::Clubs, // Default to one for now
            face_up: 0,         // Default for now
            trump_caller: 0,
            istate_keys: keys,
            first_played: None,
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
#[derive(Debug, Clone, Copy)]
pub struct EuchreGameState {
    num_players: usize,
    /// Holds the cards for each player in the game
    hands: [SortedArrayVec<5>; 4],
    trump: Suit,
    trump_caller: usize,
    face_up: Action,
    is_chance_node: bool,
    is_terminal: bool,
    phase: EPhase,
    cur_player: usize,
    istate_keys: [IStateKey; 4],
    /// the index of the 0 player istate where the first played card is
    /// used to make looking up tricks easier
    first_played: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
enum EPhase {
    DealHands,
    DealFaceUp,
    Pickup,
    /// The dealer has been told to pickup the trump suit
    Discard,
    ChooseTrump,
    Play,
}

#[derive(PartialEq)]
enum EAction {
    Pickup,
    Pass,
    Clubs,
    Spades,
    Hearts,
    Diamonds,
    Card { a: usize },
}

impl EAction {
    // Converts a non card action to the enum
    fn non_card_from(value: usize) -> Self {
        match value {
            0 => EAction::Pickup,
            1 => EAction::Pass,
            2 => EAction::Clubs,
            3 => EAction::Spades,
            4 => EAction::Hearts,
            5 => EAction::Diamonds,
            _ => panic!("unsupported action"),
        }
    }
}

impl Into<usize> for EAction {
    fn into(self) -> usize {
        match self {
            EAction::Pickup => 0,
            EAction::Pass => 1,
            EAction::Clubs => 2,
            EAction::Spades => 3,
            EAction::Hearts => 4,
            EAction::Diamonds => 5,
            EAction::Card { a: x } => x,
        }
    }
}

impl Display for EAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EAction::Clubs => f.write_char('C'),
            EAction::Spades => f.write_char('S'),
            EAction::Hearts => f.write_char('H'),
            EAction::Diamonds => f.write_char('D'),
            EAction::Pickup => f.write_char('T'),
            EAction::Pass => f.write_char('P'),
            EAction::Card { a: c } => f.write_str(&format_card(*c)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
enum Suit {
    Clubs,
    Spades,
    Hearts,
    Diamonds,
}

impl Display for Suit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let c = match self {
            Suit::Clubs => 'C',
            Suit::Spades => 'S',
            Suit::Hearts => 'H',
            Suit::Diamonds => 'D',
        };

        f.write_char(c)
    }
}

impl EuchreGameState {
    fn apply_action_deal_hands(&mut self, a: Action) {
        self.hands[self.cur_player].push(a);

        self.cur_player = (self.cur_player + 1) % self.num_players;

        if self.hands.len() == self.num_players && self.hands[self.num_players - 1].len() == 5 {
            self.phase = EPhase::DealFaceUp;
        }
    }

    fn apply_action_deal_face_up(&mut self, a: Action) {
        self.face_up = a;
        self.phase = EPhase::Pickup;
        self.cur_player = 0;
        self.is_chance_node = false;
    }

    fn apply_action_pickup(&mut self, a: Action) {
        match EAction::non_card_from(a) {
            EAction::Pass => {
                if self.cur_player == self.num_players - 1 {
                    // Dealer has passed, move to choosing
                    self.phase = EPhase::ChooseTrump;
                }
                self.cur_player = (self.cur_player + 1) % self.num_players;
            }
            EAction::Pickup => {
                self.trump_caller = self.cur_player;
                self.trump = self.get_suit(self.face_up);
                self.cur_player = 3; // dealers turn
                self.phase = EPhase::Discard;
            }
            _ => panic!("invalid action"),
        }
    }

    fn apply_action_choose_trump(&mut self, a: Action) {
        let a = EAction::non_card_from(a);
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
            self.phase = EPhase::Play;
        }
    }

    /// Can only be done by the dealer (player 3)
    fn apply_action_discard(&mut self, a: Action) {
        if !self.hands[3].contains(&a) {
            panic!("attempted to discard a card not in hand")
        }
        self.hands[3].remove(a);
        self.hands[3].push(self.face_up);

        self.cur_player = 0;
        self.phase = EPhase::Play;
    }

    fn apply_action_play(&mut self, a: Action) {
        if self.first_played.is_none() {
            self.first_played = Some(self.istate_keys[0].len() - 1);
        }

        for i in 0..self.hands[self.cur_player].len() {
            if self.hands[self.cur_player][i] == a {
                self.hands[self.cur_player].remove(a);
                break;
            }
        }

        // Set acting player based on who won last trick
        let trick_over = self.is_trick_over();
        let num_cards = self.hands[0].len();
        // trick is over and played at least one card
        if trick_over && num_cards < 5 {
            let trick = self.get_trick(0);
            let starter = (self.cur_player + 3) % self.num_players;
            let winner = self.evaluate_trick(&trick, starter);
            self.cur_player = winner;
        } else {
            self.cur_player = (self.cur_player + 1) % self.num_players;
        }

        if trick_over && num_cards == 0 {
            self.is_terminal = true;
        }
    }

    /// Determine if current trick is over (all 4 players have played)
    /// Also returns true if none have played
    fn is_trick_over(&self) -> bool {
        // if no one has played yet
        if self.first_played.is_none() {
            return true;
        }

        return (self.istate_keys[0].len() - self.first_played.unwrap()) % 4 == 0;
    }

    /// Gets the `n` last trick
    ///
    /// 0 is the most recent trick
    fn get_trick(&self, n: usize) -> [Action; 4] {
        if !self.is_trick_over() {
            panic!("cannot get trick unless the trick is over");
        }

        // can check any since trick is over, all same value
        let played_cards = 5 - self.hands[0].len();
        if n + 1 > played_cards {
            panic!("not that many tricks have been played");
        }

        // all keys should see the same tricks, so just use the first one
        let key = self.istate_keys[0];
        let sidx = key.len() - 4 * (n + 1);
        let mut trick = [0; 4];
        for i in 0..4 {
            trick[i] = key[sidx + i];
        }

        return trick;
    }

    /// Get the card that started the current trick
    fn get_leading_card(&self) -> Action {
        if self.phase != EPhase::Play {
            panic!("tried to get leading card of trick at invalid time")
        }

        let min_hand = self.hands.iter().map(|x| x.len()).min().unwrap();
        let cards_played = self.hands.iter().filter(|&x| x.len() == min_hand).count();

        let key = self.istate_keys[0];
        let first_card = key[key.len() - cards_played];

        return first_card;
    }

    fn legal_actions_dealing(&self) -> Vec<Action> {
        let mut deck = Vec::with_capacity(NUM_CARDS);
        for i in 0..NUM_CARDS {
            let mut is_dealt = false;
            for h in 0..self.num_players {
                if self.hands[h].contains(&i) {
                    is_dealt = true;
                    break;
                }
            }
            if !is_dealt {
                deck.push(i);
            }
        }
        return deck;
    }

    /// Can choose any trump except for the one from the faceup card
    /// For the dealer they aren't able to pass.
    fn legal_actions_choose_trump(&self) -> Vec<Action> {
        let mut actions = Vec::new();

        // Dealer can't pass
        if self.cur_player != 3 {
            actions.push(EAction::Pass.into())
        }

        let face_up = self.get_suit(self.face_up);
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
        return actions;
    }

    /// Needs to consider following suit if possible
    /// Can only play cards from hand
    fn legal_actions_play(&self) -> Vec<Action> {
        // If they are the first to act on a trick then can play any card in hand
        if self.is_trick_over() {
            return self.hands[self.cur_player].to_vec();
        }

        let leading_card = self.get_leading_card();
        let suit = self.get_suit(leading_card);

        let mut actions = Vec::with_capacity(5);
        for i in 0..self.hands[self.cur_player].len() {
            let c = self.hands[self.cur_player][i];
            if self.get_suit(c) == suit {
                actions.push(c);
            }
        }

        if actions.len() == 0 {
            // no suit, can play any card
            return self.hands[self.cur_player].to_vec();
        } else {
            return actions;
        }
    }

    /// Returns the player who won the trick
    fn evaluate_trick(&self, cards: &[Action], trick_starter: Player) -> usize {
        assert_eq!(cards.len(), 4); // only support 4 players

        let mut winner = 0;
        let mut winning_card = cards[0];
        let mut winning_suit = self.get_suit(cards[0]);
        for i in 1..4 {
            let suit = self.get_suit(cards[i]);
            // Player can't win if not following suit or playing trump
            // The winning suit can only ever be trump or the lead suit
            if suit != winning_suit && suit != self.trump {
                continue;
            }

            // Simple case where we don't need to worry about weird trump scoring
            if suit == winning_suit && suit != self.trump && cards[i] > winning_card {
                winner = i;
                winning_card = cards[i];
                winning_suit = self.get_suit(cards[i]);
                continue;
            }

            // Play trump over lead suit
            if suit == self.trump && winning_suit != self.trump {
                winner = i;
                winning_card = cards[i];
                winning_suit = self.get_suit(cards[i]);
                continue;
            }

            // Handle trump scoring. Need to differentiate the left and right
            if suit == self.trump && winning_suit == self.trump {
                let winning_card_value = self.get_card_value(winning_card);
                let cur_card_value = self.get_card_value(cards[i]);
                if cur_card_value > winning_card_value {
                    winner = i;
                    winning_card = cards[i];
                    winning_suit = self.get_suit(cards[i]);
                    continue;
                }
            }
        }

        return (trick_starter + winner) % self.num_players;
    }

    /// Gets the suit of a given card. Accounts for the weird scoring of the trump suit
    /// if in the playing phase of the game
    fn get_suit(&self, c: Action) -> Suit {
        let mut suit = match c / CARD_PER_SUIT {
            x if x == Suit::Clubs as usize => Suit::Clubs,
            x if x == Suit::Hearts as usize => Suit::Hearts,
            x if x == Suit::Spades as usize => Suit::Spades,
            x if x == Suit::Diamonds as usize => Suit::Diamonds,
            _ => panic!("invalid card"),
        };

        // Correct the jack if in play phase
        if self.phase == EPhase::Play && c % CARD_PER_SUIT == JACK {
            suit = match (suit.clone(), self.trump.clone()) {
                (Suit::Clubs, Suit::Spades) => Suit::Spades,
                (Suit::Spades, Suit::Clubs) => Suit::Clubs,
                (Suit::Hearts, Suit::Diamonds) => Suit::Diamonds,
                (Suit::Diamonds, Suit::Hearts) => Suit::Hearts,
                _ => suit,
            }
        }
        return suit;
    }

    /// Returns a relative value for cards. The absolute values are meaningyless
    /// but can be used to compare card values of the same suit. It accounts for
    /// left and right jack.
    fn get_card_value(&self, c: Action) -> usize {
        if self.get_suit(c) != self.trump || self.phase != EPhase::Play || c % CARD_PER_SUIT != JACK
        {
            return c % CARD_PER_SUIT;
        }

        // Get the suit "on the card" determine if left or right
        let pure_suit = match c / CARD_PER_SUIT {
            x if x == Suit::Clubs as usize => Suit::Clubs,
            x if x == Suit::Hearts as usize => Suit::Hearts,
            x if x == Suit::Spades as usize => Suit::Spades,
            x if x == Suit::Diamonds as usize => Suit::Diamonds,
            _ => panic!("invalid card"),
        };
        let is_right = pure_suit == self.trump;

        return match is_right {
            true => CARD_PER_SUIT + 2,  // right
            false => CARD_PER_SUIT + 1, // left
        };
    }

    fn update_keys(&mut self, a: Action) {
        // haven't pushed the cards yet, do it now if we've dealt all but the last card
        if (self.hands[3].len() == 4) && (self.hands[2].len() == 5) {
            for p in 0..self.num_players {
                for i in 0..self.hands[p].len() {
                    let c = self.hands[p][i];
                    self.istate_keys[p].push(c);
                }
            }

            self.istate_keys[3].push(a);
        }

        if self.phase == EPhase::DealHands {
            // don't do anything until hands are dealt so we can put the cards in order
            return;
        }

        // Private actions
        if self.phase == EPhase::DealHands || self.phase == EPhase::Discard {
            self.istate_keys[self.cur_player].push(a);
            return;
        }

        for i in 0..self.num_players {
            self.istate_keys[i].push(a);
        }
    }
}

impl Display for EuchreGameState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.phase {
            EPhase::DealHands | EPhase::DealFaceUp => {
                write!(f, "{:?}: {:?}", self.phase, self.hands,)
            }
            _ => write!(
                f,
                "{:?}: {:?} {:?} {:?}",
                self.phase,
                self.hands,
                format_card(self.face_up),
                self.istate_string(0)
            ),
        }
    }
}

/// Populates a string buffer with formated card. Must be 2 characters long
fn format_card(c: Action) -> String {
    let mut out = "XX".to_string();
    put_card(c, &mut out);
    return out.to_string();
}

fn put_card(c: Action, out: &mut str) {
    assert_eq!(out.len(), 2);

    let suit_char = match c / CARD_PER_SUIT {
        x if x == Suit::Clubs as usize => 'C',
        x if x == Suit::Hearts as usize => 'H',
        x if x == Suit::Spades as usize => 'S',
        x if x == Suit::Diamonds as usize => 'D',
        _ => panic!("invalid card"),
    };

    let num_char = match c % CARD_PER_SUIT {
        0 => '9',
        1 => 'T',
        2 => 'J',
        3 => 'Q',
        4 => 'K',
        5 => 'A',
        _ => panic!("invalid card"),
    };

    let s_bytes: &mut [u8] = unsafe { out.as_bytes_mut() };
    assert_eq!(s_bytes.len(), 2);
    // we've made sure this is safe.
    s_bytes[0] = num_char as u8;
    s_bytes[1] = suit_char as u8;
}

impl GameState for EuchreGameState {
    fn apply_action(&mut self, a: Action) {
        self.update_keys(a);

        match self.phase {
            EPhase::DealHands => self.apply_action_deal_hands(a),
            EPhase::DealFaceUp => self.apply_action_deal_face_up(a),
            EPhase::Pickup => self.apply_action_pickup(a),
            EPhase::ChooseTrump => self.apply_action_choose_trump(a),
            EPhase::Discard => self.apply_action_discard(a),
            EPhase::Play => self.apply_action_play(a),
        }
    }

    fn legal_actions(&self) -> Vec<Action> {
        match self.phase {
            EPhase::DealHands | EPhase::DealFaceUp => self.legal_actions_dealing(),
            EPhase::Pickup => vec![EAction::Pass.into(), EAction::Pickup.into()],
            EPhase::Discard => self.hands[3].to_vec(), // Dealer can discard any card
            EPhase::ChooseTrump => self.legal_actions_choose_trump(),
            EPhase::Play => self.legal_actions_play(),
        }
    }

    fn evaluate(&self) -> Vec<f32> {
        if !self.is_terminal {
            return vec![0.0; self.num_players];
        }

        let mut won_tricks = [0; 2];
        let mut starter = 0;
        for i in 0..5 {
            let trick = self.get_trick(i);
            let winner = self.evaluate_trick(&trick, starter);
            starter = winner;
            won_tricks[winner % 2] += 1;
        }

        let team_0_win = won_tricks[0] > won_tricks[1];
        let team_0_call = self.trump_caller % 2 == 0;

        match (team_0_win, team_0_call, won_tricks[0]) {
            (true, true, 5) => vec![2.0, 0.0, 2.0, 0.0],
            (true, true, _) => vec![1.0, 0.0, 1.0, 0.0],
            (true, false, _) => vec![2.0, 0.0, 2.0, 0.0],
            (false, false, 0) => vec![0.0, 2.0, 0.0, 2.0],
            (false, false, _) => vec![0.0, 1.0, 0.0, 1.0],
            (false, true, _) => vec![0.0, 2.0, 0.0, 2.0],
        }
    }

    /// Returns an information state with the following format:
    /// * 0-4: hand
    /// * 5: face up card
    /// * 6-13: calling and passing for trump call
    /// * 14: Calling player
    /// * 15: trump
    /// * 16+: play history
    fn istate_key(&self, player: Player) -> IStateKey {
        return self.istate_keys[player];
    }

    fn istate_string(&self, player: Player) -> String {
        let istate = self.istate_keys[player];

        // Full game state:
        // 9CTCJCKCKS|KH|PPPPPPCP|3H|ASTSKSAC|9C9HTDQC|JD9DTCJH|JSKCQHQD|KDADXXXX|
        let mut r = String::new();

        if self.phase == EPhase::DealHands {
            todo!("don't yet support istates during dealing phase");
        }

        for i in 0..5 {
            let a = istate[i];
            let s = EAction::Card { a }.to_string();
            r.push_str(&s);
        }

        if self.phase == EPhase::DealFaceUp {
            return r;
        }

        // Face up card
        let a = istate[5];

        r.push('|');
        let s = EAction::Card { a }.to_string();
        r.push_str(&s);
        r.push('|');

        // Pickup round and calling round
        let mut pickup_called = false;
        let mut num_pickups = 0;
        for i in 6..(istate.len()).min(6 + 4) {
            let a = istate[i];
            let a = EAction::non_card_from(a);
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

        if self.phase == EPhase::Pickup {
            return r;
        }

        let mut num_calls = 0;
        if !pickup_called {
            for i in 10..(istate.len()).min(6 + 4 + 4) {
                let a = istate[i];
                let a = EAction::non_card_from(a);
                let s = a.to_string();
                r.push_str(&s);
                num_calls += 1;

                if a != EAction::Pass {
                    break;
                }
            }

            if self.phase == EPhase::ChooseTrump {
                return r;
            }
        }

        r.push('|');

        r.push_str(&format!("{}{}", self.trump_caller, self.trump));

        if self.phase == EPhase::Discard {
            return r;
        }

        // If the dealer, show the discarded card if that happened
        if player == 3 && pickup_called {
            r.push('|');
            let a = istate[6 + num_pickups];

            let d = EAction::Card { a: a }.to_string();
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
            let c = EAction::Card { a: a }.to_string();

            r.push_str(&c);
            turn += 1;
            i += 1;
        }

        return r;
    }

    fn is_terminal(&self) -> bool {
        self.is_terminal
    }

    fn is_chance_node(&self) -> bool {
        self.is_chance_node
    }

    fn num_players(&self) -> usize {
        self.num_players
    }

    fn cur_player(&self) -> Player {
        self.cur_player
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, vec};

    use crate::{
        agents::{Agent, RandomAgent},
        euchre::{EAction, EPhase, Euchre, Suit},
    };

    use super::GameState;

    #[test]
    fn euchre_test_phases_choose_trump() {
        let mut s = Euchre::new_state();

        assert_eq!(s.phase, EPhase::DealHands);
        for i in 0..20 {
            s.apply_action(i);
        }

        assert_eq!(s.phase, EPhase::DealFaceUp);
        s.apply_action(20);

        assert_eq!(s.phase, EPhase::Pickup);
        assert!(!s.is_chance_node);
        for i in 0..4 {
            assert_eq!(s.cur_player, i);
            s.apply_action(EAction::Pass.into());
        }

        assert_eq!(s.phase, EPhase::ChooseTrump);
        assert_eq!(s.cur_player, 0);
        s.apply_action(EAction::Pass.into());
        s.apply_action(EAction::Diamonds.into());
        assert_eq!(s.cur_player, 0);

        assert_eq!(s.phase, EPhase::Play);
    }

    #[test]
    fn euchre_test_phases_pickup() {
        let mut s = Euchre::new_state();

        assert_eq!(s.phase, EPhase::DealHands);
        for i in 0..20 {
            s.apply_action(i);
        }

        assert_eq!(s.phase, EPhase::DealFaceUp);
        s.apply_action(20);

        assert_eq!(s.phase, EPhase::Pickup);
        assert!(!s.is_chance_node);
        for _ in 0..3 {
            s.apply_action(EAction::Pass.into());
        }
        s.apply_action(EAction::Pickup.into());

        assert_eq!(s.phase, EPhase::Discard);
        s.apply_action(3);

        assert_eq!(s.phase, EPhase::Play);
        assert_eq!(s.cur_player, 0);
    }

    #[test]
    fn euchre_test_legal_actions() {
        let mut s = Euchre::new_state();

        for i in 0..20 {
            s.apply_action(i);
            let legal = s.legal_actions();
            for j in 0..i + 1 {
                let index = legal.iter().position(|&x| x == j);
                assert_eq!(index, None);
            }
        }

        // Deal the face up card
        s.apply_action(21);
        assert_eq!(s.face_up, 21);

        assert_eq!(
            s.legal_actions(),
            vec![Into::<usize>::into(EAction::Pass), EAction::Pickup.into()]
        );

        s.apply_action(EAction::Pickup.into());
        // Cards in dealers hand
        assert_eq!(s.legal_actions(), vec![3, 7, 11, 15, 19]);
        assert_eq!(s.phase, EPhase::Discard);
        s.apply_action(3);

        // Cards player 0s hand
        assert_eq!(s.phase, EPhase::Play);
        assert_eq!(s.legal_actions(), vec![0, 4, 8, 12, 16]);

        s.apply_action(0);
        // Player 1 must follow suit
        assert_eq!(s.legal_actions(), vec![1, 5]);
    }

    #[test]
    fn euchre_test_suit() {
        let mut s = Euchre::new_state();

        assert_eq!(s.get_suit(0), Suit::Clubs);
        // Jack of spades is still a spade
        assert_eq!(s.get_suit(8), Suit::Spades);
        assert_eq!(s.get_suit(7), Suit::Spades);

        // Deal the cards
        for i in 1..21 {
            s.apply_action(i);
        }

        s.apply_action(0); // Deal the 9 face up
        s.apply_action(EAction::Pickup.into());
        s.apply_action(4);
        assert_eq!(s.trump, Suit::Clubs);
        assert_eq!(s.phase, EPhase::Play);
        // Jack of spades is now a club since it's trump
        assert_eq!(s.get_suit(8), Suit::Clubs);
        assert_eq!(s.get_suit(7), Suit::Spades);
    }

    #[test]
    fn euchre_test_istate() {
        let mut s = Euchre::new_state();
        // Deal the cards
        for i in 0..20 {
            s.apply_action(i);
        }

        assert_eq!(s.istate_string(0), "9CKCJS9HKH");
        assert_eq!(s.istate_string(1), "TCACQSTHAH");
        assert_eq!(s.istate_string(2), "JC9SKSJH9D");
        assert_eq!(s.istate_string(3), "QCTSASQHTD");

        s.apply_action(20);
        assert_eq!(s.istate_string(0), "9CKCJS9HKH|JD|");
        assert_eq!(s.istate_string(1), "TCACQSTHAH|JD|");
        assert_eq!(s.istate_string(2), "JC9SKSJH9D|JD|");
        assert_eq!(s.istate_string(3), "QCTSASQHTD|JD|");

        let mut new_s = s.clone(); // for alternative pickup parsing

        s.apply_action(EAction::Pickup.into());
        assert_eq!(s.istate_string(0), "9CKCJS9HKH|JD|T|0D");

        // Dealer discards the QC
        assert_eq!(s.istate_string(3), "QCTSASQHTD|JD|T|0D");
        s.apply_action(3);
        assert_eq!(s.istate_string(3), "QCTSASQHTD|JD|T|0D|QC");

        for _ in 0..4 {
            let a = s.legal_actions()[0];
            s.apply_action(a);
        }
        assert_eq!(s.istate_string(0), "9CKCJS9HKH|JD|T|0D|9CTCJCTS");
        assert_eq!(s.istate_string(1), "TCACQSTHAH|JD|T|0D|9CTCJCTS");
        assert_eq!(s.istate_string(2), "JC9SKSJH9D|JD|T|0D|9CTCJCTS");
        assert_eq!(s.istate_string(3), "QCTSASQHTD|JD|T|0D|QC|9CTCJCTS");

        while !s.is_terminal() {
            let a = s.legal_actions()[0];
            s.apply_action(a);
            s.istate_string(0);
        }
        assert_eq!(s.evaluate(), vec![0.0, 2.0, 0.0, 2.0]);

        // Different calling path
        for _ in 0..5 {
            new_s.apply_action(EAction::Pass.into());
        }
        new_s.apply_action(EAction::Hearts.into());
        assert_eq!(new_s.istate_string(0), "9CKCJS9HKH|JD|PPPPPH|1H");
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
}
