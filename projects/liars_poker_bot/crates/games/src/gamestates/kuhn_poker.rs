use std::{
    fmt::{Debug, Display},
    hash::Hash,
};

use crate::{
    actions, istate::IStateKey, resample::ResampleFromInfoState, Action, Game, GameState, Player,
};
use itertools::Itertools;

use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Copy, Serialize, Deserialize, Hash)]
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
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug, Hash)]
pub struct KPGameState {
    num_players: usize,
    key: IStateKey,
}

impl Display for KPGameState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut result = String::new();
        result.push('[');
        for c in self.key.into_iter().take(2) {
            result.push_str(&format!("{:?}", KPAction::from(c)));
        }
        result.push(']');

        for h in self.key.into_iter().skip(2) {
            let char = match KPAction::from(h) {
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
            num_players: 2,
            key: IStateKey::default(),
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
        assert!([
            KPAction::Jack.into(),
            KPAction::Queen.into(),
            KPAction::King.into()
        ]
        .contains(&card));

        if self.key.len() > 1 {
            assert!(self.key[0] != card);
        }
    }

    fn apply_action_playing(&mut self, _: Action) {}

    fn get_dealing_actions(&self, actions: &mut Vec<Action>) {
        for card in [KPAction::Jack, KPAction::Queen, KPAction::King] {
            if !self.key.is_empty() && self.key[0] == card.into() {
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

    fn phase(&self) -> KPPhase {
        if self.key.len() < 2 {
            KPPhase::Dealing
        } else {
            KPPhase::Playing
        }
    }
}

impl GameState for KPGameState {
    fn legal_actions(&self, actions: &mut Vec<Action>) {
        actions.clear();

        if self.is_terminal() {
            return;
        }

        match self.phase() {
            KPPhase::Dealing => self.get_dealing_actions(actions),
            KPPhase::Playing => self.get_betting_actions(actions),
        }
    }

    fn apply_action(&mut self, a: Action) {
        match self.phase() {
            KPPhase::Dealing => self.apply_action_dealing(a),
            KPPhase::Playing => self.apply_action_playing(a),
        }
        self.key.push(a);
    }

    /// Returns a vector of the score for each player
    /// at the end of the game
    fn evaluate(&self, p: Player) -> f64 {
        if !self.is_terminal() {
            panic!("evaluate called on non-terminal gamestate");
        }

        if self.num_players != 2 {
            panic!("game logic only implemented for 2 players")
        }

        if self.key[0] == self.key[1] {
            panic!("invalid deal, players have same cards")
        }

        let payoffs = match self.key[2..]
            .iter()
            .map(|x| KPAction::from(*x))
            .collect_vec()[..]
        {
            [KPAction::Pass, KPAction::Pass] => {
                if self.key[0] > self.key[1] {
                    [1.0, -1.0]
                } else {
                    [-1.0, 1.0]
                }
            }
            [KPAction::Bet, KPAction::Bet] | [KPAction::Pass, KPAction::Bet, KPAction::Bet] => {
                if self.key[0] > self.key[1] {
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
        let mut i_state = IStateKey::default();

        // check if we've dealt cards
        if self.key.len() > player {
            i_state.push(self.key[player]);
        }

        if self.key.len() > 2 {
            for &h in &self.key[2..] {
                i_state.push(h);
            }
        }
        i_state
    }

    fn is_terminal(&self) -> bool {
        (self.key.len() == self.num_players + 2 && self.key[2] == self.key[3])
            || (self.key.len() == self.num_players + 2
                && self.key[2] == KPAction::Bet.into()
                && self.key[3] == KPAction::Pass.into())
            || self.key.len() == 5
    }

    fn is_chance_node(&self) -> bool {
        self.key.len() < 2
    }

    fn num_players(&self) -> usize {
        self.num_players
    }

    fn cur_player(&self) -> usize {
        self.key.len() % 2
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

    fn undo(&mut self) {
        self.key.pop();
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

    use crate::{
        actions,
        gamestates::kuhn_poker::{KPAction, KuhnPoker},
        GameState,
    };
    use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

    #[test]
    fn kuhn_poker_test_bb() {
        let mut g = KuhnPoker::new_state();

        for a in [
            KPAction::King,
            KPAction::Queen,
            KPAction::Bet,
            KPAction::Bet,
        ] {
            g.apply_action(a.into());
        }

        assert_eq!(format!("{}", g), "[KingQueen]bb");
        assert_eq!(g.evaluate(0), 2.0);
        assert_eq!(g.evaluate(1), -2.0);
    }

    #[test]
    fn kuhn_poker_test_pbp() {
        let mut g = KuhnPoker::new_state();
        for a in [
            KPAction::King,
            KPAction::Queen,
            KPAction::Pass,
            KPAction::Bet,
            KPAction::Pass,
        ] {
            g.apply_action(a.into());
        }

        assert_eq!(format!("{}", g), "[KingQueen]pbp");
        assert_eq!(g.evaluate(0), -1.0);
        assert_eq!(g.evaluate(1), 1.0);
    }

    #[test]
    fn kuhn_poker_test_undo() {
        let mut rng: StdRng = SeedableRng::seed_from_u64(0);
        for _ in 0..1000 {
            let mut gs = KuhnPoker::new_state();

            while !gs.is_terminal() {
                let actions = actions!(gs);
                let a = actions.choose(&mut rng).unwrap();
                let mut ngs = gs.clone();
                ngs.apply_action(*a);
                ngs.undo();
                assert_eq!(ngs, gs);
                gs.apply_action(*a);
            }
        }
    }
}
