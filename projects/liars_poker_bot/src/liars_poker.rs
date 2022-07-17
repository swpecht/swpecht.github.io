use core::panic;
use std::ops;

use log::{debug, info};
/// Game implementation for liars poker.
use rand::Rng;

pub const NUM_DICE: usize = 4;
pub const DICE_SIDES: usize = 2;

#[derive(Clone, Copy, PartialEq)]
pub enum Action {
    Bet(usize),
    Call,
}

impl std::fmt::Debug for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Action::Call => write!(f, "C"),
            Action::Bet(x) => write!(f, "{}", x),
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
pub struct GameState {
    pub dice_state: [DiceState; NUM_DICE],

    // There are DICE_SIDES possible values for the dice, can wager up to the
    // number of dice for each value
    pub bet_state: [Option<Player>; NUM_DICE * DICE_SIDES],

    /// Track if someone has called the other player
    pub call_state: Option<Player>,
}

impl std::fmt::Debug for GameState {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{:?}, {:?}, {:?}",
            self.dice_state, self.bet_state, self.call_state
        )
    }
}

pub struct LiarsPoker {
    game_state: GameState,
}

impl LiarsPoker {
    pub fn new() -> Self {
        let mut rng = rand::thread_rng();
        let mut dice_state = [DiceState::U; NUM_DICE];
        for i in 0..NUM_DICE {
            dice_state[i] = DiceState::K(rng.gen_range(0..DICE_SIDES));
        }

        let s = GameState {
            dice_state: dice_state,
            bet_state: [None; NUM_DICE * DICE_SIDES],
            call_state: None,
        };

        info!("Game created: {:?}", s);

        Self { game_state: s }
    }

    // Play through a game. Positive if P1 wins, negative is P2 wins
    pub fn play(
        &mut self,
        p1: fn(&GameState, &Vec<Action>) -> Action,
        p2: fn(&GameState, &Vec<Action>) -> Action,
    ) -> i32 {
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

    /// Play 1 step of the game, return score. 1 if P1 wins, -1 if P2,
    /// 0 is no winner
    pub fn step(&mut self, agent: fn(&GameState, &Vec<Action>) -> Action) -> i32 {
        // TODO, implement filtering of dice state
        let filtered_state = filter_state(&self.game_state);
        let possible_actions = get_possible_actions(&filtered_state);
        let a = agent(&filtered_state, &possible_actions);

        let acting_player = get_acting_player(&self.game_state);
        info!("{:?} tried to play {:?}", acting_player, a);

        // Verify the action is allowed
        if !possible_actions.contains(&a) {
            panic!("Agent attempted invalid action")
        }

        self.game_state = apply_action(&self.game_state, &a);

        let score = evaluate_state(&self.game_state);
        if score == 1 || score == -1 {
            let winner = match score {
                1 => Player::P1,
                -1 => Player::P2,
                _ => panic!("Invalid game state"),
            };
            info!("Winner: {:?}; {:?}", winner, self.game_state);
        }

        return score;
    }
}

/// Return 0 if no one has won, 1 is P1, -1 if P2
fn evaluate_state(g: &GameState) -> i32 {
    if g.call_state == None {
        return 0; // game isn't over
    }

    let (num_dice, state) = parse_highest_bet(g).unwrap();

    let mut counted_dice = 0;
    for d in g.dice_state {
        match d {
            DiceState::K(x) if x == state => counted_dice += 1,
            _ => {}
        }
    }

    let call_is_correct = counted_dice >= num_dice;
    match (g.call_state, call_is_correct) {
        (Some(Player::P1), false) => return 1,
        (Some(Player::P2), true) => return 1,
        (None, _) => 0,
        _ => return -1,
    }
}

pub fn get_winner(g: &GameState) -> Option<Player> {
    return match evaluate_state(g) {
        1 => Some(Player::P1),
        -1 => Some(Player::P2),
        _ => None,
    };
}

/// Returns (num_dice, value)
pub fn parse_highest_bet(g: &GameState) -> Option<(usize, usize)> {
    let last_guess = get_last_bet(g);
    match last_guess {
        Some(i) => return Some(parse_bet(i)),
        None => return None,
    };
}

pub fn parse_bet(i: usize) -> (usize, usize) {
    let value = i % DICE_SIDES + 1;
    let num_dice = i / DICE_SIDES + 1;
    return (num_dice, value);
}

/// Returns a filtered version of the gamestate hiding private information
fn filter_state(g: &GameState) -> GameState {
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

fn get_last_bet(g: &GameState) -> Option<usize> {
    // Get the last guesser
    let mut last_guess = None;
    for i in 0..g.bet_state.len() {
        match g.bet_state[i] {
            Some(Player::P1) => last_guess = Some(i),
            Some(Player::P2) => last_guess = Some(i),
            _ => {}
        }
    }
    return last_guess;
}

fn get_acting_player(g: &GameState) -> Player {
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

pub fn apply_action(g: &GameState, a: &Action) -> GameState {
    let mut f = g.clone();
    let acting_player = get_acting_player(g);
    match a {
        Action::Bet(i) => f.bet_state[*i] = Some(acting_player),
        Action::Call => f.call_state = Some(acting_player),
    };

    return f;
}

pub fn get_possible_actions(g: &GameState) -> Vec<Action> {
    let mut actions = Vec::new();

    // Check for call
    if g.call_state != None {
        return actions; // no possible moves
    }

    let last_bet = get_last_bet(g);
    let start_index = match last_bet {
        None => 0,
        Some(x) => x + 1,
    };

    // Increasing bets
    for i in start_index..g.bet_state.len() {
        actions.push(Action::Bet(i));
    }

    // Call bluff, only possible if another bet has been made
    if g.bet_state.iter().any(|&x| x != None) {
        actions.push(Action::Call);
    }

    return actions;
}

#[cfg(test)]
mod tests {
    use crate::liars_poker::{
        get_possible_actions, DiceState, GameState, Player, DICE_SIDES, NUM_DICE,
    };

    #[test]
    fn move_getter() {
        let mut g = GameState {
            dice_state: [DiceState::K(1); NUM_DICE],
            bet_state: [None; NUM_DICE * DICE_SIDES],
            call_state: None,
        };

        let moves = get_possible_actions(&g);
        assert_eq!(moves.len(), NUM_DICE * DICE_SIDES);

        g.bet_state[4] = Some(Player::P1);
        let moves = get_possible_actions(&g);
        assert_eq!(moves.len(), NUM_DICE * DICE_SIDES - 5 + 1);

        g.bet_state[5] = Some(Player::P2);
        let moves = get_possible_actions(&g);
        assert_eq!(moves.len(), NUM_DICE * DICE_SIDES - DICE_SIDES + 1);
    }
}
