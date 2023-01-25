use std::{fmt::Display, rc::Rc};

use itertools::Itertools;

use crate::game::{self, Action, Game, GameState, IState, Player};

const JACK: usize = 2;
const CARD_PER_SUIT: usize = 6;

pub struct Euchre {}
impl Euchre {
    pub fn new_state() -> EuchreGameState {
        let mut deck = Vec::new();
        for i in 0..24 {
            deck.push(i);
        }

        EuchreGameState {
            num_players: 4,
            hands: Vec::new(),
            is_chance_node: true,
            is_terminal: false,
            phase: EPhase::DealHands,
            cur_player: 0,
            trump: Suit::Clubs, // Default to one for now
            face_up: 0,         // Default for now
            play_history: Vec::with_capacity(20),
            deck: deck,
            trump_caller: 0,
            trump_call_history: [EAction::Pass as usize; 8],
            starting_hands: Rc::new(Vec::new()),
        }
    }

    pub fn game() -> Game {
        Game {
            new: Box::new(|| -> Box<dyn GameState> { Box::new(Self::new_state()) }),
            max_players: 2,
            max_actions: 24, // 1 for each card dealt
        }
    }
}

/// We use Rc for the starting hand information since these values rarely change
/// and are consistent across all children of the given state
#[derive(Debug)]
pub struct EuchreGameState {
    num_players: usize,
    /// Holds the cards for each player in the game
    hands: Vec<Vec<Action>>,
    starting_hands: Rc<Vec<Vec<Action>>>,
    /// Undealt cards
    deck: Vec<Action>,
    trump: Suit,
    trump_caller: usize,
    face_up: Action,
    is_chance_node: bool,
    is_terminal: bool,
    phase: EPhase,
    cur_player: usize,
    play_history: Vec<Action>,
    trump_call_history: [Action; 8],
}

impl Clone for EuchreGameState {
    fn clone(&self) -> Self {
        if self.phase == EPhase::Play {
            // if we're in the playing phase, can avoid copying the starting hand memory and
            // instead just keep a single reference. Doing this led to a ~15% improvement on the euchre
            // game tree traversal benchmark
            Self {
                num_players: self.num_players.clone(),
                hands: self.hands.clone(),
                starting_hands: Rc::clone(&self.starting_hands),
                deck: self.deck.clone(),
                trump: self.trump.clone(),
                trump_caller: self.trump_caller.clone(),
                face_up: self.face_up.clone(),
                is_chance_node: self.is_chance_node.clone(),
                is_terminal: self.is_terminal.clone(),
                phase: self.phase.clone(),
                cur_player: self.cur_player.clone(),
                play_history: self.play_history.clone(),
                trump_call_history: self.trump_call_history.clone(),
            }
        } else {
            Self {
                num_players: self.num_players.clone(),
                hands: self.hands.clone(),
                starting_hands: Rc::new((*self.starting_hands).clone()),
                deck: self.deck.clone(),
                trump: self.trump.clone(),
                trump_caller: self.trump_caller.clone(),
                face_up: self.face_up.clone(),
                is_chance_node: self.is_chance_node.clone(),
                is_terminal: self.is_terminal.clone(),
                phase: self.phase.clone(),
                cur_player: self.cur_player.clone(),
                play_history: self.play_history.clone(),
                trump_call_history: self.trump_call_history.clone(),
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EPhase {
    DealHands,
    DealFaceUp,
    Pickup,
    /// The dealer has been told to pickup the trump suit
    Discard,
    ChooseTrump,
    Play,
}

enum EAction {
    Pickup,
    Pass,
    Clubs,
    Spades,
    Hearts,
    Diamonds,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Suit {
    Clubs,
    Spades,
    Hearts,
    Diamonds,
}

impl EuchreGameState {
    fn apply_action_deal_hands(&mut self, a: Action) {
        let index = self.deck.iter().position(|&x| x == a).unwrap();
        self.deck.remove(index);

        if self.hands.len() <= self.cur_player {
            self.hands.push(Vec::new());
        }

        self.hands[self.cur_player].push(a);
        self.hands[self.cur_player].sort();

        self.starting_hands = Rc::new(self.hands.clone());

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
        match a {
            x if x == EAction::Pass as usize => {
                if self.cur_player == self.num_players - 1 {
                    // Dealer has passed, move to choosing
                    self.phase = EPhase::ChooseTrump;
                }
                self.cur_player = (self.cur_player + 1) % self.num_players;
            }
            x if x == EAction::Pickup as usize => {
                self.trump_caller = self.cur_player;
                self.trump_call_history[self.cur_player] = a;
                self.trump = self.get_suit(self.face_up);
                self.cur_player = 3; // dealers turn
                self.phase = EPhase::Discard;
            }
            _ => panic!("invalid action"),
        }
    }

    fn apply_action_choose_trump(&mut self, a: Action) {
        match a {
            x if x == EAction::Clubs as usize => self.trump = Suit::Clubs,
            x if x == EAction::Spades as usize => self.trump = Suit::Spades,
            x if x == EAction::Hearts as usize => self.trump = Suit::Hearts,
            x if x == EAction::Diamonds as usize => self.trump = Suit::Diamonds,
            x if x == EAction::Pass as usize => {}
            _ => panic!("invalid action"),
        };

        if a == EAction::Pass as usize {
            self.cur_player += 1;
        } else {
            self.trump_caller = self.cur_player;
            self.trump_call_history[4 + self.cur_player] = a; // in the second round of calling trump
            self.cur_player = 0;
            self.phase = EPhase::Play;
        }
    }

    /// Can only be done by the dealer (player 3)
    fn apply_action_discard(&mut self, a: Action) {
        let index = self.hands[3].iter().position(|&x| x == a);
        if let Some(index) = index {
            self.hands[3][index] = self.face_up;
        } else {
            panic!("attempted to discard a card not in hand")
        }

        self.starting_hands = Rc::new(self.hands.clone());
        self.cur_player = 0;
        self.phase = EPhase::Play;
    }

    fn apply_action_play(&mut self, a: Action) {
        self.play_history.push(a);
        let index = self.hands[self.cur_player]
            .iter()
            .position(|&x| x == a)
            .unwrap();
        self.hands[self.cur_player].remove(index);

        // Set acting player based on who won last trick
        if self.play_history.len() % 4 == 0 {
            let starter = (self.cur_player + 3) % self.num_players;
            let trick = &self.play_history[self.play_history.len() - 4..];
            let winner = self.evaluate_trick(trick, starter);
            self.cur_player = winner;
        }

        self.cur_player = (self.cur_player + 1) % self.num_players;
        if self.play_history.len() >= self.num_players * 5 {
            self.is_terminal = true;
        }
    }

    fn legal_actions_dealing(&self) -> Vec<Action> {
        return self.deck.clone();
    }

    /// Can choose any trump except for the one from the faceup card
    /// For the dealer they aren't able to pass.
    fn legal_actions_choose_trump(&self) -> Vec<Action> {
        let mut actions = Vec::new();

        // Dealer can't pass
        if self.cur_player != 3 {
            actions.push(EAction::Pass as usize)
        }

        let face_up = self.get_suit(self.face_up);
        if face_up != Suit::Clubs {
            actions.push(EAction::Clubs as usize);
        }
        if face_up != Suit::Spades {
            actions.push(EAction::Spades as usize);
        }
        if face_up != Suit::Hearts {
            actions.push(EAction::Hearts as usize);
        }
        if face_up != Suit::Diamonds {
            actions.push(EAction::Diamonds as usize);
        }
        return actions;
    }

    /// Needs to consider following suit if possible
    /// Can only play cards from hand
    fn legal_actions_play(&self) -> Vec<Action> {
        // If they are the first to act on a trick then can play any card in hand
        if self.play_history.len() % self.num_players == 0 {
            return self.hands[self.cur_player].clone();
        }

        let leading_card = self.play_history[self.play_history.len() / 4 * 4];
        let suit = self.get_suit(leading_card);

        let actions = self.hands[self.cur_player]
            .iter()
            .filter(|&&x| self.get_suit(x) == suit)
            .map(|x| *x)
            .collect_vec();

        if actions.len() == 0 {
            // no suit, can play any card
            return self.hands[self.cur_player].clone();
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
                self.play_history
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
            EPhase::Pickup => vec![EAction::Pass as usize, EAction::Pickup as usize],
            EPhase::Discard => self.hands[3].clone(), // Dealer can discard any card
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
            let trick = &self.play_history[i * 4..4 * i + 4];
            let winner = self.evaluate_trick(trick, starter);
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
    fn information_state(&self, player: Player) -> Vec<game::IState> {
        let mut istate = Vec::with_capacity(27);
        for &c in &self.starting_hands[player] {
            istate.push(c as IState);
        }

        if self.starting_hands[player].len() < 5 || self.phase == EPhase::DealFaceUp {
            return istate;
        }

        // Add the face up card
        istate.push(self.face_up as IState);

        if self.phase != EPhase::DealHands && self.phase != EPhase::DealFaceUp {
            istate.extend(self.trump_call_history[0..4].iter().map(|&x| x as IState));
        }

        if self.phase != EPhase::DealHands
            && self.phase != EPhase::DealFaceUp
            && self.phase != EPhase::Pickup
        {
            istate.extend(self.trump_call_history[4..8].iter().map(|&x| x as IState));
        }

        if self.phase == EPhase::Discard || self.phase == EPhase::Play {
            istate.push(self.trump_caller as IState);
            istate.push(self.trump.clone() as usize as IState);
        }

        for c in &self.play_history {
            istate.push(*c as IState);
        }

        return istate;
    }

    fn information_state_string(&self, player: Player) -> String {
        let istate = self.information_state(player);
        // Full game state:
        // 9CTCJCKCKS|KH|PPPPPPCP|3H|ASTSKSAC|9C9HTDQC|JD9DTCJH|JSKCQHQD|KDADXXXX|
        let mut r = String::with_capacity(71);

        for i in 0..5 {
            r.push_str("XX");
            let len = r.len();
            let s = r[len - 2..].as_mut();
            // let card = format_card(istate[i] as Action);
            // r.push_str(&card)
            put_card(istate[i] as Action, s);
        }

        if istate.len() <= 5 {
            return r;
        }

        // Face up card
        r.push('|');
        r.push_str("XX");
        let len = r.len();
        let s = r[len - 2..].as_mut();
        put_card(istate[5] as Action, s);
        r.push('|');

        if istate.len() <= 6 {
            return r;
        }

        // Pickup round
        for i in 0..4 {
            let c = match self.trump_call_history[i] {
                x if x == EAction::Clubs as usize => 'C',
                x if x == EAction::Spades as usize => 'S',
                x if x == EAction::Hearts as usize => 'H',
                x if x == EAction::Diamonds as usize => 'D',
                x if x == EAction::Pass as usize => 'P',
                x if x == EAction::Pickup as usize => 'T',
                _ => panic!("invalid action"),
            };
            r.push(c);
        }

        if istate.len() <= 10 {
            return r;
        }
        for i in 4..8 {
            let c = match self.trump_call_history[i] {
                x if x == EAction::Clubs as usize => 'C',
                x if x == EAction::Spades as usize => 'S',
                x if x == EAction::Hearts as usize => 'H',
                x if x == EAction::Diamonds as usize => 'D',
                x if x == EAction::Pass as usize => 'P',
                x if x == EAction::Pickup as usize => 'T',
                _ => panic!("invalid action"),
            };
            r.push(c);
        }

        r.push('|');

        if istate.len() <= 14 {
            return r;
        }

        // Calling player
        r.push_str(&format!("{}", istate[14] as usize));

        let trump_char = match istate[15] as usize {
            x if x == Suit::Clubs as usize => 'C',
            x if x == Suit::Spades as usize => 'S',
            x if x == Suit::Diamonds as usize => 'D',
            x if x == Suit::Hearts as usize => 'H',
            _ => panic!("invalid trump"),
        };

        r.push(trump_char);

        for i in 0..self.play_history.len() {
            if i % self.num_players == 0 {
                r.push('|');
            }
            r.push_str("XX");
            let len = r.len();
            let s = r[len - 2..].as_mut();
            put_card(self.play_history[i], s);
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
            s.apply_action(EAction::Pass as usize);
        }

        assert_eq!(s.phase, EPhase::ChooseTrump);
        assert_eq!(s.cur_player, 0);
        s.apply_action(EAction::Pass as usize);
        s.apply_action(EAction::Diamonds as usize);
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
            s.apply_action(EAction::Pass as usize);
        }
        s.apply_action(EAction::Pickup as usize);

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

        assert_eq!(
            s.legal_actions(),
            vec![EAction::Pass as usize, EAction::Pickup as usize]
        );

        s.apply_action(EAction::Pickup as usize);
        // Cards in dealers hand
        assert_eq!(s.legal_actions(), vec![3, 7, 11, 15, 19]);

        s.apply_action(3);

        // Cards player 0s hand
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
        s.apply_action(EAction::Pickup as usize);
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

        assert_eq!(s.information_state_string(0), "9CKCJS9HKH");
        assert_eq!(s.information_state_string(1), "TCACQSTHAH");
        assert_eq!(s.information_state_string(2), "JC9SKSJH9D");
        assert_eq!(s.information_state_string(3), "QCTSASQHTD");

        s.apply_action(20);
        assert_eq!(s.information_state_string(0), "9CKCJS9HKH|JD|PPPP");
        assert_eq!(s.information_state_string(1), "TCACQSTHAH|JD|PPPP");
        assert_eq!(s.information_state_string(2), "JC9SKSJH9D|JD|PPPP");
        assert_eq!(s.information_state_string(3), "QCTSASQHTD|JD|PPPP");

        s.apply_action(EAction::Pickup as usize);
        assert_eq!(s.information_state_string(0), "9CKCJS9HKH|JD|TPPPPPPP|0D");

        // Dealer discards the QC
        s.apply_action(3);
        assert_eq!(s.information_state_string(3), "JDTSASQHTD|JD|TPPPPPPP|0D");

        for _ in 0..4 {
            let a = s.legal_actions()[0];
            s.apply_action(a);
        }
        assert_eq!(
            s.information_state_string(0),
            "9CKCJS9HKH|JD|TPPPPPPP|0D|9CTCJCJD"
        );

        while !s.is_terminal() {
            let a = s.legal_actions()[0];
            s.apply_action(a);
        }

        assert_eq!(s.evaluate(), vec![0.0, 2.0, 0.0, 2.0])
    }

    #[test]
    fn euchre_test_unique_istate() {
        let mut ra = RandomAgent::new();

        for _ in 0..10000 {
            let mut s = Euchre::new_state();
            let mut istates = HashSet::new();
            while s.is_chance_node() {
                let a = ra.step(&s);
                s.apply_action(a);
            }

            istates.insert(s.information_state_string(s.cur_player));
            while !s.is_terminal() {
                let a = ra.step(&s);
                s.apply_action(a);
                let istate = s.information_state_string(s.cur_player);
                assert!(!istates.contains(&istate));
                istates.insert(istate);
            }
        }
    }
}
