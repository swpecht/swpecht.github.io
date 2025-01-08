use rand::rngs::StdRng;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChanceResult {
    Pass,
    Fail,
}

/// Probabilities for chance outcomes
pub struct ChanceProbabilities {}

impl ChanceProbabilities {
    pub fn sample(&self, rng: StdRng) -> Self {
        todo!()
    }

    /// Returns the probability of a given chance result
    pub fn prob(&self, result: ChanceResult) -> f32 {
        todo!()
    }
}
