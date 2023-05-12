use rand::{rngs::ThreadRng, seq::SliceRandom, thread_rng};

use crate::game::{Action, GameState};

pub trait Agent<T: GameState> {
    fn step(&mut self, s: &T) -> Action;
    fn get_name(&self) -> String {
        std::any::type_name::<Self>().to_string()
    }
}

pub struct RandomAgent {
    pub rng: ThreadRng,
}

impl Default for RandomAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl RandomAgent {
    pub fn new() -> Self {
        Self { rng: thread_rng() }
    }
}

impl<T: GameState> Agent<T> for RandomAgent {
    fn step(&mut self, s: &T) -> Action {
        let mut actions = Vec::new();
        s.legal_actions(&mut actions);
        return *actions.choose(&mut self.rng).unwrap();
    }

    fn get_name(&self) -> String {
        "RandomAgent".to_string()
    }
}

pub struct AlwaysFirstAgent {}

impl Default for AlwaysFirstAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl AlwaysFirstAgent {
    pub fn new() -> Self {
        Self {}
    }
}

impl<T: GameState> Agent<T> for AlwaysFirstAgent {
    fn step(&mut self, s: &T) -> Action {
        let mut actions = Vec::new();
        s.legal_actions(&mut actions);
        actions[0]
    }
}

/// Agent plays the actions in the order provided and then starts from beginning
pub struct RecordedAgent {
    actions: Vec<Action>,
    cur_action: usize,
}

impl RecordedAgent {
    pub fn new(actions: Vec<Action>) -> Self {
        RecordedAgent {
            actions,
            cur_action: 0,
        }
    }
}

impl<T: GameState> Agent<T> for RecordedAgent {
    fn step(&mut self, _: &T) -> Action {
        let a = self.actions[self.cur_action];
        self.cur_action = (self.cur_action + 1) % self.actions.len();
        a
    }
}
