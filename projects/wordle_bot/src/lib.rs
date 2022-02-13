use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{self, BufRead},
    path::Path,
};

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum LetterState {
    /// Letter is in the right position
    Green,
    /// Letter is in the word, but wrong position
    Yellow,
    /// Letter not in word
    Gray,
}

/// Returns Green, Yellow, Gray for a given guess and answer
pub fn score_guess(guess: &str, answer: &str) -> [LetterState; 5] {
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

pub fn filter_answers(
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
    // TODO: need to account for double letters, do an optional for known char counts
    let mut known_char_counts = [0; 26];
    let mut is_absent = [false; 26];
    for i in 0..5 {
        let g = guess.chars().nth(i).unwrap();
        let index = get_index(g);
        match score[i] {
            LetterState::Yellow | LetterState::Green => known_char_counts[index] += 1,
            LetterState::Gray => is_absent[index] = true,
        }
    }

    for answer in &filtered.clone() {
        let mut char_count = [0; 26];
        for a in answer.chars() {
            increment_count(a, &mut char_count);
        }

        for i in 0..26 {
            if (char_count[i] < known_char_counts[i])
                // Need to check for known_char_counts being zero to handle double letters
                || (char_count[i] > 0 && is_absent[i] && known_char_counts[i] == 0)
            {
                filtered.remove(answer);
            }
        }
    }

    return filtered;
}

fn increment_count(c: char, counts: &mut [usize; 26]) {
    let index = get_index(c);
    counts[index] += 1;
}

fn get_index(c: char) -> usize {
    const A_DECIMAL: usize = 97;
    let index = c.to_ascii_lowercase() as usize - A_DECIMAL;
    return index;
}

/// Returns all possible scores
///
/// Could model this as a bit mask
/// Every 2 bits corresponds to the flag state
///
fn get_all_scores() -> Vec<[LetterState; 5]> {
    let mut cur = [LetterState::Green; 5];
    let mut scores = Vec::new();
    scores.push(cur);

    while cur != [LetterState::Gray; 5] {
        for i in 0..5 {
            match cur[i] {
                LetterState::Green => {
                    cur[i] = LetterState::Yellow;
                    scores.push(cur);
                    break;
                }
                LetterState::Yellow => {
                    cur[i] = LetterState::Gray;
                    scores.push(cur);
                    break;
                }
                LetterState::Gray => cur[i] = LetterState::Green, // restart
            }
        }
    }

    return scores;
}

/// Returns the expected value for the number of remaining answers after the guess
///
/// This method can be used to iterate overall all possible guesses. The guess with the
/// lowest expected value of remaining answers is the best guess
pub fn evaluate_guess(guess: &str, answers: &HashSet<String>) -> usize {
    let mut expected_remaining_answers = 0;
    for score in get_all_scores() {
        let remaining_answers = filter_answers(guess, score, answers);

        // E[] = Sum( P(# answers) * # answers )
        // P(# answers) = # answers/ total answers
        // Factor out the total answers and divide at the end
        expected_remaining_answers += remaining_answers.len() * remaining_answers.len();
    }
    return (expected_remaining_answers as f64 / answers.len() as f64) as usize;
}

pub fn load_word_list(path: &str, set: &mut HashSet<String>) {
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

    use crate::{filter_answers, get_all_scores, score_guess, LetterState};

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
        let answers = HashSet::from_iter(vec!["cxxxx".to_string(), "bxxxx".to_string()]);
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
            "xaxxe".to_string(), // Not included, A not in answer
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

    #[test]
    fn test_filter_weary() {
        let answers = HashSet::from_iter(vec![
            "warby".to_string(), // no
            "wordy".to_string(), // yes
            "wormy".to_string(), // yes
            "wryly".to_string(), // yes
        ]);
        let filtered = filter_answers(
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
        assert_eq!(filtered.len(), 3);
    }

    #[test]
    fn test_filter_digit() {
        let answer_str: Vec<String> = ["robin", "roomy", "rowdy", "round", "rocky", "rough"]
            .iter()
            .map(|&s| s.into())
            .collect();
        let answers = HashSet::from_iter(answer_str);
        let filtered = filter_answers(
            "digit",
            [
                LetterState::Gray,
                LetterState::Gray,
                LetterState::Gray,
                LetterState::Green,
                LetterState::Gray,
            ],
            &answers,
        );
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_get_all_scores() {
        let scores = get_all_scores();
        assert_eq!(scores.len(), 243); //3^5 options
    }
}
