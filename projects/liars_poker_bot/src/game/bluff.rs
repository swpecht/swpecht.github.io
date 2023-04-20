use std::fmt::{Display, Write};

use crate::istate::IStateKey;

use super::{
    arrayvec::{ArrayVec, SortedArrayVec},
    Action, Game, GameState, Player,
};

const STARTING_DICE: usize = 2;

/// Helper variable for iterating over dice faces
const FACES: [Dice; 6] = [
    Dice::One,
    Dice::Two,
    Dice::Three,
    Dice::Four,
    Dice::Five,
    Dice::Wild,
];

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Dice {
    One,
    Two,
    Three,
    Four,
    Five,
    Wild,
}

impl Display for Dice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let c = match self {
            Dice::One => '1',
            Dice::Two => '2',
            Dice::Three => '3',
            Dice::Four => '4',
            Dice::Five => '5',
            Dice::Wild => '*',
        };

        f.write_char(c)
    }
}

impl Into<usize> for Dice {
    fn into(self) -> usize {
        match self {
            Dice::One => 1,
            Dice::Two => 2,
            Dice::Three => 3,
            Dice::Four => 4,
            Dice::Five => 5,
            Dice::Wild => 6,
        }
    }
}

impl From<usize> for Dice {
    fn from(value: usize) -> Self {
        match value {
            1 => Dice::One,
            2 => Dice::Two,
            3 => Dice::Three,
            4 => Dice::Four,
            5 => Dice::Five,
            6 => Dice::Wild,
            _ => panic!("invalid value to cast to dice"),
        }
    }
}

#[derive(Copy, Clone, PartialEq)]
pub enum BluffActions {
    Roll(Dice),
    Bid(usize, Dice),
    Call,
}

impl PartialOrd for BluffActions {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        let (sn, &sd) = match self {
            BluffActions::Roll(_) => todo!(),
            BluffActions::Bid(n, d) => (n, d),
            BluffActions::Call => todo!(),
        };

        let (on, &od) = match other {
            BluffActions::Roll(_) => todo!(),
            BluffActions::Bid(n, d) => (n, d),
            BluffActions::Call => todo!(),
        };

        if sd == Dice::Wild || od == Dice::Wild {
            todo!();
        }

        // if same dice, go on number of bid
        if sd == od {
            if sn < on {
                return Some(std::cmp::Ordering::Less);
            } else if sn > on {
                return Some(std::cmp::Ordering::Greater);
            } else {
                return Some(std::cmp::Ordering::Equal);
            }
        }

        if sd < od {
            return Some(std::cmp::Ordering::Less);
        } else if sd > od {
            return Some(std::cmp::Ordering::Greater);
        } else {
            return Some(std::cmp::Ordering::Equal);
        }
    }
}

impl Into<usize> for BluffActions {
    fn into(self) -> usize {
        match self {
            BluffActions::Call => 0,
            BluffActions::Roll(d) => d.into(), // 1-6
            BluffActions::Bid(n, d) => {
                let d: usize = d.into();
                6 + (STARTING_DICE * d * 2) + n
            }
        }
    }
}

impl From<usize> for BluffActions {
    fn from(value: usize) -> Self {
        match value {
            0 => BluffActions::Call,
            x if x >= 1 && x <= 6 => BluffActions::Roll(Dice::from(x)),
            x if x <= 30 => {
                let n = (x - 6) % 6;
                let d = x / 6;
                BluffActions::Bid(n, Dice::from(d))
            }
            _ => panic!("invalid action"),
        }
    }
}

impl Display for BluffActions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BluffActions::Bid(n, d) => f.write_str(&format!("{}-{}", n, d)),
            BluffActions::Call => f.write_char('C'),
            BluffActions::Roll(d) => f.write_str(&format!("{}", d)),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Phase {
    RollingDice,
    Betting,
}

#[derive(Copy, Clone, Debug)]
pub struct BluffGameState {
    is_chance_node: bool,
    phase: Phase,
    dice: [SortedArrayVec<STARTING_DICE>; 2],
    bids: ArrayVec<20>,
    cur_player: Player,
    num_players: usize,
    keys: [IStateKey; 2],
}

impl BluffGameState {
    pub fn new_state() -> Self {
        Self {
            is_chance_node: true,
            phase: Phase::RollingDice,
            dice: [SortedArrayVec::new(); 2],
            cur_player: 0,
            num_players: 2,
            keys: [IStateKey::new(); 2],
            bids: ArrayVec::new(),
        }
    }

    pub fn game() -> Game<Self> {
        Game {
            new: Box::new(|| -> Self { Self::new_state() }),
            max_players: 2,
            max_actions: 31, // 4 * 6 for bets + 6 for roll + 1 for call
        }
    }

    pub fn from_actions(actions: &[BluffActions]) -> Self {
        let mut g = (Self::game().new)();
        for &a in actions {
            g.apply_action(a.into());
        }

        return g;
    }

    fn apply_action_rolling(&mut self, a: Action) {
        self.dice[self.cur_player].push(a);
        self.cur_player = (self.cur_player + 1) % 2;

        // check if done rolling
        if self.dice[1].len() == STARTING_DICE {
            self.phase = Phase::Betting;
            self.is_chance_node = false;
        }
    }

    fn apply_action_bids(&mut self, a: Action) {
        self.bids.push(a);
    }

    fn legal_actions_rolling(&self) -> Vec<Action> {
        // Actions are independent
        return vec![
            BluffActions::Roll(Dice::One).into(),
            BluffActions::Roll(Dice::Two).into(),
            BluffActions::Roll(Dice::Three).into(),
            BluffActions::Roll(Dice::Four).into(),
            BluffActions::Roll(Dice::Five).into(),
            BluffActions::Roll(Dice::Wild).into(),
        ];
    }

    fn legal_actions_bids(&self) -> Vec<Action> {
        if self.is_terminal() {
            return Vec::new();
        }

        let mut legal_actions: Vec<Action> = vec![BluffActions::Call.into()];
        if self.bids.len() == 0 {
            let max_bets = STARTING_DICE * 2;
            for &f in &FACES[0..FACES.len() - 1] {
                // don't include the wild
                for n in 1..max_bets + 1 {
                    legal_actions.push(BluffActions::Bid(n, f).into())
                }
            }
            return legal_actions;
        }

        let lb = BluffActions::from(self.bids[self.bids.len() - 1]);

        let max_bets = STARTING_DICE * 2;
        for &f in &FACES[0..FACES.len() - 1] {
            // don't include the wild
            for n in 1..max_bets + 1 {
                let b = BluffActions::Bid(n, f);
                if b > lb {
                    legal_actions.push(b.into())
                }
            }
        }
        return legal_actions;
    }
}

impl GameState for BluffGameState {
    fn apply_action(&mut self, a: Action) {
        match self.phase {
            Phase::RollingDice => self.apply_action_rolling(a),
            Phase::Betting => self.apply_action_bids(a),
        }
    }

    fn legal_actions(&self) -> Vec<Action> {
        match self.phase {
            Phase::RollingDice => self.legal_actions_rolling(),
            Phase::Betting => self.legal_actions_bids(),
        }
    }

    fn evaluate(&self) -> Vec<f32> {
        todo!()
    }

    fn istate_key(&self, player: Player) -> crate::istate::IStateKey {
        return self.keys[player];
    }

    fn istate_string(&self, player: super::Player) -> String {
        todo!()
    }

    fn is_terminal(&self) -> bool {
        if self.bids.len() == 0 {
            return false;
        }

        return self.bids[self.bids.len() - 1] == BluffActions::Call.into();
    }

    fn is_chance_node(&self) -> bool {
        return self.is_chance_node;
    }

    fn num_players(&self) -> usize {
        return self.num_players;
    }

    fn cur_player(&self) -> Player {
        self.cur_player
    }

    fn chance_outcomes(&self, fixed_player: super::Player) -> Vec<super::Action> {
        todo!()
    }

    fn co_istate(
        &self,
        player: super::Player,
        chance_outcome: super::Action,
    ) -> crate::istate::IStateKey {
        todo!()
    }

    fn get_payoff(&self, fixed_player: super::Player, chance_outcome: super::Action) -> f64 {
        todo!()
    }
}

impl Display for BluffGameState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::game::{
        bluff::{BluffGameState, Phase, FACES},
        Action, GameState,
    };

    use super::{BluffActions, Dice, STARTING_DICE};

    /// Ensure the actions are all unique
    #[test]
    fn test_bluff_actions_to_usize() {
        let mut values: HashSet<usize> = HashSet::new();

        let c: usize = BluffActions::Call.into();
        values.insert(c);

        for &f in &FACES {
            let d: usize = BluffActions::Roll(f).into();
            assert!(!values.contains(&d));
            values.insert(d);
        }

        let max_bets = STARTING_DICE * 2;
        for n in 1..max_bets + 1 {
            for &f in &FACES {
                let a: usize = BluffActions::Bid(n, f).into();
                assert!(!values.contains(&a));
                values.insert(a);
            }
        }

        assert_eq!(values.len(), 31)
    }

    #[test]
    fn test_bluff_legal_actions() {
        let mut gs = BluffGameState::new_state();

        assert!(gs.is_chance_node());
        assert_eq!(gs.phase, Phase::RollingDice);

        assert_eq!(
            gs.legal_actions(),
            vec![
                BluffActions::Roll(Dice::One).into(),
                BluffActions::Roll(Dice::Two).into(),
                BluffActions::Roll(Dice::Three).into(),
                BluffActions::Roll(Dice::Four).into(),
                BluffActions::Roll(Dice::Five).into(),
                BluffActions::Roll(Dice::Wild).into()
            ]
        );

        while gs.is_chance_node() {
            gs.apply_action(BluffActions::Roll(Dice::One).into());
        }

        assert!(!gs.is_chance_node());
        assert_eq!(gs.phase, Phase::Betting);

        let mut legal_actions: Vec<Action> = vec![BluffActions::Call.into()];
        let max_bets = STARTING_DICE * 2;
        for &f in &FACES[0..FACES.len() - 1] {
            // don't include the wild
            for n in 1..max_bets + 1 {
                legal_actions.push(BluffActions::Bid(n, f).into())
            }
        }

        assert_eq!(gs.legal_actions(), legal_actions);

        gs.apply_action(BluffActions::Bid(2, Dice::Three).into());
        let mut legal_actions: Vec<Action> = vec![BluffActions::Call.into()];
        let max_bets = STARTING_DICE * 2;
        for n in 3..max_bets + 1 {
            legal_actions.push(BluffActions::Bid(n, Dice::Three).into());
        }

        for &f in &FACES[3..FACES.len() - 1] {
            // don't include the wild
            for n in 1..max_bets + 1 {
                legal_actions.push(BluffActions::Bid(n, f).into())
            }
        }

        assert_eq!(gs.legal_actions(), legal_actions);
    }
}
