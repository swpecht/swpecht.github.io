use core::panic;
use std::ops;

use log::{debug, info};
/// Game implementation for liars poker.
use rand::Rng;

use crate::{
    agents::Agent,
    game::{Game, GameState},
};

pub const NUM_DICE: usize = 4;
pub const DICE_SIDES: usize = 3;

#[derive(Clone, Copy, PartialEq)]
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

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Player {
    P1,
    P2,
}

#[derive(Clone, Copy, PartialEq)]
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

#[derive(Clone, PartialEq)]
pub struct LPGameState {
    pub dice_state: [DiceState; NUM_DICE],

    // There are DICE_SIDES possible values for the dice, can wager up to the
    // number of dice for each value
    pub bet_state: [Option<Player>; NUM_DICE * DICE_SIDES],

    /// Track if someone has called the other player
    pub call_state: Option<Player>,
}

impl GameState for LPGameState {
    type Action = LPAction;

    fn get_actions(&self) -> Vec<Self::Action> {
        let mut actions = Vec::new();

        // Check for call
        if self.call_state != None {
            return actions; // no possible moves
        }

        let last_bet = get_last_bet(self);
        let start_index = match last_bet {
            None => 0,
            Some(x) => x + 1,
        };

        // Increasing bets
        for i in start_index..self.bet_state.len() {
            actions.push(LPAction::Bet(i));
        }

        // Call bluff, only possible if another bet has been made
        if self.bet_state.iter().any(|&x| x != None) {
            actions.push(LPAction::Call);
        }

        return actions;
    }

    fn apply(&mut self, a: &Self::Action) {
        let acting_player = get_acting_player(self);
        match a {
            LPAction::Bet(i) => self.bet_state[*i] = Some(acting_player),
            LPAction::Call => self.call_state = Some(acting_player),
        };
    }

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

pub struct LiarsPoker {
    game_state: LPGameState,
}

impl Game for LiarsPoker {
    type G = LPGameState;
    type Action = LPAction;

    fn new() -> Self {
        let mut rng = rand::thread_rng();
        let mut dice_state = [DiceState::U; NUM_DICE];
        for i in 0..NUM_DICE {
            dice_state[i] = DiceState::K(rng.gen_range(0..DICE_SIDES));
        }

        let s = LPGameState {
            dice_state: dice_state,
            bet_state: [None; NUM_DICE * DICE_SIDES],
            call_state: None,
        };

        info!("Game created: {:?}", s);

        Self { game_state: s }
    }

    // Play through a game. Positive if P1 wins, negative is P2 wins
    fn play(
        &mut self,
        p1: &(impl Agent<LPGameState> + ?Sized),
        p2: &(impl Agent<LPGameState> + ?Sized),
    ) -> i32 {
        info!("{} playing {}", p1.name(), p2.name());
        let mut score = 0;
        let mut is_player1_turn = true;
        while score == 0 {
            match is_player1_turn {
                true => score = self.step(p1),
                false => score = self.step(p2),
            }

            is_player1_turn = !is_player1_turn;
            debug!("Game state: {:?}", self.game_state);
        }

        return score;
    }
}

impl LiarsPoker {
    /// Play 1 step of the game, return score. 1 if P1 wins, -1 if P2,
    /// 0 is no winner
    fn step(&mut self, agent: &(impl Agent<LPGameState> + ?Sized)) -> i32 {
        let filtered_state = filter_state(&self.game_state);
        let possible_actions = filtered_state.get_actions();
        debug!("{} sees game state of {:?}", agent.name(), filtered_state);
        debug!("{} evaluating moves: {:?}", agent.name(), possible_actions);
        let a = agent.play(&filtered_state, &possible_actions);

        let acting_player = get_acting_player(&self.game_state);
        info!("{:?} tried to play {:?}", acting_player, a);

        // Verify the action is allowed
        if !possible_actions.contains(&a) {
            panic!("Agent attempted invalid action")
        }

        self.game_state.apply(&a);

        let score = self.game_state.evaluate();

        if score != 0 {
            let winner = match score {
                x if x > 0 => Player::P1,
                x if x < 0 => Player::P2,
                _ => panic!("invalid winner"),
            };

            info!("Winner: {:?}; {:?}", winner, self.game_state);
        }

        return score;
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

/// Returns a filtered version of the gamestate hiding private information
fn filter_state(g: &LPGameState) -> LPGameState {
    let mut f = g.clone();
    let acting_player = get_acting_player(g);

    let filter_start = match acting_player {
        Player::P1 => NUM_DICE / 2,
        Player::P2 => 0,
    };

    for i in filter_start..filter_start + NUM_DICE / 2 {
        f.dice_state[i] = DiceState::U;
    }

    return f;
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

pub fn get_acting_player(g: &LPGameState) -> Player {
    // Check for a call
    match g.call_state {
        Some(Player::P1) => return Player::P2,
        Some(Player::P2) => return Player::P1,
        _ => {}
    }

    // Get the last guesser
    let mut last_guesser = None;
    for i in 0..g.bet_state.len() {
        match g.bet_state[i] {
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

#[cfg(test)]
mod tests {
    use crate::{
        game::GameState,
        liars_poker::{DiceState, LPGameState, Player, DICE_SIDES, NUM_DICE},
    };

    #[test]
    fn move_getter() {
        let mut g = LPGameState {
            dice_state: [DiceState::K(1); NUM_DICE],
            bet_state: [None; NUM_DICE * DICE_SIDES],
            call_state: None,
        };

        let moves = g.get_actions();
        assert_eq!(moves.len(), NUM_DICE * DICE_SIDES);

        g.bet_state[4] = Some(Player::P1);
        let moves = g.get_actions();
        assert_eq!(moves.len(), NUM_DICE * DICE_SIDES - 5 + 1);

        g.bet_state[5] = Some(Player::P2);
        let moves = g.get_actions();
        assert_eq!(moves.len(), NUM_DICE * DICE_SIDES - DICE_SIDES + 1);
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
