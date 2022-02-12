use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{self, BufRead},
    path::Path,
};

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

#[derive(Debug, PartialEq, Copy, Clone)]
enum LetterState {
    /// Letter is in the right position
    Green,
    /// Letter is in the word, but wrong position
    Yellow,
    /// Letter not in word
    Gray,
}

/// Returns Green, Yellow, Gray for a given guess and answer
fn score_guess(guess: &str, answer: &str) -> [LetterState; 5] {
    let mut score = [LetterState::Gray; 5];
    let mut unmatch_chars = HashMap::new();

    for i in 0..5 {
        let answer_char = answer.chars().nth(i).unwrap();
        if guess.chars().nth(i).unwrap() == answer_char {
            score[i] = LetterState::Green;
        } else {
            let count = *unmatch_chars.get(&answer_char).unwrap_or(&0);
            unmatch_chars.insert(answer_char, count + 1);
        }
    }

    for i in 0..5 {
        if score[i] == LetterState::Green {
            continue; // skip already matched chars
        }

        let guess_char = guess.chars().nth(i).unwrap();
        let count = *unmatch_chars.get(&guess_char).unwrap_or(&0);
        if count > 0 {
            score[i] = LetterState::Yellow;
            unmatch_chars.insert(guess_char, count - 1);
        }
    }

    return score;
}

fn filter_answers(
    guess: &str,
    score: [LetterState; 5],
    answers: &HashSet<String>,
) -> HashSet<String> {
    // Start with a copy of all answers and remove ones that can't match
    let mut filtered = answers.clone();

    // Filter letters where they should be, e.g. Green
    for i in 0..5 {
        if score[i] == LetterState::Green {
            let g = guess.chars().nth(i).unwrap();
            for answer in answers {
                if answer.chars().nth(i).unwrap() != g {
                    filtered.remove(answer);
                }
            }
        }
    }

    // Filter letters where they shouldn't be, e.g. Gray and not Green
    for i in 0..5 {
        if score[i] == LetterState::Yellow {
            let g = guess.chars().nth(i).unwrap();
            for answer in answers {
                if answer.chars().nth(i).unwrap() == g {
                    filtered.remove(answer);
                }
            }
        }
    }

    // Filter by char counts
    let mut known_char_counts = HashMap::new();
    for i in 0..5 {
        if score[i] == LetterState::Yellow || score[i] == LetterState::Green {
            let g = guess.chars().nth(i).unwrap();
            increment_count(g, &mut known_char_counts);
        }
    }

    for answer in answers {
        let mut char_count = HashMap::new();
        for a in answer.chars() {
            increment_count(a, &mut char_count);
        }

        for (c, count) in known_char_counts.iter() {
            if char_count.get(c).unwrap_or(&0) < count {
                filtered.remove(answer);
            }
        }
    }

    return filtered;
}

fn increment_count(c: char, counts: &mut HashMap<char, i32>) {
    let count = *counts.get(&c).unwrap_or(&0);
    counts.insert(c, count + 1);
}

/// Returns the expected value for the number of remaining answers after the guess
///
/// This method can be used to iterate overall all possible guesses. The guess with the
/// lowest expected value of remaining answers is the best guess
fn evaluate_guess(guess: &str, answers: &HashSet<String>) -> usize {
    let mut total_remaining_answers = 0;
    for answer in answers {
        let score = score_guess(guess, &answer); // the evaluation
        let remaining_answers = filter_answers(guess, score, answers);
        total_remaining_answers += remaining_answers.len();
    }
    return total_remaining_answers / answers.len();
}

fn load_word_list(path: &str, set: &mut HashSet<String>) {
    if let Ok(lines) = read_lines(path) {
        for line in lines {
            if let Ok(word) = line {
                set.insert(word);
            }
        }
    }
}

// https://doc.rust-lang.org/rust-by-example/std_misc/file/read_lines.html
fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::{filter_answers, score_guess, LetterState};

    #[test]
    fn test_score_correct() {
        let score = score_guess("crane", "crane");
        assert_eq!(score, [LetterState::Green; 5]);

        let score = score_guess("foods", "foods");
        assert_eq!(score, [LetterState::Green; 5]);
    }

    #[test]
    fn test_score_double_letter() {
        let score = score_guess("foods", "nodes");
        assert_eq!(
            score,
            [
                LetterState::Gray,
                LetterState::Green,
                LetterState::Gray,
                LetterState::Yellow,
                LetterState::Green
            ]
        );
    }

    #[test]
    fn test_filter_green_letters() {
        let answers = HashSet::from_iter(vec!["capes".to_string(), "boats".to_string()]);
        let filtered = filter_answers(
            "crane",
            [
                LetterState::Green,
                LetterState::Gray,
                LetterState::Gray,
                LetterState::Gray,
                LetterState::Gray,
            ],
            &answers,
        );
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_yellow_letters() {
        let answers = HashSet::from_iter(vec![
            "cxxxe".to_string(), // Not included, otherwise C would be green
            "xcxxe".to_string(), // answer
            "xxaxe".to_string(), // Not included, A not in answer
            "xxxxe".to_string(), // Not included, no c
        ]);
        let filtered = filter_answers(
            "crane",
            [
                LetterState::Yellow,
                LetterState::Gray,
                LetterState::Gray,
                LetterState::Gray,
                LetterState::Green,
            ],
            &answers,
        );
        assert_eq!(filtered.len(), 1);
    }
}
