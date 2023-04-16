use std::fmt::Display;

use crate::{
    game::{Action, Game, GameState, Player},
    istate::IStateKey,
};
use log::info;

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
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
    history: Vec<KPAction>,
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
                KPAction::Bet => 'b',
                KPAction::Pass => 'p',
            };
            result.push(char)
        }

        write!(f, "{}", result)
    }
}

pub struct KuhnPoker {}
impl KuhnPoker {
    pub fn new_state() -> KPGameState {
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

    pub fn game() -> Game<KPGameState> {
        Game {
            new: Box::new(|| -> KPGameState { Self::new_state() }),
            max_players: 2,
            max_actions: 3, // 1 for each card dealt
        }
    }

    pub fn from_actions(actions: &[Action]) -> KPGameState {
        let mut g = (KuhnPoker::game().new)();
        for &a in actions {
            g.apply_action(a);
        }

        return g;
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
            x if x == KPAction::Bet as usize => self.history.push(KPAction::Bet),
            x if x == KPAction::Pass as usize => self.history.push(KPAction::Pass),
            _ => panic!("attempted invalid action"),
        }

        if (self.history.len() == self.num_players && self.history[0] == KPAction::Bet)
            || (self.history.len() == self.num_players
                && self.history[0] == KPAction::Pass
                && self.history[1] == KPAction::Pass)
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

    /// Get the payoff for the non-fixed player assuming the fixed players chance
    /// outcomes are replaced with the sepficied one
    pub fn get_payoff(&self, fixed_player: Player, chance_outcome: Action) -> f64 {
        let non_fixed = if fixed_player == 0 { 1 } else { 0 };
        let mut ngs = self.clone();
        ngs.hands[fixed_player] = chance_outcome;
        return ngs.evaluate()[non_fixed] as f64;
    }

    pub fn chance_outcomes(&self, fixed_player: Player) -> Vec<Action> {
        let nf = if fixed_player == 0 { 1 } else { 0 };

        if nf >= self.hands.len() {
            return vec![0, 1, 2]; // could be any card
        }

        return match self.hands[nf] {
            0 => vec![1, 2],
            1 => vec![0, 2],
            2 => vec![0, 1],
            _ => panic!("not implemented for other hands"),
        };
    }

    // returns the istate key for a given player with the chance outcomes replaced with the specified one
    pub fn co_istate(&self, player: Player, chance_outcome: Action) -> IStateKey {
        let mut ngs = self.clone();
        ngs.hands[player] = chance_outcome;
        return ngs.istate_key(player);
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

        if self.hands[0] == self.hands[1] {
            panic!("invalid deal, players have same cards")
        }

        let payoffs = match self.history[..] {
            [KPAction::Pass, KPAction::Pass] => {
                if self.hands[0] > self.hands[1] {
                    [1.0, -1.0]
                } else {
                    [-1.0, 1.0]
                }
            }
            [KPAction::Bet, KPAction::Bet] | [KPAction::Pass, KPAction::Bet, KPAction::Bet] => {
                if self.hands[0] > self.hands[1] {
                    [2.0, -2.0]
                } else {
                    [-2.0, 2.0]
                }
            }
            [KPAction::Pass, KPAction::Bet, KPAction::Pass] => [-1.0, 1.0],
            [KPAction::Bet, KPAction::Pass] => [1.0, -1.0],
            _ => panic!("invalid history"),
        };

        return payoffs.to_vec();
    }

    /// Returns an information state with the following data at each index:
    /// 0: Card dealt
    /// 1+: History of play
    fn istate_key(&self, player: Player) -> IStateKey {
        let mut i_state = IStateKey::new();
        i_state.push(self.hands[player]);

        for &h in &self.history {
            let u = h as usize;
            i_state.push(u);
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

    fn istate_string(&self, player: Player) -> String {
        let istate = self.istate_key(player);
        let mut result = String::new();

        result.push_str(format!("{}", istate[0]).as_str());

        for i in 1..istate.len() {
            let char = match istate[i] {
                x if x == KPAction::Bet as usize => 'b',
                x if x == KPAction::Pass as usize => 'p',
                _ => panic!("invalid history"),
            };
            result.push(char);
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
    use rand::{rngs::StdRng, SeedableRng};

    #[test]
    fn kuhn_poker_test_bb() {
        let mut g = KuhnPoker::new_state();
        let mut rng: StdRng = SeedableRng::seed_from_u64(0);
        let mut a1 = RecordedAgent::new(vec![KPAction::Bet as Action; 1]);
        let mut a2 = RecordedAgent::new(vec![KPAction::Bet as Action; 1]);

        run_game(&mut g, &mut vec![&mut a1, &mut a2], &mut rng);

        assert_eq!(format!("{}", g), "[21]bb");
        assert_eq!(g.evaluate(), vec![2.0, -2.0])
    }

    #[test]
    fn kuhn_poker_test_pbp() {
        let mut g = KuhnPoker::new_state();
        let mut rng: StdRng = SeedableRng::seed_from_u64(0);
        let mut a1 = RecordedAgent::new(vec![KPAction::Pass as Action; 2]);
        let mut a2 = RecordedAgent::new(vec![KPAction::Bet as Action; 1]);

        run_game(&mut g, &mut vec![&mut a1, &mut a2], &mut rng);

        assert_eq!(format!("{}", g), "[21]pbp");
        assert_eq!(g.evaluate(), vec![-1.0, 1.0])
    }
}
