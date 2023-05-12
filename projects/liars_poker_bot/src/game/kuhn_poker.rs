use std::fmt::{Debug, Display};

use crate::{
    actions,
    algorithms::ismcts::ResampleFromInfoState,
    game::{Action, Game, GameState, Player},
    istate::IStateKey,
};
use log::trace;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Copy, Serialize, Deserialize)]
pub enum KPAction {
    Bet,
    Pass,
    Jack,
    Queen,
    King,
}

impl From<KPAction> for Action {
    fn from(value: KPAction) -> Self {
        Action(value as u8)
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum KPPhase {
    Dealing,
    Playing,
}

/// Adapted from: https://github.com/deepmind/open_spiel/blob/master/open_spiel/games/kuhn_poker.cc
/// All of the randomness occurs outside of the gamestate. Instead some game states are change nodes. And the
/// "Game runner" will choose of of the random, valid actions
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KPGameState {
    num_players: usize,
    /// Holds the cards for each player in the game
    hands: Vec<Action>,
    is_chance_node: bool,
    is_terminal: bool,
    phase: KPPhase,
    cur_player: usize,
    history: Vec<KPAction>,
    key: IStateKey,
}

impl Display for KPGameState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut result = String::new();
        result.push('[');
        for c in &self.hands {
            result.push_str(&format!("{:?}", KPAction::from(*c)));
        }
        result.push(']');

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

impl Debug for KPGameState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut result = String::new();
        result.push('[');
        for c in &self.hands {
            result.push_str(&format!("{:?}", KPAction::from(*c)));
        }
        result.push(']');

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
            key: IStateKey::new(),
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

        g
    }

    pub fn istate_key(actions: &[KPAction], p: Player) -> IStateKey {
        let mut g = (KuhnPoker::game().new)();
        for &a in actions {
            g.apply_action(a.into());
        }

        g.istate_key(p)
    }
}

impl KPGameState {
    fn apply_action_dealing(&mut self, card: Action) {
        trace!("player {} dealt card {}", self.cur_player, card);

        assert!(vec![
            KPAction::Jack.into(),
            KPAction::Queen.into(),
            KPAction::King.into()
        ]
        .contains(&card));

        assert!(!self.hands.contains(&card));

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
        self.cur_player %= self.num_players;
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
    fn legal_actions(&self, actions: &mut Vec<Action>) {
        actions.clear();

        if self.is_terminal {
            return;
        }

        match self.phase {
            KPPhase::Dealing => self.get_dealing_actions(actions),
            KPPhase::Playing => self.get_betting_actions(actions),
        }
    }

    fn apply_action(&mut self, a: Action) {
        self.key.push(a);

        match self.phase {
            KPPhase::Dealing => self.apply_action_dealing(a),
            KPPhase::Playing => self.apply_action_playing(a),
        }
    }

    /// Returns a vector of the score for each player
    /// at the end of the game
    fn evaluate(&self, p: Player) -> f64 {
        if !self.is_terminal {
            panic!("evaluate called on non-terminal gamestate");
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

        payoffs[p]
    }

    /// Returns an information state with the following data at each index:
    /// 0: Card dealt
    /// 1+: History of play
    fn istate_key(&self, player: Player) -> IStateKey {
        let mut i_state = IStateKey::new();

        // check if we've dealt cards
        if self.hands.len() > player {
            i_state.push(self.hands[player]);
        }

        for &h in &self.history {
            i_state.push(h.into());
        }
        i_state
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

        result
    }

    fn key(&self) -> IStateKey {
        self.key
    }
}

impl ResampleFromInfoState for KPGameState {
    fn resample_from_istate<T: rand::Rng>(&self, player: Player, rng: &mut T) -> Self {
        let player_chance = self.key[player];

        let mut ngs = KuhnPoker::new_state();

        for i in 0..self.key.len() {
            if i == player {
                // the player chance node
                ngs.apply_action(player_chance);
            } else if ngs.is_chance_node() {
                // other player chance node
                let mut actions = actions!(ngs);
                actions.shuffle(rng);
                for a in actions {
                    if a != player_chance {
                        // can't deal same card
                        ngs.apply_action(a);
                        break;
                    }
                }
            } else {
                // public history gets repeated
                ngs.apply_action(self.key[i]);
            }
        }
        ngs
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

        run_game(&mut g, &mut a1, &mut Some(&mut a2), &mut rng);

        assert_eq!(format!("{}", g), "[KingQueen]bb");
        assert_eq!(g.evaluate(0), 2.0);
        assert_eq!(g.evaluate(1), -2.0);
    }

    #[test]
    fn kuhn_poker_test_pbp() {
        let mut g = KuhnPoker::new_state();
        let mut rng: StdRng = SeedableRng::seed_from_u64(0);
        let mut a1 = RecordedAgent::new(vec![KPAction::Pass.into(); 2]);
        let mut a2 = RecordedAgent::new(vec![KPAction::Bet.into(); 1]);

        run_game(&mut g, &mut a1, &mut Some(&mut a2), &mut rng);

        assert_eq!(format!("{}", g), "[KingQueen]pbp");
        assert_eq!(g.evaluate(0), -1.0);
        assert_eq!(g.evaluate(1), 1.0);
    }
}
