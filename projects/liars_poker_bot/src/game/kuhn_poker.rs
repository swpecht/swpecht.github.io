use std::fmt::Display;

use crate::{
    bestresponse::ChanceOutcome,
    game::{Action, Game, GameState, Player},
    istate::IStateKey,
};
use log::trace;

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum KPAction {
    Bet,
    Pass,
    Jack,
    Queen,
    King,
}

impl Into<Action> for KPAction {
    fn into(self) -> Action {
        return Action(self as u8);
    }
}

impl From<Action> for KPAction {
    fn from(value: Action) -> Self {
        match value {
            x if x == KPAction::Bet.into() => KPAction::Bet,
            x if x == KPAction::Pass.into() => KPAction::Pass,
            x if x == KPAction::Jack.into() => KPAction::Jack,
            x if x == KPAction::Queen.into() => KPAction::Queen,
            x if x == KPAction::King.into() => KPAction::King,
            _ => panic!("invalid action"),
        }
    }
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
            result.push_str(&format!("{:?}", KPAction::from(*c)));
        }
        result.push_str("]");

        for &h in &self.history {
            let char = match h {
                KPAction::Bet => 'b',
                KPAction::Pass => 'p',
                _ => panic!("invalid action for history"),
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

    pub fn from_actions(actions: &[KPAction]) -> KPGameState {
        let mut g = (KuhnPoker::game().new)();
        for &a in actions {
            g.apply_action(a.into());
        }

        return g;
    }
}

impl KPGameState {
    fn apply_action_dealing(&mut self, card: Action) {
        trace!("player {} dealt card {}", self.cur_player, card);
        self.hands.push(card);
        self.cur_player += 1;

        if self.cur_player >= self.num_players {
            trace!("moving to playing phase");
            self.phase = KPPhase::Playing;
            self.cur_player = 0;
            self.is_chance_node = false;
        }
    }

    fn apply_action_playing(&mut self, a: Action) {
        match KPAction::from(a) {
            KPAction::Bet => self.history.push(KPAction::Bet),
            KPAction::Pass => self.history.push(KPAction::Pass),
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
        for card in [KPAction::Jack, KPAction::Queen, KPAction::King] {
            if self.hands.contains(&card.into()) {
                // Don't return cards already dealt
                continue;
            }
            actions.push(card.into());
        }
    }

    fn get_betting_actions(&self, actions: &mut Vec<Action>) {
        actions.push(KPAction::Bet.into());
        actions.push(KPAction::Pass.into());
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
            i_state.push(h.into());
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

        result.push_str(format!("{:?}", KPAction::from(istate[0])).as_str());

        for i in 1..istate.len() {
            let char = match istate[i] {
                x if x == KPAction::Bet.into() => 'b',
                x if x == KPAction::Pass.into() => 'p',
                _ => panic!("invalid history"),
            };
            result.push(char);
        }

        return result;
    }

    fn get_payoff(&self, fixed_player: Player, chance_outcome: ChanceOutcome) -> f64 {
        let non_fixed = if fixed_player == 0 { 1 } else { 0 };
        let mut ngs = self.clone();
        ngs.hands[fixed_player] = chance_outcome[0];
        return ngs.evaluate()[non_fixed] as f64;
    }

    fn chance_outcomes(&self, fixed_player: Player) -> Vec<ChanceOutcome> {
        let nf = if fixed_player == 0 { 1 } else { 0 };

        return match KPAction::from(self.hands[nf]) {
            KPAction::Jack => vec![
                ChanceOutcome::new(vec![KPAction::Queen.into()]),
                ChanceOutcome::new(vec![KPAction::King.into()]),
            ],
            KPAction::Queen => vec![
                ChanceOutcome::new(vec![KPAction::Jack.into()]),
                ChanceOutcome::new(vec![KPAction::King.into()]),
            ],
            KPAction::King => vec![
                ChanceOutcome::new(vec![KPAction::Jack.into()]),
                ChanceOutcome::new(vec![KPAction::Queen.into()]),
            ],
            _ => panic!("not implemented for other hands"),
        };
    }

    // returns the istate key for a given player with the chance outcomes replaced with the specified one
    fn co_istate(&self, player: Player, chance_outcome: ChanceOutcome) -> IStateKey {
        let mut istate = self.istate_key(player);
        istate[0] = chance_outcome[0];

        return istate;
    }
}

#[cfg(test)]
mod tests {
    use std::vec;

    use crate::{
        agents::RecordedAgent,
        game::kuhn_poker::{KPAction, KuhnPoker},
        game::{run_game, GameState},
    };
    use rand::{rngs::StdRng, SeedableRng};

    #[test]
    fn kuhn_poker_test_bb() {
        let mut g = KuhnPoker::new_state();
        let mut rng: StdRng = SeedableRng::seed_from_u64(0);
        let mut a1 = RecordedAgent::new(vec![KPAction::Bet.into(); 1]);
        let mut a2 = RecordedAgent::new(vec![KPAction::Bet.into(); 1]);

        run_game(&mut g, &mut vec![&mut a1, &mut a2], &mut rng);

        assert_eq!(format!("{}", g), "[KingQueen]bb");
        assert_eq!(g.evaluate(), vec![2.0, -2.0])
    }

    #[test]
    fn kuhn_poker_test_pbp() {
        let mut g = KuhnPoker::new_state();
        let mut rng: StdRng = SeedableRng::seed_from_u64(0);
        let mut a1 = RecordedAgent::new(vec![KPAction::Pass.into(); 2]);
        let mut a2 = RecordedAgent::new(vec![KPAction::Bet.into(); 1]);

        run_game(&mut g, &mut vec![&mut a1, &mut a2], &mut rng);

        assert_eq!(format!("{}", g), "[KingQueen]pbp");
        assert_eq!(g.evaluate(), vec![-1.0, 1.0])
    }
}
