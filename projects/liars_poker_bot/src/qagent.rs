use crate::agents::Agent;

struct QAgent {}

impl QAgent {
    pub fn new(seed: &str) {}
}

impl Agent for QAgent {
    fn step(&mut self, s: &dyn crate::game::GameState) -> crate::game::Action {
        todo!()
    }
}
