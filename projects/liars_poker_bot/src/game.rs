use std::fmt::Debug;

use itertools::Itertools;
use log::{debug, info};

use crate::{agents::Agent, liars_poker::Player};

pub trait GameState: Sized {
    type Action: Clone;
    fn new() -> Self;
    fn get_actions(&self) -> Vec<Self::Action>;
    fn apply(&mut self, p: Player, a: &Self::Action);
    fn evaluate(&self) -> i32;
    fn get_acting_player(&self) -> Player;
    /// Return true is the gamestate is terminal
    fn is_terminal(&self) -> bool;
    /// Return the gamestate with only the information a given player can see
    fn get_filtered_state(&self, player: Player) -> Self;
    /// Return all poassible game states given hidden information
    fn get_possible_states(&self) -> Vec<Self>;
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RPSAction {
    Rock,
    Paper,
    Scissors,
}

/// Implementation of weighted RPS. Any game involving scissors means the payoff is doubled
///
/// https://arxiv.org/pdf/2007.13544.pdf
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RPSState {
    actions: [Option<RPSAction>; 2],
}

impl GameState for RPSState {
    type Action = RPSAction;

    fn get_actions(&self) -> Vec<Self::Action> {
        return match self.actions {
            [Some(_), Some(_)] => Vec::new(),
            _ => vec![RPSAction::Rock, RPSAction::Paper, RPSAction::Scissors],
        };
    }

    fn apply(&mut self, p: Player, a: &Self::Action) {
        match p {
            Player::P1 => self.actions[0] = Some(*a),
            Player::P2 => self.actions[1] = Some(*a),
        }
    }

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
}

pub fn play<G>(p1: &(impl Agent<G> + ?Sized), p2: &(impl Agent<G> + ?Sized)) -> i32
where
    G: GameState + Debug,
    G::Action: Debug + PartialEq,
{
    let mut g = G::new();
    info!("{} playing {}", p1.name(), p2.name());
    let mut is_player1_turn = true;
    while !g.is_terminal() {
        match is_player1_turn {
            true => step(&mut g, p1, Player::P1),
            false => step(&mut g, p2, Player::P2),
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

fn step<G>(g: &mut G, agent: &(impl Agent<G> + ?Sized), p: Player)
where
    G: GameState + Debug,
    G::Action: Debug + PartialEq,
{
    let acting_player = p;

    let filtered_state = g.get_filtered_state(acting_player);
    let possible_actions = filtered_state.get_actions();
    debug!("{} sees game state of {:?}", agent.name(), filtered_state);
    debug!("{} evaluating moves: {:?}", agent.name(), possible_actions);
    let a = agent.play(&filtered_state, &possible_actions);

    info!("{:?} tried to play {:?}", acting_player, a);

    // Verify the action is allowed
    if !possible_actions.contains(&a) {
        panic!("Agent attempted invalid action")
    }

    g.apply(p, &a);
}
