use std::{
    collections::hash_map::DefaultHasher,
    fmt::{Debug, Display},
    hash::{Hash, Hasher},
};

pub mod cards;
pub mod gamestates;
pub mod istate;
pub mod iterator;
pub mod pool;
pub mod resample;

use rand::{rngs::StdRng, seq::SliceRandom};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::istate::{IStateKey, IsomorphicHash};

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

pub trait GameState: Display + Clone + Debug + Serialize + DeserializeOwned + Hash + Send {
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
    /// A key representing the entire game state, likely a history of all actions
    fn key(&self) -> IStateKey;
    /// Returns an isomorphic hash of the current gamestate
    fn transposition_table_hash(&self) -> Option<IsomorphicHash> {
        let mut hasher = DefaultHasher::default();
        self.hash(&mut hasher);
        Some(hasher.finish())
    }
    /// Undo the last played actions
    fn undo(&mut self);
}

pub fn get_games<T: GameState>(game: Game<T>, n: usize, rng: &mut StdRng) -> Vec<T> {
    let mut games = Vec::with_capacity(n);
    let mut actions = Vec::new();

    for _ in 0..n {
        let mut gs = (game.new)();
        while gs.is_chance_node() {
            gs.legal_actions(&mut actions);
            let a = actions.choose(rng).unwrap();
            gs.apply_action(*a);
            actions.clear();
        }

        games.push(gs);
    }
    games
}

#[derive(PartialEq, Eq, Clone, Copy, Hash)]
pub enum Team {
    Team1,
    Team2,
}

impl From<Team> for Player {
    fn from(val: Team) -> Self {
        val as usize
    }
}

impl From<Player> for Team {
    fn from(val: Player) -> Self {
        if val % 2 == 0 {
            Team::Team1
        } else {
            Team::Team2
        }
    }
}

#[macro_export]
macro_rules! actions {
    ( $x:expr ) => {{
        let mut temp_vec = Vec::new();
        $x.legal_actions(&mut temp_vec);
        temp_vec
    }};
}

#[cfg(test)]
mod tests {
    use itertools::Itertools;
    use rand::{seq::SliceRandom, thread_rng};

    use crate::{
        gamestates::{
            bluff::Bluff,
            euchre::{actions::EAction, Euchre},
            kuhn_poker::KuhnPoker,
        },
        Game, GameState,
    };

    #[test]
    fn test_actions_sorted() {
        _test_actions_sorted(Euchre::game());
        _test_actions_sorted(Bluff::game(2, 2));
        _test_actions_sorted(KuhnPoker::game());
    }

    /// Helper function to ensure games always return actions in a sorted order.
    /// This is necessary to ensure agents are deterministic
    fn _test_actions_sorted<G: GameState>(game: Game<G>) {
        let mut rng = thread_rng();
        let mut actions = Vec::new();
        for _ in 0..100 {
            let mut gs = (game.new)();
            while !gs.is_terminal() {
                gs.legal_actions(&mut actions);
                let mut sorted_actions = actions.clone();
                sorted_actions.sort();
                assert_eq!(
                    actions,
                    sorted_actions,
                    "{:?} vs {:?}",
                    actions.clone().into_iter().map(EAction::from).collect_vec(),
                    sorted_actions
                        .clone()
                        .into_iter()
                        .map(EAction::from)
                        .collect_vec()
                );
                let a = actions.choose(&mut rng).unwrap();
                gs.apply_action(*a);
            }
        }
    }
}
