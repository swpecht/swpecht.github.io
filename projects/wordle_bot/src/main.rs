use std::collections::HashSet;

use wordle_bot::{evaluate_guess, filter_answers, load_word_list, LetterState};

const ANSWER_FILE: &str = "data/wordle-answers-alphabetical.txt";
const GUESS_FILE: &str = "data/wordle-allowed-guesses.txt";

fn main() {
    let mut answers = HashSet::new();
    load_word_list(GUESS_FILE, &mut answers);
    let r = filter_answers(
        "weary",
        [
            LetterState::Green,
            LetterState::Gray,
            LetterState::Gray,
            LetterState::Yellow,
            LetterState::Green,
        ],
        &answers,
    );

    println!("Num: {}", r.len());
    println!("{:?}", r);

    find_best_guess();
}

fn find_best_guess() {
    let mut answers = HashSet::new();
    load_word_list(ANSWER_FILE, &mut answers);
    let mut guesses = HashSet::new();
    load_word_list(GUESS_FILE, &mut guesses);
    load_word_list(ANSWER_FILE, &mut guesses);

    println!("Loaded {} answers", answers.len());
    println!("Loaded {} guesses", guesses.len());

    let mut best_guess_score = usize::MAX;
    let mut count = 0;

    for guess in guesses {
        let expected_answers = evaluate_guess("crane", &answers);
        count += 1;
        if expected_answers < best_guess_score {
            best_guess_score = expected_answers;
            println!(
                "New best guess found: {}, {}, in {}",
                guess, expected_answers, count
            );
        }

        if count % 10 == 0 {
            println!("evaluated: {}", count)
        }
    }
}
