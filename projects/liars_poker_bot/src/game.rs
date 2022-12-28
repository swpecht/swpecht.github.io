use std::fmt::Display;

use log::info;
use rand::{seq::SliceRandom, Rng};

use crate::agents::Agent;

pub type Action = usize;
pub type IState = f64;
pub type Player = usize;

pub struct Game {
    // pub new: fn() -> Box<dyn GameState>,
    pub max_players: usize,
    pub max_actions: usize,
}

pub trait GameState: Display {
    /// Applies an action in place
    fn apply_action(&mut self, a: Action);
    /// Returns all legal actions at a given game state
    fn legal_actions(&self) -> Vec<Action>;
    /// Returns a vector of the score for each player
    /// at the end of the game
    fn evaluate(&self) -> Vec<f32>;
    fn information_state(&self, player: Player) -> Vec<IState>;
    fn information_state_string(&self, player: Player) -> String;
    fn is_terminal(&self) -> bool;
    fn is_chance_node(&self) -> bool;
    fn num_players(&self) -> usize;
    fn cur_player(&self) -> Player;
}

pub fn run_game<R>(s: &mut (dyn GameState), agents: &mut Vec<&mut dyn Agent>, rng: &mut R)
where
    R: Rng + ?Sized,
{
    if s.num_players() != agents.len() {
        panic!(
            "Number of players doesn't equal the number of agents, {} players and {} agents",
            s.num_players(),
            agents.len()
        );
    }

    while !s.is_terminal() {
        info!("game state: {}", s);

        if s.is_chance_node() {
            let actions = s.legal_actions();
            let a = *actions.choose(rng).unwrap();
            info!("chance action: {}", a);
            s.apply_action(a);
        } else {
            let agent = &mut agents[s.cur_player()];
            let a = agent.step(s);
            info!("player {} taking action {}", s.cur_player(), a);
            s.apply_action(a);
        }
    }

    info!("game state: {}", s);
    info!("game over, rewards: {:?}", s.evaluate());
}
