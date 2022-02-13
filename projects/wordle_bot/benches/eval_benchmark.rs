use std::collections::HashSet;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use wordle_bot::{evaluate_guess, load_word_list};

const ANSWER_FILE: &str = "data/wordle-answers-alphabetical.txt";

fn criterion_benchmark(c: &mut Criterion) {
    let mut answers = HashSet::new();
    load_word_list(ANSWER_FILE, &mut answers);

    c.bench_function("eval crane", |b| {
        b.iter(|| evaluate_guess(black_box(&['c', 'r', 'a', 'n', 'e']), &answers))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
