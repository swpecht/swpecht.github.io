use itertools::Itertools;
use rand::prelude::*;

use crate::gamestate::Action;

const MAX_NUM_SUCCESS: usize = 12;

/// Probabilities for chance outcomes
#[derive(Default)]
pub struct ChanceProbabilities {
    probs: [f32; MAX_NUM_SUCCESS + 1],
}

impl ChanceProbabilities {
    pub fn sample<T: Rng>(&self, rng: &mut T) -> Action {
        Action::RollResult {
            num_success: *(0..MAX_NUM_SUCCESS + 1)
                .collect_vec()
                .choose_weighted(rng, |i| &self.probs[*i])
                .unwrap() as u8,
        }
    }

    /// Returns the probability of a given chance result
    pub fn prob(&self, num_success: u8) -> f32 {
        self.probs[num_success as usize]
    }

    pub fn to_vec(&self) -> Vec<f32> {
        self.probs.to_vec()
    }
}

/// Returns a vector of length num_attacks with the probability for that many
/// successful wounds
pub(super) fn attack_success_probs(
    num_attacks: u8,
    attack_skill: u8,
    attack_strength: u8,
    target_toughness: u8,
    attack_ap: u8,
    target_save: u8,
) -> ChanceProbabilities {
    if num_attacks as usize > MAX_NUM_SUCCESS {
        panic!("attempting to calculate probabilities on too many attacks")
    }

    let mut probs = ChanceProbabilities::default();

    // hit roll, d6 greater than ballistic skill
    // unless a 6 then always passes
    // 1 always fails
    let hit_chance = p_d6(attack_skill).clamp(1.0 / 6.0, 5.0 / 6.0);
    // wound roll
    // attack's strength vs target toughness implies what's needed
    // unless 6, always passes
    // 1 always fails

    let target = if attack_strength >= target_toughness * 2 {
        2
    } else if attack_strength > target_toughness {
        3
    } else if attack_strength == target_toughness {
        4
    } else if attack_strength * 2 <= target_toughness {
        6
    } else {
        // strength < toughness
        5
    };
    let wound_chance = p_d6(target).clamp(1.0 / 6.0, 5.0 / 6.0);

    // saving throw -- this is where the attack allocation matters for future
    // d6 - AP >= Sv
    // rolls of 1 always fails
    let save_fail_chance = (1.0 - p_d6(target_save + attack_ap)).clamp(1.0 / 6.0, 5.0 / 6.0);

    for i in 0..num_attacks + 1 {
        // at least a 1/6 chance for both success and failure with nat 1 and 6 rolls

        probs.probs[i as usize] =
            prob_num_success(num_attacks, i, hit_chance * wound_chance * save_fail_chance);
    }

    probs
}

/// Results of 2d6
pub(super) fn charge_success_probs() -> ChanceProbabilities {
    // https://www.thedarkfortress.co.uk/tech_reports/2_dice_rolls.php
    ChanceProbabilities {
        probs: [
            0.0, 0.0, 0.0277, 0.0555, 0.0833, 0.1111, 0.1388, 0.1666, 0.1388, 0.1111, 0.0833,
            0.0555, 0.0277,
        ],
    }
}

/// Returns the probability a d6 rolls greater than or equal to x
fn p_d6(x: u8) -> f32 {
    (6.0 - x as f32 + 1.0) / 6.0
}

fn prob_num_success(n: u8, k: u8, p: f32) -> f32 {
    n_choose_k(n, k) as f32 * p.powi(k as i32) * (1.0 - p).powi((n - k) as i32)
}

fn n_choose_k(n: u8, k: u8) -> usize {
    factorial(n) / (factorial(k) * factorial(n - k))
}

fn factorial(x: u8) -> usize {
    let mut r = 1;
    for i in 2..x + 1 {
        r *= i as usize
    }
    r
}

#[cfg(test)]
mod tests {

    use core::{assert, assert_eq};

    use super::*;

    #[test]
    fn test_p_d6() {
        assert_eq!(p_d6(1), 1.0);
        assert_eq!(p_d6(2), 5.0 / 6.0);
        assert_eq!(p_d6(3), 4.0 / 6.0);
        assert_eq!(p_d6(4), 3.0 / 6.0);
        assert_eq!(p_d6(5), 2.0 / 6.0);
        assert_eq!(p_d6(6), 1.0 / 6.0);
    }

    #[test]
    fn test_factorial() {
        assert_eq!(factorial(10), 3628800);
        assert_eq!(factorial(5), 120);
        assert_eq!(factorial(1), 1);
        assert_eq!(factorial(0), 1);
    }

    #[test]
    fn test_n_choose_k() {
        assert_eq!(n_choose_k(10, 5), 252);
        assert_eq!(n_choose_k(10, 1), 10);
        assert_eq!(n_choose_k(5, 4), 5);
    }

    #[test]
    fn test_attack_success_probs() {
        // Boltgun attack against a necron warrior
        // Hit probability: 4/6
        // wound: 3/6: 50%
        // saving throw: 3/6: 50%
        // overall 1 / 6 chance to successfully damage
        let probs = attack_success_probs(1, 3, 4, 4, 0, 4);
        assert_eq!(probs.prob(0), 5.0 / 6.0);
        assert_eq!(probs.prob(1), 1.0 / 6.0);

        let probs = attack_success_probs(5, 3, 4, 4, 0, 4);
        for (l, r) in probs.to_vec().iter().zip(vec![
            0.40187752,
            0.40187755,
            0.16075101,
            0.03215021,
            0.003215021,
            0.00012860085,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
        ]) {
            assert!((l - r).abs() < 0.01);
        }

        // check no over flows
        attack_success_probs(10, 3, 4, 4, 0, 4);
    }
}
