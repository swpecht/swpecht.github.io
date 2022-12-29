use rand::{rngs::ThreadRng, seq::SliceRandom, thread_rng};

use crate::game::{Action, GameState};

pub trait Agent {
    fn step(&mut self, s: &dyn GameState) -> Action;
    fn get_name(&self) -> String {
        return format!("{}", std::any::type_name::<Self>());
    }
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

/// Agent plays the actions in the order provided and then starts from beginning
pub struct RecordedAgent {
    actions: Vec<Action>,
    cur_action: usize,
}

impl RecordedAgent {
    pub fn new(actions: Vec<Action>) -> Self {
        return RecordedAgent {
            actions,
            cur_action: 0,
        };
    }
}

impl Agent for RecordedAgent {
    fn step(&mut self, _: &dyn GameState) -> Action {
        let a = self.actions[self.cur_action];
        self.cur_action = (self.cur_action + 1) % self.actions.len();
        return a;
    }
}
