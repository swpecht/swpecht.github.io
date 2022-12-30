use std::fmt::Display;

use itertools::Itertools;

use crate::game::{self, Action, GameState, Player};

const JACK: usize = 2;
const CARD_PER_SUIT: usize = 6;

pub struct Euchre {}
impl Euchre {
    pub fn new() -> EuchreGameState {
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
            play_history: Vec::new(),
            deck: deck,
            trump_caller: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EuchreGameState {
    num_players: usize,
    /// Holds the cards for each player in the game
    hands: Vec<Vec<Action>>,
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
                self.cur_player = self.cur_player + 1 % self.num_players
            }
            x if x == EAction::Pickup as usize => {
                self.trump_caller = self.cur_player;
                self.cur_player = 4; // dealers turn
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

        self.cur_player = 0;
        self.phase = EPhase::Play;
    }

    fn apply_action_play(&mut self, a: Action) {
        self.play_history.push(a);
        self.cur_player = self.cur_player + 1 % self.num_players;
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
    fn evaluate_trick(&self, cards: &[Action]) -> usize {
        assert_eq!(cards.len(), 4); // only support 4 players

        let mut winner = 0;
        let mut winning_card = cards[0];
        let mut winning_suit = self.get_suit(cards[0]);
        for i in 1..4 {
            let suit = self.get_suit(cards[i]);
            // Player can't win if not following suit or playing trump
            // The winning suit can only ever be trump or the lead suit
            if suit != winning_suit || suit != self.trump {
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

        return winner;
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
        todo!()
    }
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
        for i in 0..5 {
            let trick = &self.play_history[i * 4..i + 4];
            let winner = self.evaluate_trick(trick);
            won_tricks[winner % 2] += 1;
        }

        let team_0_win = won_tricks[0] > won_tricks[1];
        let team_0_call = self.trump_caller % 2 == 0;

        match (team_0_win, team_0_call) {
            (true, true) => vec![1.0, 0.0, 1.0, 0.0],
            (true, false) => vec![2.0, 0.0, 2.0, 0.0],
            (false, false) => vec![0.0, 1.0, 0.0, 1.0],
            (false, true) => vec![0.0, 2.0, 0.0, 2.0],
        }
    }

    fn information_state(&self, player: Player) -> Vec<game::IState> {
        todo!()
    }

    fn information_state_string(&self, player: Player) -> String {
        todo!()
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
    use std::vec;

    use crate::euchre::{EAction, EPhase, Euchre, Suit};

    use super::GameState;

    #[test]
    fn euchre_test_phases_choose_trump() {
        let mut s = Euchre::new();

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
        s.apply_action(EAction::Pass as usize);
        s.apply_action(EAction::Diamonds as usize);

        assert_eq!(s.phase, EPhase::Play);
    }

    #[test]
    fn euchre_test_phases_pickup() {
        let mut s = Euchre::new();

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
        let mut s = Euchre::new();

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
    fn test_suit() {
        let mut s = Euchre::new();

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
}
