use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{self, BufRead},
    path::Path,
};

#[derive(Debug, PartialEq, Copy, Clone, Eq, Hash)]
pub enum LetterState {
    /// Letter is in the right position
    Green,
    /// Letter is in the word, but wrong position
    Yellow,
    /// Letter not in word
    Gray,
}

/// Returns Green, Yellow, Gray for a given guess and answer
pub fn score_guess(guess: &[char; 5], answer: &[char; 5]) -> [LetterState; 5] {
    let mut score = [LetterState::Gray; 5];
    let mut unmatch_chars = HashMap::new();

    for i in 0..5 {
        let answer_char = answer[i];
        if guess[i] == answer_char {
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

        let guess_char = guess[i];
        let count = *unmatch_chars.get(&guess_char).unwrap_or(&0);
        if count > 0 {
            score[i] = LetterState::Yellow;
            unmatch_chars.insert(guess_char, count - 1);
        }
    }

    return score;
}

/// Returns the number of guesses to get the word
pub fn play_game(
    answer: &[char; 5],
    start_guess: &[char; 5],
    answers: &HashSet<[char; 5]>,
    guesses: &HashSet<[char; 5]>,
    second_guess_lookup: &HashMap<[LetterState; 5], [char; 5]>,
) -> u32 {
    let mut answers = answers.clone();
    let mut num_rounds = 0;
    let mut score = [LetterState::Gray; 5];

    while score != [LetterState::Green; 5] {
        let guess = match num_rounds {
            0 => *start_guess,
            1 => second_guess_lookup.get(&score).unwrap().clone(),
            _ => find_best_guess(&answers, guesses),
        };
        score = score_guess(&guess, answer);
        answers = filter_answers(&guess, score, &answers);
        println!(
            "{}: {:?}, {} answers remain",
            guess.into_iter().collect::<String>(),
            &score,
            answers.len()
        );
        num_rounds += 1;
    }

    return num_rounds;
}

pub fn find_best_guess(answers: &HashSet<[char; 5]>, guesses: &HashSet<[char; 5]>) -> [char; 5] {
    let mut best_guess_score = usize::MAX;
    let mut best_guess = ['x'; 5];

    // Early exit if only 2 or fewer possible answers
    // Randomly choose 1
    if answers.len() <= 2 {
        return answers.into_iter().nth(0).unwrap().clone();
    }

    for guess in guesses {
        let expected_answers = evaluate_guess(&guess, &answers);
        if expected_answers < best_guess_score {
            best_guess_score = expected_answers;
            best_guess = guess.clone();
            // Can't get better than 1, can return early
            if expected_answers == 1 {
                return best_guess;
            }
        }
    }

    return best_guess;
}

pub fn filter_answers(
    guess: &[char; 5],
    score: [LetterState; 5],
    answers: &HashSet<[char; 5]>,
) -> HashSet<[char; 5]> {
    // return filter_answers_hashset(guess, score, answers);
    return filter_answers_vec(guess, score, answers);
}

fn filter_green<'a>(
    guess: &[char; 5],
    score: [LetterState; 5],
    answers: &'a HashSet<[char; 5]>,
) -> Vec<&'a [char; 5]> {
    let mut filtered = Vec::with_capacity(answers.len());
    // Filter letters where they should be, e.g. Green
    for answer in answers {
        let mut is_match = true;
        for i in 0..5 {
            if score[i] == LetterState::Green {
                let g = guess[i];
                let a = answer[i];
                is_match = is_match && a == g
            }
        }

        if is_match {
            filtered.push(answer);
        }
    }

    let mut new_filtered = Vec::with_capacity(filtered.len());

    // Filter letters where they shouldn't be, e.g. Gray and not Green
    for answer in filtered {
        let mut is_match = true;
        for i in 0..5 {
            if score[i] == LetterState::Yellow {
                let g = guess[i];
                let a = answer[i];
                is_match = is_match && a != g
            }
        }

        if is_match {
            new_filtered.push(answer);
        }
    }

    return new_filtered;
}

fn filter_answers_vec(
    guess: &[char; 5],
    score: [LetterState; 5],
    answers: &HashSet<[char; 5]>,
) -> HashSet<[char; 5]> {
    let filtered = filter_green(guess, score, answers);
    let mut new_filtered = Vec::with_capacity(filtered.len());

    // Filter by char counts
    let mut known_char_counts = [0; 26];
    let mut is_absent = [false; 26];
    for i in 0..5 {
        let g = guess[i];
        let index = get_index(g);
        match score[i] {
            LetterState::Yellow | LetterState::Green => known_char_counts[index] += 1,
            LetterState::Gray => is_absent[index] = true,
        }
    }

    for answer in filtered {
        let mut char_count = [0; 26];
        for &a in answer {
            increment_count(a, &mut char_count);
        }

        let mut is_match = true;
        for i in 0..26 {
            is_match = is_match
                && !((char_count[i] < known_char_counts[i])
                // Need to check for known_char_counts being zero to handle double letters
                || (char_count[i] > 0 && is_absent[i] && known_char_counts[i] == 0));

            if !is_match {
                break;
            }
        }

        if is_match {
            new_filtered.push(answer)
        }
    }

    return HashSet::from_iter(new_filtered.into_iter().map(|s| s.clone()));
}

fn filter_answers_hashset(
    guess: &str,
    score: [LetterState; 5],
    answers: &HashSet<[char; 5]>,
) -> HashSet<[char; 5]> {
    // Start with a copy of all answers and remove ones that can't match
    let mut filtered = answers.clone();

    // Filter letters where they should be, e.g. Green
    for i in 0..5 {
        if score[i] == LetterState::Green {
            let g = guess.chars().nth(i).unwrap();
            for answer in answers {
                if answer[i] != g {
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
                if answer[i] == g {
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
        for &a in answer {
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
pub fn get_all_scores() -> Vec<[LetterState; 5]> {
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
pub fn evaluate_guess(guess: &[char; 5], answers: &HashSet<[char; 5]>) -> usize {
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

pub fn load_word_list(path: &str, set: &mut HashSet<[char; 5]>) {
    if let Ok(lines) = read_lines(path) {
        for line in lines {
            if let Ok(word) = line {
                let mut chars = ['a'; 5];
                let mut i = 0;
                for c in word.chars() {
                    chars[i] = c;
                    i += 1;
                }
                set.insert(chars);
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

    /// Returns char array from str
    fn to_chars(s: &str) -> [char; 5] {
        let mut chars = ['a'; 5];
        for i in 0..5 {
            chars[i] = s.chars().nth(i).unwrap();
        }

        return chars;
    }

    #[test]
    fn test_score_correct() {
        let score = score_guess(&to_chars("crane"), &to_chars("crane"));
        assert_eq!(score, [LetterState::Green; 5]);

        let score = score_guess(&to_chars("foods"), &to_chars("foods"));
        assert_eq!(score, [LetterState::Green; 5]);
    }

    #[test]
    fn test_score_double_letter() {
        let score = score_guess(&to_chars("foods"), &to_chars("nodes"));
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
        let answers = HashSet::from_iter(vec![to_chars("cxxxx"), to_chars("bxxxx")]);
        let filtered = filter_answers(
            &to_chars("crane"),
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
            to_chars("cxxxe"), // Not included, otherwise C would be green
            to_chars("xcxxe"), // answer
            to_chars("xxaxe"), // Not included, A not in answer
            to_chars("xaxxe"), // Not included, A not in answer
            to_chars("xxxxe"), // Not included, no c
        ]);
        let filtered = filter_answers(
            &to_chars("crane"),
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
            to_chars("warby"), // no
            to_chars("wordy"), // yes
            to_chars("wormy"), // yes
            to_chars("wryly"), // yes
        ]);
        let filtered = filter_answers(
            &to_chars("weary"),
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
        let answers = HashSet::from_iter(vec![
            to_chars("robin"),
            to_chars("roomy"),
            to_chars("rowdy"),
            to_chars("round"),
            to_chars("rocky"),
            to_chars("rough"),
        ]);
        let filtered = filter_answers(
            &to_chars("digit"),
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
