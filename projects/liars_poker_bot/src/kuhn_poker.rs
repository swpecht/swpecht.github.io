use std::fmt::Display;

use crate::game::{Action, GameState, IState, Player};
use log::info;

#[derive(Debug)]
pub enum KPAction {
    Bet,
    Pass,
}

#[derive(Debug)]
pub enum KPPhase {
    Dealing,
    Playing,
}

/// Adapted from: https://github.com/deepmind/open_spiel/blob/master/open_spiel/games/kuhn_poker.cc
/// All of the randomness occurs outside of the gamestate. Instead some game states are change nodes. And the
/// "Game runner" will choose of of the random, valid actions
#[derive(Debug)]
pub struct KPGameState {
    num_players: usize,
    /// Holds the cards for each player in the game
    hands: Vec<i64>,
    is_chance_node: bool,
    is_terminal: bool,
    phase: KPPhase,
    cur_player: usize,
    history: Vec<Action>,
}

impl Display for KPGameState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut result = String::new();
        result.push_str("[");
        for c in &self.hands {
            result.push_str(&format!("{}", c));
        }
        result.push_str("]");

        for &h in &self.history {
            let char = match h {
                x if x == KPAction::Bet as Action => 'b',
                x if x == KPAction::Pass as Action => 'p',
                _ => panic!("invalid history"),
            };
            result.push(char)
        }

        write!(f, "{}", result)
    }
}

pub struct KuhnPoker {}
impl KuhnPoker {
    pub fn new() -> KPGameState {
        KPGameState {
            hands: Vec::new(),
            phase: KPPhase::Dealing,
            cur_player: 0,
            num_players: 2,
            is_chance_node: true,
            history: Vec::new(),
            is_terminal: false,
        }
    }
}

impl KPGameState {
    fn apply_action_dealing(&mut self, card: Action) {
        info!("player {} dealt card {}", self.cur_player, card);
        self.hands.push(card);
        self.cur_player += 1;

        if self.cur_player >= self.num_players {
            info!("moving to playing phase");
            self.phase = KPPhase::Playing;
            self.cur_player = 0;
            self.is_chance_node = false;
        }
    }

    fn apply_action_playing(&mut self, a: Action) {
        match a {
            0 | 1 => self.history.push(a), // only bet or pass allowed
            _ => panic!("attempted invalid action"),
        }

        if (self.history.len() == self.num_players && self.history[0] == KPAction::Bet as Action)
            || (self.history.len() == self.num_players
                && self.history[0] == KPAction::Pass as Action
                && self.history[1] == KPAction::Pass as Action)
            || self.history.len() == 3
        {
            self.is_terminal = true;
        }

        self.cur_player += 1;
        self.cur_player = self.cur_player % self.num_players;
    }

    fn get_dealing_actions(&self, actions: &mut Vec<Action>) {
        for i in 0..self.num_players + 1 {
            let card = i as Action;
            if self.hands.contains(&card) {
                // Don't return cards already dealt
                continue;
            }
            actions.push(card);
        }
    }

    fn get_betting_actions(&self, actions: &mut Vec<Action>) {
        actions.push(KPAction::Bet as Action);
        actions.push(KPAction::Pass as Action);
    }
}

impl GameState for KPGameState {
    fn legal_actions(&self) -> Vec<Action> {
        let mut actions = Vec::new();

        if self.is_terminal {
            return actions;
        }

        match self.phase {
            KPPhase::Dealing => self.get_dealing_actions(&mut actions),
            KPPhase::Playing => self.get_betting_actions(&mut actions),
        }

        return actions;
    }

    fn apply_action(&mut self, a: Action) {
        match self.phase {
            KPPhase::Dealing => self.apply_action_dealing(a),
            KPPhase::Playing => self.apply_action_playing(a),
        }
    }

    /// Returns a vector of the score for each player
    /// at the end of the game
    fn evaluate(&self) -> Vec<f32> {
        if !self.is_terminal {
            return vec![0.0; self.num_players]; // No one gets points
        }

        // get pot
        let mut pot = self.num_players; // capture the antes
        for &h in &self.history {
            if h == KPAction::Bet as Action {
                pot += 1;
            }
        }

        if self.num_players != 2 {
            panic!("game logic only implemented for 2 players")
        }

        let s: Vec<String> = self
            .history
            .iter()
            .map(|&x| {
                format!(
                    "{}",
                    match x {
                        a if a == KPAction::Bet as Action => "b",
                        a if a == KPAction::Pass as Action => "p",
                        _ => panic!("invalid game state"),
                    }
                )
            })
            .collect();
        let s = s.join("");

        let winner = match s.as_str() {
            "pp" | "bb" | "pbb" => {
                if self.hands[0] > self.hands[1] {
                    0
                } else {
                    1
                }
            }
            "pbp" => 1,
            "bp" => 0,
            _ => panic!("invalid game state"),
        };

        // The winnder gets the whole pot, everyone else gets nothing
        let mut payoffs = vec![0.0; self.num_players];
        payoffs[winner] = pot as f32;

        return payoffs;
    }

    /// Returns an information state with the following data at each index:
    /// 0: Card dealt
    /// 1+: History of play
    fn information_state(&self, player: Player) -> Vec<IState> {
        let mut i_state = Vec::new();
        i_state.push(self.hands[player] as IState);

        for &h in &self.history {
            i_state.push(h as IState);
        }
        return i_state;
    }

    fn is_terminal(&self) -> bool {
        self.is_terminal
    }

    fn is_chance_node(&self) -> bool {
        self.is_chance_node
    }

    fn num_players(&self) -> usize {
        self.num_players
    }

    fn cur_player(&self) -> usize {
        self.cur_player
    }
}

#[cfg(test)]
mod tests {
    use crate::{agents::RandomAgent, game::run_game, kuhn_poker::KuhnPoker};
    use rand::thread_rng;

    #[test]
    fn kuhn_poker_test() {
        let mut g = KuhnPoker::new();
        let mut a1 = RandomAgent { rng: thread_rng() };
        let mut a2 = RandomAgent { rng: thread_rng() };

        run_game(&mut g, &mut vec![&mut a1, &mut a2]);
        todo!("not implemented");
    }
}
