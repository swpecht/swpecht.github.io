use std::{
    collections::HashSet,
    io::{self, BufRead},
};

use wordle_bot::{evaluate_guess, filter_answers, load_word_list, LetterState};

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

    loop {
        let stdin = io::stdin();

        println!("Enter guess:");
        let mut guess = String::new();
        stdin
            .lock()
            .read_line(&mut guess)
            .expect("Could not read line");

        println!("Enter score (GYX):");
        let mut score_str = String::new();
        stdin
            .lock()
            .read_line(&mut score_str)
            .expect("Could not read line");

        let mut score = [LetterState::Gray; 5];
        for i in 0..5 {
            let c = score_str.chars().nth(i).unwrap();
            match c {
                'G' => score[i] = LetterState::Green,
                'Y' => score[i] = LetterState::Yellow,
                _ => score[i] = LetterState::Gray,
            }
        }

        answers = filter_answers(&guess, score, &answers);
        println!("{} answers remain", answers.len());
        println!("{:?}", answers);

        find_best_guess(&answers, &guesses);
    }
}

fn find_best_guess(answers: &HashSet<String>, guesses: &HashSet<String>) {
    let mut best_guess_score = usize::MAX;
    let mut best_guess = "N/A".to_string();
    let mut count = 0;

    if answers.len() == 1 {
        println!("{:?}", answers);
        return;
    }

    for guess in guesses {
        let expected_answers = evaluate_guess(&guess, &answers);
        count += 1;
        if expected_answers < best_guess_score && expected_answers > 0 {
            best_guess_score = expected_answers;
            best_guess = guess.clone();
            println!(
                "New best guess found: {}, {}, in {}",
                best_guess, expected_answers, count
            );
        }

        if count % 1000 == 0 {
            println!("evaluated: {}", count)
        }
    }

    println!("Best guess: {}, {}", best_guess, best_guess_score);
}
