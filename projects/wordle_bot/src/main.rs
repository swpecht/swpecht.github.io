use std::collections::HashSet;

use wordle_bot::{evaluate_guess, load_word_list};

const ANSWER_FILE: &str = "data/wordle-answers-alphabetical.txt";
const GUESS_FILE: &str = "data/wordle-allowed-guesses.txt";

fn main() {
    let mut answers = HashSet::new();
    load_word_list(ANSWER_FILE, &mut answers);
    let mut guesses = HashSet::new();
    load_word_list(GUESS_FILE, &mut guesses);
    load_word_list(ANSWER_FILE, &mut guesses);

    println!("Loaded {} answers", answers.len());
    println!("Loaded {} guesses", guesses.len());

    let expected_answers = evaluate_guess("crane", &answers);
    println!("crane: {}", expected_answers);

    let expected_answers = evaluate_guess("while", &answers);
    println!("while: {}", expected_answers);
}
