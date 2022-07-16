use core::panic;

use log::info;
/// Game implementation for liars poker.
use rand::Rng;

const NUM_DICE: usize = 4;

#[derive(Clone, Copy, PartialEq)]
enum DiceState {
    U,     // unknown
    K(u8), // Known
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
enum BetState {
    NB, // Not bet
    P1, // Player 1
    P2, // Player 2
}

#[derive(Clone, PartialEq)]
pub struct GameState {
    dice_state: [DiceState; NUM_DICE],

    // There are 6 possible values for the dice, can wager up to the
    // number of dice for each value
    bet_state: [BetState; NUM_DICE * 6],

    /// Track if someone has called the other player
    call_state: BetState,
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

    /// Play 1 step of the game, return score. 1 if P1 wins, -1 if P2,
    /// 0 is no winner
    pub fn step(&mut self, agent: fn(&Vec<GameState>) -> usize) -> i32 {
        // TODO, implement filtering of dice state
        let fitlered_state = filter_state(&self.game_state);
        let possible_moves = get_possible_moves(&fitlered_state);
        let choice = agent(&possible_moves);

        // Only copy over the bet state since the game state is filtered
        self.game_state.bet_state = possible_moves[choice].bet_state.clone();
        self.game_state.call_state = possible_moves[choice].call_state;

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

    let last_guess = get_last_bet(g);

    let value = last_guess % 6 + 1;
    let num_dice = last_guess / 6 + 1;

    let mut counted_dice = 0;
    for d in g.dice_state {
        match d {
            DiceState::K(x) if usize::from(x) == value => counted_dice += 1,
            _ => {}
        }
    }

    info!(
        "{:?} called against {} {}s there were {} {}s",
        g.call_state, num_dice, value, counted_dice, value
    );

    let call_is_correct = counted_dice >= num_dice;
    match (g.call_state, call_is_correct) {
        (BetState::P1, false) => return 1,
        (BetState::P2, true) => return 1,
        (BetState::NB, _) => panic!("Invalid player state"),
        _ => return -1,
    }
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

fn get_last_bet(g: &GameState) -> usize {
    // Get the last guesser
    let mut last_guess = 0;
    for i in 0..g.bet_state.len() {
        match g.bet_state[i] {
            BetState::P1 => last_guess = i,
            BetState::P2 => last_guess = i,
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

/// Return the list of GameStates reachable from a given state
fn get_possible_moves(g: &GameState) -> Vec<GameState> {
    let mut possible_moves = Vec::new();

    // Check for call
    if g.call_state != BetState::NB {
        return possible_moves; // no possible moves
    }

    // Only need to handle bets, as the next reachable state can't reveal dice
    let next_beter = get_acting_player(g);
    let last_bet = get_last_bet(g);

    // Increasing bets
    for i in last_bet + 1..g.bet_state.len() {
        let mut s = g.clone();
        s.bet_state[i] = next_beter;
        possible_moves.push(s);
    }

    // Call bluff
    let mut s = g.clone();
    s.call_state = next_beter;
    possible_moves.push(s);

    return possible_moves;
}

#[cfg(test)]
mod tests {
    use crate::liars_poker::{get_possible_moves, BetState, DiceState, GameState, NUM_DICE};

    #[test]
    fn move_getter() {
        let mut g = GameState {
            dice_state: [DiceState::K(1), DiceState::K(1), DiceState::U, DiceState::U],
            bet_state: [BetState::NB; NUM_DICE * 6],
            call_state: BetState::NB,
        };

        let moves = get_possible_moves(&g);
        assert_eq!(moves.len(), NUM_DICE * 6);

        g.bet_state[4] = BetState::P1;
        let moves = get_possible_moves(&g);
        assert_eq!(moves.len(), NUM_DICE * 6 - 4);

        g.bet_state[5] = BetState::P2;
        let moves = get_possible_moves(&g);
        assert_eq!(moves.len(), NUM_DICE * 6 - 5);
    }
}
