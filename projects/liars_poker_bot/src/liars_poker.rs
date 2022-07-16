use core::panic;

use log::info;
/// Game implementation for liars poker.
use rand::Rng;

const NUM_DICE: usize = 4;

#[derive(Clone, PartialEq)]
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

pub enum Players {
    P1,
    P2,
}

#[derive(Clone, Copy, PartialEq)]
pub enum DiceState {
    U,        // unknown
    K(usize), // Known
}

impl std::fmt::Debug for DiceState {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            DiceState::U => write!(f, "U"),
            DiceState::K(x) => write!(f, "{}", x),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BetState {
    NB, // Not bet
    P1, // Player 1
    P2, // Player 2
}

#[derive(Clone, PartialEq)]
pub struct GameState {
    pub dice_state: [DiceState; NUM_DICE],

    // There are 6 possible values for the dice, can wager up to the
    // number of dice for each value
    pub bet_state: [BetState; NUM_DICE * 6],

    /// Track if someone has called the other player
    pub call_state: BetState,
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
            dice_state[i] = DiceState::K(rng.gen_range(1..6));
        }

        let s = GameState {
            dice_state: dice_state,
            bet_state: [BetState::NB; NUM_DICE * 6],
            call_state: BetState::NB,
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

        // Verify the action is allowed
        if !possible_actions.contains(&a) {
            panic!("Agent attempted invalid action")
        }

        let acting_player = get_acting_player(&self.game_state);
        info!("{:?} played {:?}", acting_player, a);
        // Apply the action
        match a {
            Action::Bet(i) => self.game_state.bet_state[i] = acting_player,
            Action::Call => self.game_state.call_state = acting_player,
        }

        let score = evaluate_state(&self.game_state);
        if score == 1 || score == -1 {
            let winner = match score {
                1 => BetState::P1,
                -1 => BetState::P2,
                _ => panic!("Invalid game state"),
            };
            info!("Winner: {:?}; {:?}", winner, self.game_state);
        }

        return score;
    }
}

/// Return 0 if no one has won, 1 is P1, -1 if P2
fn evaluate_state(g: &GameState) -> i32 {
    if g.call_state == BetState::NB {
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

    info!(
        "{:?} called against {} {:?}s there were {} {:?}s",
        g.call_state, num_dice, state, counted_dice, state
    );

    let call_is_correct = counted_dice >= num_dice;
    match (g.call_state, call_is_correct) {
        (BetState::P1, false) => return 1,
        (BetState::P2, true) => return 1,
        (BetState::NB, _) => panic!("Invalid player state"),
        _ => return -1,
    }
}

/// Returns the number
pub fn parse_highest_bet(g: &GameState) -> Option<(usize, usize)> {
    let last_guess = get_last_bet(g);
    match last_guess {
        Some(i) => return Some(parse_bet(i)),
        None => return None,
    };
}

pub fn parse_bet(i: usize) -> (usize, usize) {
    let value = i % 6 + 1;
    let num_dice = i / 6 + 1;
    return (num_dice, value);
}

/// Returns a filtered version of the gamestate hiding private information
fn filter_state(g: &GameState) -> GameState {
    let mut f = g.clone();
    let acting_player = get_acting_player(g);

    let filter_start = match acting_player {
        BetState::P1 => NUM_DICE / 2,
        BetState::P2 => 0,
        _ => panic!("Invalid player state"),
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
            BetState::P1 => last_guess = Some(i),
            BetState::P2 => last_guess = Some(i),
            _ => {}
        }
    }
    return last_guess;
}

fn get_acting_player(g: &GameState) -> BetState {
    // Check for a call
    match g.call_state {
        BetState::P1 => return BetState::P2,
        BetState::P2 => return BetState::P1,
        _ => {}
    }

    // Get the last guesser
    let mut last_guesser = BetState::NB;
    for i in 0..g.bet_state.len() {
        match g.bet_state[i] {
            BetState::P1 => {
                last_guesser = BetState::P1;
            }
            BetState::P2 => {
                last_guesser = BetState::P2;
            }
            _ => {}
        }
    }

    let next_guesser = match last_guesser {
        BetState::P1 => BetState::P2,
        BetState::P2 => BetState::P1,
        BetState::NB => BetState::P1,
    };

    return next_guesser;
}

fn get_possible_actions(g: &GameState) -> Vec<Action> {
    let mut actions = Vec::new();

    // Check for call
    if g.call_state != BetState::NB {
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
    if g.bet_state.iter().any(|&x| x != BetState::NB) {
        actions.push(Action::Call);
    }

    return actions;
}

#[cfg(test)]
mod tests {
    use crate::liars_poker::{get_possible_actions, BetState, DiceState, GameState, NUM_DICE};

    #[test]
    fn move_getter() {
        let mut g = GameState {
            dice_state: [DiceState::K(1), DiceState::K(1), DiceState::U, DiceState::U],
            bet_state: [BetState::NB; NUM_DICE * 6],
            call_state: BetState::NB,
        };

        let moves = get_possible_actions(&g);
        assert_eq!(moves.len(), NUM_DICE * 6);

        g.bet_state[4] = BetState::P1;
        let moves = get_possible_actions(&g);
        assert_eq!(moves.len(), NUM_DICE * 6 - 5 + 1);

        g.bet_state[5] = BetState::P2;
        let moves = get_possible_actions(&g);
        assert_eq!(moves.len(), NUM_DICE * 6 - 6 + 1);
    }
}
