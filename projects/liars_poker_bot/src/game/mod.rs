use std::fmt::{Debug, Display};

pub mod bluff;
pub mod euchre;
pub mod kuhn_poker;
pub mod updownriver;

use log::trace;
use rand::{seq::SliceRandom, Rng};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{agents::Agent, istate::IStateKey};

// pub type Action = usize;
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord, Default)]
pub struct Action(pub u8);

impl From<Action> for u8 {
    fn from(value: Action) -> Self {
        value.0
    }
}

impl Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Debug for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
pub type IState = f64;
pub type Player = usize;

#[derive(Clone)]
pub struct Game<T: GameState> {
    pub new: Box<fn() -> T>,
    pub max_players: usize,
    pub max_actions: usize,
}

pub trait GameState: Display + Clone + Debug + Serialize + DeserializeOwned {
    /// Applies an action in place
    fn apply_action(&mut self, a: Action);
    /// Returns all legal actions at a given game state
    fn legal_actions(&self, actions: &mut Vec<Action>);
    /// Returns a vector of the score for each player
    /// at the end of the game
    fn evaluate(&self, p: Player) -> f64;
    fn istate_key(&self, player: Player) -> IStateKey;
    fn istate_string(&self, player: Player) -> String;
    fn is_terminal(&self) -> bool;
    fn is_chance_node(&self) -> bool;
    fn num_players(&self) -> usize;
    fn cur_player(&self) -> Player;
    // A key representing the entire game state, likely a history of all actions
    fn key(&self) -> IStateKey;
}

/// Runs an iteration of the game. If Team 0 is none, the agent plays against itself
pub fn run_game<G, R>(
    s: &mut G,
    team0: &mut dyn Agent<G>,
    team1: &mut Option<&mut dyn Agent<G>>,
    rng: &mut R,
) -> Vec<f64>
where
    R: Rng + ?Sized,
    G: GameState,
{
    let mut actions = Vec::new();

    while !s.is_terminal() {
        trace!("game state: {}", s);

        if s.is_chance_node() {
            s.legal_actions(&mut actions);
            let a = *actions.choose(rng).unwrap();
            trace!("chance action: {}", a);
            s.apply_action(a);
        } else {
            let a = if team1.is_none() {
                team0.step(s)
            } else {
                match s.cur_player() % 2 {
                    0 => team0.step(s),
                    1 => team1.as_mut().unwrap().step(s),
                    _ => todo!(),
                }
            };

            trace!("player {} taking action {}", s.cur_player(), a);
            s.apply_action(a);
        }
    }

    let mut returns = Vec::new();
    for p in 0..s.num_players() {
        returns.push(s.evaluate(p));
    }

    trace!("game state: {}", s);
    trace!("game over, rewards: {}, {}", s.evaluate(0), s.evaluate(1));
    returns
}

#[macro_export]
macro_rules! actions {
    ( $x:expr ) => {{
        let mut temp_vec = Vec::new();
        $x.legal_actions(&mut temp_vec);
        temp_vec
    }};
}
