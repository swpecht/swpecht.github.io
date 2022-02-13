use std::collections::HashSet;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use wordle_bot::{filter_answers, find_best_guess, load_word_list, LetterState};

const ANSWER_FILE: &str = "data/wordle-answers-alphabetical.txt";
const GUESS_FILE: &str = "data/wordle-allowed-guesses.txt";

fn criterion_benchmark(c: &mut Criterion) {
    let mut answers = HashSet::new();
    load_word_list(ANSWER_FILE, &mut answers);
    let mut guesses = HashSet::new();
    load_word_list(GUESS_FILE, &mut guesses);
    load_word_list(ANSWER_FILE, &mut guesses);

    // Filter down to ~100 answers
    answers = filter_answers(
        &['r', 'o', 'a', 't', 'e'],
        [
            LetterState::Gray,
            LetterState::Gray,
            LetterState::Yellow,
            LetterState::Gray,
            LetterState::Gray,
        ],
        &answers,
    );

    c.bench_function("find best guess", |b| {
        b.iter(|| find_best_guess(black_box(&answers), &guesses))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
