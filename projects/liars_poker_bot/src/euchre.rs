use std::fmt::Display;

use crate::game::{self, Action, GameState, Player};

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
            history: Vec::new(),
            deck: deck,
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
    face_up: Action,
    is_chance_node: bool,
    is_terminal: bool,
    phase: EPhase,
    cur_player: usize,
    history: Vec<Action>,
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

#[derive(Debug, Clone)]
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

    fn apply_action_play(&mut self, a: Action) {}

    fn legal_actions_dealing(&self) -> Vec<Action> {
        return self.deck.clone();
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
            _ => todo!(),
        }
    }

    fn evaluate(&self) -> Vec<f32> {
        todo!()
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
    use crate::euchre::{EAction, EPhase, Euchre};

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
    fn euchre_test_dealing() {
        let mut s = Euchre::new();

        for i in 0..20 {
            s.apply_action(i);
            let legal = s.legal_actions();
            let index = legal.iter().position(|&x| x == i);
            assert_eq!(index, None);
        }
    }
}
