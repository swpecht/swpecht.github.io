const NUM_DICE: usize = 4;

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

#[derive(Debug, Clone, Copy)]
enum GuessState {
    NG, // Not guessed
    P1, // Player 1
    P2, // Player 2
}

struct GameState {
    dice_state: [DiceState; NUM_DICE],

    // There are 6 possible values for the dice, can wager up to the
    // number of dice for each value
    guess_state: [GuessState; NUM_DICE * 6],
}

impl std::fmt::Debug for GameState {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}, {:?}", self.dice_state, self.guess_state)
    }
}

fn main() {
    let g = GameState {
        dice_state: [DiceState::K(1), DiceState::K(1), DiceState::U, DiceState::U],
        guess_state: [GuessState::NG; NUM_DICE * 6],
    };

    println!("{:?}", g);
}
