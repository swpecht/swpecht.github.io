use rand::{rngs::ThreadRng, seq::SliceRandom, thread_rng};

use crate::game::{Action, GameState};

pub trait Agent {
    fn step(&mut self, s: &dyn GameState) -> Action;
}

pub struct RandomAgent {
    pub rng: ThreadRng,
}

impl RandomAgent {
    pub fn new() -> Self {
        Self { rng: thread_rng() }
    }
}

impl Agent for RandomAgent {
    fn step(&mut self, s: &dyn GameState) -> Action {
        return *s.legal_actions().choose(&mut self.rng).unwrap();
    }
}

pub struct AlwaysFirstAgent {}

impl AlwaysFirstAgent {
    pub fn new() -> Self {
        Self {}
    }
}

impl Agent for AlwaysFirstAgent {
    fn step(&mut self, s: &dyn GameState) -> Action {
        return s.legal_actions()[0];
    }
}
