use std::fmt::Display;

use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use crate::{
    actions,
    algorithms::ismcts::ResampleFromInfoState,
    collections::SortedArrayVec,
    game::{Action, Game, GameState, Player},
    istate::IStateKey,
};

use self::actions::{EAction, Face, Suit, CARD_PER_SUIT};

const NUM_CARDS: usize = 24;
const CARDS_PER_HAND: usize = 5;

pub mod actions;

pub struct Euchre {}
impl Euchre {
    pub fn new_state() -> EuchreGameState {
        let keys = [IStateKey::new(); 4];
        let hands = [SortedArrayVec::<Action, 5>::new(); 4];

        EuchreGameState {
            num_players: 4,
            hands,
            is_chance_node: true,
            is_terminal: false,
            phase: EPhase::DealHands,
            cur_player: 0,
            trump: Suit::Clubs,     // Default to one for now
            face_up: EAction::Pass, // Default for now
            trump_caller: 0,
            istate_keys: keys,
            first_played: None,
            discard: None,
            trick_winners: [0; 5],
            key: IStateKey::new(),
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
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EuchreGameState {
    num_players: usize,
    /// Holds the cards for each player in the game
    hands: [SortedArrayVec<Action, 5>; 4],
    trump: Suit,
    trump_caller: usize,
    face_up: EAction,
    is_chance_node: bool,
    is_terminal: bool,
    phase: EPhase,
    cur_player: usize,
    istate_keys: [IStateKey; 4],
    /// index of the game key where the first played card is
    /// used to make looking up tricks easier
    first_played: Option<usize>,
    /// index of the discard action in the game key if one occured
    discard: Option<usize>,
    /// keep track of who has won tricks to avoid re-computing
    trick_winners: [Player; 5],
    key: IStateKey,
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
        self.hands[self.cur_player].push(a);

        if self.hands[self.cur_player].len() == CARDS_PER_HAND {
            self.cur_player = (self.cur_player + 1) % self.num_players
        }

        if self.hands.len() == self.num_players && self.hands[self.num_players - 1].len() == 5 {
            self.phase = EPhase::DealFaceUp;
        }
    }

    fn apply_action_deal_face_up(&mut self, a: Action) {
        self.face_up = EAction::from(a);
        self.phase = EPhase::Pickup;
        self.cur_player = 0;
        self.is_chance_node = false;
    }

    fn apply_action_pickup(&mut self, a: Action) {
        match EAction::from(a) {
            EAction::Pass => {
                if self.cur_player == self.num_players - 1 {
                    // Dealer has passed, move to choosing
                    self.phase = EPhase::ChooseTrump;
                }
                self.cur_player = (self.cur_player + 1) % self.num_players;
            }
            EAction::Pickup => {
                self.trump_caller = self.cur_player;
                self.trump = self.get_suit(self.face_up.into());
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
            self.phase = EPhase::Play;
        }
    }

    /// Can only be done by the dealer (player 3)
    fn apply_action_discard(&mut self, a: Action) {
        self.discard = Some(self.key.len() - 1);

        if !self.hands[3].contains(&a) {
            panic!("attempted to discard a card not in hand")
        }
        self.hands[3].remove(a);
        self.hands[3].push(self.face_up.into());

        self.cur_player = 0;
        self.phase = EPhase::Play;
    }

    fn apply_action_play(&mut self, a: Action) {
        if self.first_played.is_none() {
            self.first_played = Some(self.key.len() - 1);
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

            // save the trick winner for later
            let trick = ((self.key.len() - self.first_played.unwrap()) / 4) - 1;
            self.trick_winners[trick] = winner;
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

        (self.key.len() - self.first_played.unwrap()) % 4 == 0
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
        let mut trick = [Action::default(); 4];
        for i in 0..4 {
            trick[i] = key[sidx + i];
        }

        trick
    }

    /// Get the card that started the current trick
    fn get_leading_card(&self) -> Action {
        if self.phase != EPhase::Play {
            panic!("tried to get leading card of trick at invalid time")
        }

        let min_hand = self.hands.iter().map(|x| x.len()).min().unwrap();
        let cards_played = self.hands.iter().filter(|&x| x.len() == min_hand).count();

        let key = self.istate_keys[0];

        key[key.len() - cards_played]
    }

    fn legal_actions_dealing(&self, actions: &mut Vec<Action>) {
        for i in 0..NUM_CARDS {
            let mut is_dealt = false;
            for h in 0..self.num_players {
                if self.hands[h].contains(&EAction::Card { a: i as u8 }.into()) {
                    is_dealt = true;
                    break;
                }
            }
            if !is_dealt {
                actions.push(EAction::Card { a: i as u8 }.into());
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

        let face_up = self.get_suit(self.face_up.into());
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
        // If they are the first to act on a trick then can play any card in hand
        if self.is_trick_over() {
            for i in 0..self.hands[self.cur_player].len() {
                actions.push(self.hands[self.cur_player][i]);
            }
            return;
        }

        let leading_card = self.get_leading_card();
        let suit = self.get_suit(leading_card);

        for i in 0..self.hands[self.cur_player].len() {
            let c = self.hands[self.cur_player][i];
            if self.get_suit(c) == suit {
                actions.push(c);
            }
        }

        if actions.is_empty() {
            // no suit, can play any card
            for i in 0..self.hands[self.cur_player].len() {
                actions.push(self.hands[self.cur_player][i]);
            }
        }
    }

    /// Returns the player who won the trick
    fn evaluate_trick(&self, cards: &[Action], trick_starter: Player) -> Player {
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
            if suit == winning_suit && suit != self.trump && c > winning_card {
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
    fn get_suit(&self, c: Action) -> Suit {
        let c = EAction::from(c);
        let mut suit = c.get_suit();
        let face = c.get_face();

        // Correct the jack if in play phase
        if self.phase == EPhase::Play && face == Face::J {
            suit = match (suit, self.trump) {
                (Suit::Clubs, Suit::Spades) => Suit::Spades,
                (Suit::Spades, Suit::Clubs) => Suit::Clubs,
                (Suit::Hearts, Suit::Diamonds) => Suit::Diamonds,
                (Suit::Diamonds, Suit::Hearts) => Suit::Hearts,
                _ => suit,
            }
        }
        suit
    }

    /// Returns a relative value for cards. The absolute values are meaningyless
    /// but can be used to compare card values of the same suit. It accounts for
    /// left and right jack.
    fn get_card_value(&self, c: Action) -> usize {
        let face = EAction::from(c).get_face();
        if self.get_suit(c) != self.trump || self.phase != EPhase::Play || face != Face::J {
            return face as usize;
        }

        // Get the suit "on the card" determine if left or right
        let pure_suit = EAction::from(c).get_suit();
        let is_right = pure_suit == self.trump;

        match is_right {
            true => (CARD_PER_SUIT + 2) as usize,  // right
            false => (CARD_PER_SUIT + 1) as usize, // left
        }
    }

    fn update_keys(&mut self, a: Action) {
        self.key.push(a);

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

    fn legal_actions(&self, actions: &mut Vec<Action>) {
        actions.clear();

        match self.phase {
            EPhase::DealHands | EPhase::DealFaceUp => self.legal_actions_dealing(actions),
            EPhase::Pickup => {
                actions.append(&mut vec![EAction::Pass.into(), EAction::Pickup.into()])
            }
            EPhase::Discard => actions.append(&mut self.hands[3].to_vec()), // Dealer can discard any card
            EPhase::ChooseTrump => self.legal_actions_choose_trump(actions),
            EPhase::Play => self.legal_actions_play(actions),
        };
    }

    fn evaluate(&self, p: Player) -> f64 {
        if !self.is_terminal {
            panic!("evaluate called on non-terminal gamestate");
        }

        let mut won_tricks = [0; 2];
        for i in 0..self.trick_winners.len() {
            let winner = self.trick_winners[i] % 2;
            won_tricks[winner] += 1;
        }

        let team_0_win = won_tricks[0] > won_tricks[1];
        let team_0_call = self.trump_caller % 2 == 0;

        let v = match (team_0_win, team_0_call, won_tricks[0]) {
            (true, true, 5) => vec![2.0, 0.0, 2.0, 0.0],
            (true, true, _) => vec![1.0, 0.0, 1.0, 0.0],
            (true, false, _) => vec![2.0, 0.0, 2.0, 0.0],
            (false, false, 0) => vec![0.0, 2.0, 0.0, 2.0],
            (false, false, _) => vec![0.0, 1.0, 0.0, 1.0],
            (false, true, _) => vec![0.0, 2.0, 0.0, 2.0],
        };

        v[p]
    }

    /// Returns an information state with the following format:
    /// * 0-4: hand
    /// * 5: face up card
    /// * 6-13: calling and passing for trump call
    /// * 14: Calling player
    /// * 15: trump
    /// * 16+: play history
    fn istate_key(&self, player: Player) -> IStateKey {
        self.istate_keys[player]
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
            let s = EAction::from(a).to_string();
            r.push_str(&s);
        }

        if self.phase == EPhase::DealFaceUp {
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

        if self.phase == EPhase::Pickup {
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

    fn key(&self) -> IStateKey {
        self.key
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

        if self.phase == EPhase::Play {
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
    use rand::thread_rng;

    use crate::{
        actions,
        agents::{Agent, RandomAgent},
        algorithms::ismcts::ResampleFromInfoState,
        game::euchre::{EAction, EPhase, Euchre, Suit},
    };

    use super::GameState;

    #[test]
    fn euchre_test_phases_choose_trump() {
        let mut s = Euchre::new_state();

        assert_eq!(s.phase, EPhase::DealHands);
        for i in 0..20 {
            s.apply_action(EAction::Card { a: i }.into());
        }

        assert_eq!(s.phase, EPhase::DealFaceUp);
        s.apply_action(EAction::Card { a: 20 }.into());

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
            s.apply_action(EAction::Card { a: i }.into());
        }

        assert_eq!(s.phase, EPhase::DealFaceUp);
        s.apply_action(EAction::Card { a: 20 }.into());

        assert_eq!(s.phase, EPhase::Pickup);
        assert!(!s.is_chance_node);
        for _ in 0..3 {
            s.apply_action(EAction::Pass.into());
        }
        s.apply_action(EAction::Pickup.into());

        assert_eq!(s.phase, EPhase::Discard);
        s.apply_action(EAction::Card { a: 19 }.into());

        assert_eq!(s.phase, EPhase::Play);
        assert_eq!(s.cur_player, 0);
    }

    #[test]
    fn euchre_test_legal_actions() {
        let mut gs = Euchre::new_state();

        for i in 0..20 {
            gs.apply_action(EAction::Card { a: i }.into());
            let legal = actions!(gs);
            for j in 0..i + 1 {
                let index = legal
                    .iter()
                    .position(|&x| x == EAction::Card { a: j }.into());
                assert_eq!(index, None);
            }
        }

        // Deal the face up card
        gs.apply_action(EAction::Card { a: 21 }.into());
        assert_eq!(gs.face_up, EAction::Card { a: 21 });

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
        assert_eq!(gs.phase, EPhase::Discard);
        gs.apply_action(EAction::from("QH").into());

        // Cards player 0s hand
        assert_eq!(gs.phase, EPhase::Play);
        assert_eq!(
            actions!(gs)
                .iter()
                .map(|x| EAction::from(*x).to_string())
                .collect_vec(),
            vec!["9C", "TC", "JC", "QC", "KC"]
        );

        gs.apply_action(EAction::from("9C").into());
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

        assert_eq!(s.get_suit(EAction::from("9C").into()), Suit::Clubs);
        // Jack of spades is still a spade
        assert_eq!(s.get_suit(EAction::from("JS").into()), Suit::Spades);
        assert_eq!(s.get_suit(EAction::from("TS").into()), Suit::Spades);

        // Deal the cards
        for i in 1..21 {
            s.apply_action(EAction::Card { a: i }.into());
        }

        s.apply_action(EAction::from("9C").into()); // Deal the 9 face up
        s.apply_action(EAction::Pickup.into());
        s.apply_action(EAction::Card { a: 20 }.into());
        assert_eq!(s.trump, Suit::Clubs);
        assert_eq!(s.phase, EPhase::Play);
        // Jack of spades is now a club since it's trump
        assert_eq!(s.get_suit(EAction::from("JS").into()), Suit::Clubs);
        assert_eq!(s.get_suit(EAction::from("TS").into()), Suit::Spades);
    }

    #[test]
    fn euchre_test_istate() {
        let mut gs = Euchre::new_state();
        // Deal the cards
        for i in 0..20 {
            gs.apply_action(EAction::Card { a: i }.into());
        }

        assert_eq!(gs.istate_string(0), "9CTCJCQCKC");
        assert_eq!(gs.istate_string(1), "AC9STSJSQS");
        assert_eq!(gs.istate_string(2), "KSAS9HTHJH");
        assert_eq!(gs.istate_string(3), "QHKHAH9DTD");

        gs.apply_action(EAction::Card { a: 20 }.into());
        assert_eq!(gs.istate_string(0), "9CTCJCQCKC|JD|");
        assert_eq!(gs.istate_string(1), "AC9STSJSQS|JD|");
        assert_eq!(gs.istate_string(2), "KSAS9HTHJH|JD|");
        assert_eq!(gs.istate_string(3), "QHKHAH9DTD|JD|");

        let mut new_s = gs; // for alternative pickup parsing

        gs.apply_action(EAction::Pickup.into());
        assert_eq!(gs.istate_string(0), "9CTCJCQCKC|JD|T|0D");

        // Dealer discards the QC
        assert_eq!(gs.istate_string(3), "QHKHAH9DTD|JD|T|0D");
        gs.apply_action(EAction::from("QH").into());
        assert_eq!(gs.istate_string(3), "QHKHAH9DTD|JD|T|0D|QH");

        for _ in 0..4 {
            let a = actions!(gs)[0];
            gs.apply_action(a);
        }
        assert_eq!(gs.istate_string(0), "9CTCJCQCKC|JD|T|0D|9CACKSKH");
        assert_eq!(gs.istate_string(1), "AC9STSJSQS|JD|T|0D|9CACKSKH");
        assert_eq!(gs.istate_string(2), "KSAS9HTHJH|JD|T|0D|9CACKSKH");
        assert_eq!(gs.istate_string(3), "QHKHAH9DTD|JD|T|0D|QH|9CACKSKH");

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
}
