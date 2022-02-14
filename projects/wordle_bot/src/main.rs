use std::{
    collections::{HashMap, HashSet},
    io::{self, BufRead},
};

use wordle_bot::{
    filter_answers, find_best_guess, get_all_scores, load_word_list, play_game, LetterState,
};

const ANSWER_FILE: &str = "data/wordle-answers-alphabetical.txt";
const GUESS_FILE: &str = "data/wordle-allowed-guesses.txt";

fn main() {
    evaluate();
    // interactive_mode();
}

fn evaluate() {
    let mut answers = HashSet::new();
    load_word_list(ANSWER_FILE, &mut answers);
    let mut guesses = HashSet::new();
    load_word_list(GUESS_FILE, &mut guesses);
    load_word_list(ANSWER_FILE, &mut guesses);

    println!("calculating starting guess...");
    let starting_guess = find_best_guess(&answers, &guesses);

    // Build lookup table for second guess
    println!("building second guess lookup...");
    let mut second_guess_lookup = HashMap::new();
    for score in get_all_scores() {
        let filtered_answers = filter_answers(&starting_guess, score, &answers);

        if filtered_answers.len() == 0 {
            // impossible state, don't need to pre-compute
            continue;
        }

        let best_guess = find_best_guess(&filtered_answers, &guesses);
        second_guess_lookup.insert(score, best_guess);
    }

    let mut histogram = HashMap::new();

    for answer in &answers {
        let turns = play_game(
            answer,
            &starting_guess,
            &answers,
            &guesses,
            &second_guess_lookup,
        );
        println!(
            "Solved {} in {}",
            answer.into_iter().collect::<String>(),
            turns
        );
        let count = *histogram.get(&turns).unwrap_or(&0);
        histogram.insert(turns, count + 1);

        for i in 0..8 {
            let count = histogram.get(&i).unwrap_or(&0);
            println!("{}: {}", i, &count);
        }
    }
}

fn interactive_mode() {
    let mut answers = HashSet::new();
    load_word_list(ANSWER_FILE, &mut answers);
    let mut guesses = HashSet::new();
    load_word_list(GUESS_FILE, &mut guesses);
    load_word_list(ANSWER_FILE, &mut guesses);

    println!("Loaded {} answers", answers.len());
    println!("Loaded {} guesses", guesses.len());
    let stdin = io::stdin();
    loop {
        println!("Enter guess:");
        let mut guess_string = String::new();
        stdin
            .lock()
            .read_line(&mut guess_string)
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

        let mut guess = ['a'; 5];
        for i in 0..5 {
            guess[i] = guess_string.chars().nth(i).unwrap();
        }

        answers = filter_answers(&guess, score, &answers);
        println!("{} answers remain", answers.len());
        println!("{:?}", answers);

        let best_guess = find_best_guess(&answers, &guesses);
        println!("Best guess: {}", best_guess.into_iter().collect::<String>())
    }
}
