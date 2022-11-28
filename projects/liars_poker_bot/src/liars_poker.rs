use std::ops;

use itertools::Itertools;
/// Game implementation for liars poker.
use rand::Rng;

use crate::game::GameState;

pub const NUM_DICE: usize = 4;
pub const DICE_SIDES: usize = 3;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LPAction {
    Bet(usize),
    Call,
}

impl std::fmt::Debug for LPAction {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            LPAction::Call => write!(f, "C"),
            LPAction::Bet(x) => write!(f, "{:?}", parse_bet(*x)),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Eq)]
pub enum Player {
    P1,
    P2,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DiceState {
    U,        // unknown
    K(usize), // Known
}

impl DiceState {
    pub fn unwrap_or(&self, x: usize) -> usize {
        return match self {
            DiceState::K(v) => *v,
            _ => x,
        };
    }
}

impl ops::Add<usize> for DiceState {
    type Output = DiceState;

    fn add(self, rhs: usize) -> DiceState {
        match self {
            DiceState::K(x) => DiceState::K(x + rhs),
            DiceState::U => DiceState::U,
        }
    }
}

impl std::fmt::Debug for DiceState {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            DiceState::U => write!(f, "U"),
            DiceState::K(x) => write!(f, "{}", x),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct LPGameState {
    pub dice_state: [DiceState; NUM_DICE],

    // There are DICE_SIDES possible values for the dice, can wager up to the
    // number of dice for each value
    pub bet_state: [Option<Player>; NUM_DICE * DICE_SIDES],

    /// Track if someone has called the other player
    pub call_state: Option<Player>,
}

impl LPGameState {
    pub fn apply(&mut self, p: Player, a: &LPAction) {
        match a {
            LPAction::Bet(i) => self.bet_state[*i] = Some(p),
            LPAction::Call => self.call_state = Some(p),
        };
    }
}

impl GameState for LPGameState {
    fn evaluate(&self) -> i32 {
        if self.call_state == None {
            return 0; // game isn't over
        }

        let (num_dice, state) = parse_highest_bet(self).unwrap();

        let mut counted_dice = 0;
        for d in self.dice_state {
            match d {
                DiceState::K(x) if x == state => counted_dice += 1,
                _ => {}
            }
        }

        let call_is_correct = counted_dice < num_dice;

        match (self.call_state, call_is_correct) {
            (Some(Player::P1), true) => return 1,
            (Some(Player::P2), true) => return -1,
            (Some(Player::P1), false) => return -1,
            (Some(Player::P2), false) => return 1,
            (None, _) => return 0,
        }
    }

    fn get_acting_player(&self) -> Player {
        // Check for a call
        match self.call_state {
            Some(Player::P1) => return Player::P2,
            Some(Player::P2) => return Player::P1,
            _ => {}
        }

        // Get the last guesser
        let mut last_guesser = None;
        for i in 0..self.bet_state.len() {
            match self.bet_state[i] {
                Some(Player::P1) => {
                    last_guesser = Some(Player::P1);
                }
                Some(Player::P2) => {
                    last_guesser = Some(Player::P2);
                }
                _ => {}
            }
        }

        let next_guesser = match last_guesser {
            Some(Player::P1) => Player::P2,
            Some(Player::P2) => Player::P1,
            None => Player::P1,
        };

        return next_guesser;
    }

    fn get_possible_states(&self) -> Vec<Self> {
        let known_dice = self
            .dice_state
            .iter()
            .filter(|&x| match x {
                DiceState::K(_) => true,
                _ => false,
            })
            .collect_vec();

        let num_unknown = self
            .dice_state
            .iter()
            .filter(|&x| *x == DiceState::U)
            .count();
        if num_unknown == 0 {
            return vec![self.clone()];
        }

        let unknown_dice = (0..num_unknown)
            .map(|_| 0..DICE_SIDES)
            .multi_cartesian_product();
        let mut dice_state = [DiceState::K(1); NUM_DICE];

        for i in 0..known_dice.len() {
            dice_state[i] = *known_dice[i];
        }

        let mut states = Vec::new();
        for p in unknown_dice {
            let mut guess = p.iter();
            for i in known_dice.len()..NUM_DICE {
                dice_state[i] = DiceState::K(*guess.next().unwrap());
            }

            let mut state = self.clone();
            state.dice_state = dice_state;
            states.push(state);
        }

        return states;
    }

    fn get_filtered_state(&self, player: Player) -> Self {
        let mut f = self.clone();
        let filter_start = match player {
            Player::P1 => NUM_DICE / 2,
            Player::P2 => 0,
        };

        for i in filter_start..filter_start + NUM_DICE / 2 {
            f.dice_state[i] = DiceState::U;
        }

        return f;
    }

    fn new() -> Self {
        let mut rng = rand::thread_rng();
        let mut dice_state = [DiceState::U; NUM_DICE];
        for i in 0..NUM_DICE {
            dice_state[i] = DiceState::K(rng.gen_range(0..DICE_SIDES));
        }

        LPGameState {
            dice_state: dice_state,
            bet_state: [None; NUM_DICE * DICE_SIDES],
            call_state: None,
        }
    }

    fn is_terminal(&self) -> bool {
        match self.call_state {
            Some(_) => true,
            _ => false,
        }
    }

    fn get_children(&self) -> Vec<Self> {
        let mut children = Vec::new();
        let p = self.get_acting_player();

        // Check for call
        if self.call_state != None {
            return children; // no possible moves
        }

        let last_bet = get_last_bet(self);
        let start_index = match last_bet {
            None => 0,
            Some(x) => x + 1,
        };

        // Increasing bets
        for i in start_index..self.bet_state.len() {
            let mut c = self.clone();
            c.bet_state[i] = Some(p);
            children.push(c);
        }

        // Call bluff, only possible if another bet has been made
        if self.bet_state.iter().any(|&x| x != None) {
            let mut c = self.clone();
            c.call_state = Some(p);
            children.push(c);
        }

        return children;
    }
}

impl std::fmt::Debug for LPGameState {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{:?}, {:?}, {:?}",
            self.dice_state, self.bet_state, self.call_state
        )
    }
}

/// Returns (num_dice, value)
pub fn parse_highest_bet(g: &LPGameState) -> Option<(usize, usize)> {
    let last_guess = get_last_bet(g);
    match last_guess {
        Some(i) => return Some(parse_bet(i)),
        None => return None,
    };
}

pub fn parse_bet(i: usize) -> (usize, usize) {
    let value = i % DICE_SIDES;
    let num_dice = i / DICE_SIDES + 1;
    return (num_dice, value);
}

pub fn get_last_bet(g: &LPGameState) -> Option<usize> {
    for i in (0..g.bet_state.len()).rev() {
        match g.bet_state[i] {
            Some(_) => return Some(i),
            _ => {}
        }
    }
    return None;
}

#[cfg(test)]
mod tests {
    use crate::{
        game::GameState,
        liars_poker::{DiceState, LPGameState, Player, DICE_SIDES, NUM_DICE},
    };

    #[test]
    fn move_getter() {
        let mut g = LPGameState {
            dice_state: [DiceState::K(0); NUM_DICE],
            bet_state: [None; NUM_DICE * DICE_SIDES],
            call_state: None,
        };

        let moves = g.get_children();
        assert_eq!(moves.len(), NUM_DICE * DICE_SIDES);

        g.bet_state[4] = Some(Player::P1);
        let moves = g.get_children();
        assert_eq!(moves.len(), NUM_DICE * DICE_SIDES - 5 + 1);

        g.bet_state[5] = Some(Player::P2);
        let moves = g.get_children();
        assert_eq!(moves.len(), NUM_DICE * DICE_SIDES - 6 + 1);
    }

    #[test]
    fn test_get_winner() {
        let mut g = LPGameState {
            dice_state: [DiceState::K(0); NUM_DICE],
            bet_state: [None; NUM_DICE * DICE_SIDES],
            call_state: None,
        };

        g.bet_state[0] = Some(Player::P1);
        g.call_state = Some(Player::P2);
        let winner = g.evaluate();
        assert!(winner > 0);

        g.dice_state = [DiceState::K(1); NUM_DICE];
        let winner = g.evaluate();
        assert!(winner < 0);
    }
}
