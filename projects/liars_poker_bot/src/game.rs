use std::fmt::Debug;

use itertools::Itertools;
use log::{debug, info};

use crate::{agents::Agent, liars_poker::Player};

pub trait GameState: Sized {
    fn new() -> Self;
    fn evaluate(&self) -> i32;
    fn get_acting_player(&self) -> Player;
    /// Return true is the gamestate is terminal
    fn is_terminal(&self) -> bool;
    /// Return the gamestate with only the information a given player can see
    fn get_filtered_state(&self, player: Player) -> Self;
    /// Return all poassible game states given hidden information
    fn get_possible_states(&self) -> Vec<Self>;
    /// Returns all possible children gamestates
    fn get_children(&self) -> Vec<Self>;
}

#[derive(Clone, Copy, PartialEq, Debug, Eq)]
pub enum RPSAction {
    Rock,
    Paper,
    Scissors,
}

/// Implementation of weighted RPS. Any game involving scissors means the payoff is doubled
///
/// https://arxiv.org/pdf/2007.13544.pdf
#[derive(Clone, Copy, PartialEq, Debug, Eq)]
pub struct RPSState {
    actions: [Option<RPSAction>; 2],
}

impl GameState for RPSState {
    fn evaluate(&self) -> i32 {
        return match (self.actions[0], self.actions[1]) {
            (Some(x), Some(y)) if x == y => 0,
            (Some(RPSAction::Paper), Some(RPSAction::Rock)) => 1,
            (Some(RPSAction::Paper), Some(RPSAction::Scissors)) => -2,
            (Some(RPSAction::Rock), Some(RPSAction::Scissors)) => 2,
            (Some(RPSAction::Rock), Some(RPSAction::Paper)) => -1,
            (Some(RPSAction::Scissors), Some(RPSAction::Paper)) => 2,
            (Some(RPSAction::Scissors), Some(RPSAction::Rock)) => -2,
            _ => 0,
        };
    }

    fn get_acting_player(&self) -> Player {
        match self.actions {
            [None, _] => Player::P1,
            _ => Player::P2,
        }
    }

    fn get_possible_states(&self) -> Vec<Self> {
        let mut possibilities = Vec::new();

        for i in 0..self.actions.len() {
            possibilities.push(match self.actions[i] {
                None => vec![
                    Some(RPSAction::Rock),
                    Some(RPSAction::Paper),
                    Some(RPSAction::Scissors),
                ],
                _ => vec![self.actions[i]],
            });
        }

        let actions = possibilities.iter().multi_cartesian_product();
        let mut states = Vec::new();
        for a in actions {
            let mut s = self.clone();
            for i in 0..s.actions.len() {
                s.actions[i] = *a[i];
            }
            states.push(s);
        }

        return states;
    }

    fn get_filtered_state(&self, player: Player) -> Self {
        let mut filtered_state = self.clone();
        match player {
            Player::P1 => filtered_state.actions[1] = None,
            Player::P2 => filtered_state.actions[0] = None,
        }
        return filtered_state;
    }

    fn new() -> Self {
        return Self { actions: [None; 2] };
    }

    fn is_terminal(&self) -> bool {
        match self.actions {
            [Some(_), Some(_)] => true,
            _ => false,
        }
    }

    fn get_children(&self) -> Vec<Self> {
        let actions = vec![RPSAction::Rock, RPSAction::Paper, RPSAction::Scissors];
        let mut possibilities = Vec::new();

        if let Some(_) = self.actions[1] {
            return possibilities;
        }

        for a in actions {
            let mut g = self.clone();
            match g.actions[0] {
                None => g.actions[0] = Some(a),
                _ => g.actions[1] = Some(a),
            }
            possibilities.push(g);
        }

        return possibilities;
    }
}

impl RPSState {
    pub fn get_last_action(&self) -> Option<RPSAction> {
        return match self.actions[1] {
            None => self.actions[0],
            _ => self.actions[1],
        };
    }
}

pub fn play<G>(g: &mut G, p1: &mut dyn Agent<G>, p2: &mut dyn Agent<G>) -> i32
where
    G: GameState + Debug + Eq,
{
    info!("{} playing {}", p1.name(), p2.name());
    let mut is_player1_turn = true;
    while !g.is_terminal() {
        match is_player1_turn {
            true => step(g, p1, Player::P1),
            false => step(g, p2, Player::P2),
        }

        is_player1_turn = !is_player1_turn;
        debug!("Game state: {:?}", g);
    }

    let score = g.evaluate();
    match score {
        x if x > 0 => info!("P1 wins"),
        x if x < 0 => info!("P2 wins"),
        0 => info!("tie"),
        _ => panic!("invalid winner"),
    };

    return score;
}

fn step<G: Eq>(g: &mut G, agent: &mut (impl Agent<G> + ?Sized), p: Player)
where
    G: GameState + Debug,
{
    let acting_player = p;

    let filtered_state = g.get_filtered_state(acting_player);
    let possible_children = filtered_state.get_children();
    debug!("{} sees game state of {:?}", agent.name(), filtered_state);
    debug!(
        "{} evaluating future states: {:?}",
        agent.name(),
        possible_children
    );
    let new_g = agent.play(&filtered_state, &possible_children);

    info!("{:?} tried to play {:?}", acting_player, new_g);

    // Verify the action is allowed
    if !possible_children.contains(&new_g) {
        panic!("Agent attempted invalid action")
    }

    *g = new_g;
}
