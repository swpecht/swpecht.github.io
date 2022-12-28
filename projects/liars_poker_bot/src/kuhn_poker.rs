use std::fmt::Display;

use crate::game::{Action, GameState, IState, Player};
use log::info;

#[derive(Debug)]
pub enum KPAction {
    Bet,
    Pass,
}

#[derive(Debug, Clone)]
pub enum KPPhase {
    Dealing,
    Playing,
}

/// Adapted from: https://github.com/deepmind/open_spiel/blob/master/open_spiel/games/kuhn_poker.cc
/// All of the randomness occurs outside of the gamestate. Instead some game states are change nodes. And the
/// "Game runner" will choose of of the random, valid actions
#[derive(Debug, Clone)]
pub struct KPGameState {
    num_players: usize,
    /// Holds the cards for each player in the game
    hands: Vec<Action>,
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

    /// Max possible actions are the cards being dealt
    pub fn max_actions() -> usize {
        return 3;
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

        let p0_bets = self
            .history
            .iter()
            .map(|&x| (x == KPAction::Bet as Action))
            .enumerate()
            .filter(|(i, is_bet)| *is_bet && *i % 2 == 0)
            .count() as f32;

        let mut total_bets = 0.0;
        for &h in &self.history {
            if h == KPAction::Bet as Action {
                total_bets += 1.0;
            }
        }

        let p1_bets = total_bets - p0_bets;

        // The winnder gets the whole pot, everyone else gets nothing
        let mut payoffs = vec![0.0; self.num_players];

        match winner {
            0 => {
                payoffs[1] = -1.0 * p1_bets - 1.0;
                payoffs[0] = p1_bets + 1.0;
            }
            1 => {
                payoffs[0] = -1.0 * p0_bets - 1.0;
                payoffs[1] = p0_bets + 1.0;
            }
            _ => panic!("only supports 2 players"),
        }

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

    fn information_state_string(&self, player: Player) -> String {
        let istate = self.information_state(player);
        let mut result = String::new();
        result.push_str(format!("{}", istate[0]).as_str());

        for i in 1..istate.len() {
            let char = match istate[i] {
                x if x == KPAction::Bet as i64 as f64 => 'b',
                x if x == KPAction::Pass as i64 as f64 => 'p',
                _ => panic!("invalid history"),
            };
            result.push(char)
        }

        return result;
    }
}

#[cfg(test)]
mod tests {
    use std::vec;

    use crate::{
        agents::RecordedAgent,
        game::{run_game, Action, GameState},
        kuhn_poker::{KPAction, KuhnPoker},
    };
    use rand_pcg::Pcg64;
    use rand_seeder::Seeder;

    #[test]
    fn kuhn_poker_test_bb() {
        let mut g = KuhnPoker::new();
        let mut rng: Pcg64 = Seeder::from("test").make_rng();
        let mut a1 = RecordedAgent::new(vec![KPAction::Bet as Action; 1]);
        let mut a2 = RecordedAgent::new(vec![KPAction::Bet as Action; 1]);

        run_game(&mut g, &mut vec![&mut a1, &mut a2], &mut rng);

        assert_eq!(format!("{}", g), "[21]bb");
        assert_eq!(g.evaluate(), vec![2.0, -2.0])
    }

    #[test]
    fn kuhn_poker_test_pbp() {
        let mut g = KuhnPoker::new();
        let mut rng: Pcg64 = Seeder::from("test").make_rng();
        let mut a1 = RecordedAgent::new(vec![KPAction::Pass as Action; 2]);
        let mut a2 = RecordedAgent::new(vec![KPAction::Bet as Action; 1]);

        run_game(&mut g, &mut vec![&mut a1, &mut a2], &mut rng);

        assert_eq!(format!("{}", g), "[21]pbp");
        assert_eq!(g.evaluate(), vec![-1.0, 1.0])
    }
}
