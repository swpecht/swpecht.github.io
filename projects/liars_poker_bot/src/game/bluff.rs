use std::{
    fmt::{Display, Write},
    hash::Hash,
};

use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use crate::{actions, algorithms::ismcts::ResampleFromInfoState, istate::IStateKey};

use super::{Action, Game, GameState, Player};

const STARTING_DICE: usize = 2;
/// The value `last_bid` is initialized with, represents the lowest bid
const STARTING_BID: BluffActions = BluffActions::Bid(0, Dice::One);

/// Helper variable for iterating over dice faces
const FACES: [Dice; 6] = [
    Dice::One,
    Dice::Two,
    Dice::Three,
    Dice::Four,
    Dice::Five,
    Dice::Wild,
];

#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default, Serialize, Deserialize, Hash,
)]
pub enum Dice {
    #[default]
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

impl From<Dice> for u8 {
    fn from(val: Dice) -> Self {
        match val {
            Dice::One => 1,
            Dice::Two => 2,
            Dice::Three => 3,
            Dice::Four => 4,
            Dice::Five => 5,
            Dice::Wild => 6,
        }
    }
}

impl From<u8> for Dice {
    fn from(value: u8) -> Self {
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

#[derive(Copy, Clone, PartialEq, Debug, Eq, Serialize, Deserialize)]
pub enum BluffActions {
    Roll(Dice),
    Bid(usize, Dice),
    Call,
}

impl BluffActions {
    fn get_dice(&self) -> Dice {
        match self {
            BluffActions::Roll(d) => *d,
            BluffActions::Bid(_, d) => *d,
            BluffActions::Call => panic!("can't get dice on a call"),
        }
    }
}

impl PartialOrd for BluffActions {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (*self == BluffActions::Call, *other == BluffActions::Call) {
            (true, true) => {
                return Some(std::cmp::Ordering::Equal);
            }
            (true, false) => return Some(std::cmp::Ordering::Greater),
            (false, true) => return Some(std::cmp::Ordering::Less),
            _ => {}
        }

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

        // handle 0 bids
        match (*sn, *on) {
            (0, 0) => panic!("can't compare 2 zero bids"),
            (0, _) => return Some(std::cmp::Ordering::Less),
            (_, 0) => return Some(std::cmp::Ordering::Greater),
            _ => {}
        }

        // if same number, go on face of dice
        if sn == on {
            return Some(sd.cmp(&od));
        }

        // If different dice, go on the face value
        Some(sn.cmp(on))
    }
}

impl From<BluffActions> for Action {
    fn from(val: BluffActions) -> Self {
        match val {
            BluffActions::Call => Action(0),
            BluffActions::Roll(d) => Action(d.into()), // 1-6
            BluffActions::Bid(n, d) => {
                let d: u8 = d.into();
                Action(6 + ((d - 1) * STARTING_DICE as u8 * 2) + n as u8)
            }
        }
    }
}

impl From<Action> for BluffActions {
    fn from(value: Action) -> Self {
        match value.0 {
            0 => BluffActions::Call,
            x if (1..=6).contains(&x) => BluffActions::Roll(Dice::from(x)),
            x if x <= 30 => {
                let n = (x as usize - 6) % (STARTING_DICE * 2);
                let n = if n == 0 { 4 } else { n };
                let d = (x - 7) / 4 + 1;
                BluffActions::Bid(n, Dice::from(d))
            }
            _ => panic!("invalid action"),
        }
    }
}

impl From<Dice> for BluffActions {
    fn from(value: Dice) -> Self {
        BluffActions::Roll(value)
    }
}

impl Display for BluffActions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BluffActions::Bid(n, d) => f.write_str(&format!("{}{}", n, d)),
            BluffActions::Call => f.write_char('C'),
            BluffActions::Roll(d) => f.write_str(&format!("{}", d)),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Phase {
    RollingDice,
    Betting,
}

pub struct Bluff {}

impl Bluff {
    pub fn new_state(dice0: usize, dice1: usize) -> BluffGameState {
        BluffGameState {
            dice: Default::default(),
            num_players: 2,
            num_dice: [dice0, dice1],
            key: IStateKey::default(),
        }
    }

    pub fn game(dice0: usize, dice1: usize) -> Game<BluffGameState> {
        let new_f = match (dice0, dice1) {
            (1, 1) => || -> BluffGameState { Self::new_state(1, 1) },
            (2, 1) => || -> BluffGameState { Self::new_state(2, 1) },
            (2, 2) => || -> BluffGameState { Self::new_state(2, 2) },
            _ => panic!("invalid dice configuration"),
        };

        Game {
            new: Box::new(new_f),
            max_players: 2,
            max_actions: 31, // 4 * 6 for bets + 6 for roll + 1 for call
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct BluffGameState {
    dice: [Vec<Dice>; 2],
    num_dice: [usize; 2],
    num_players: usize,
    key: IStateKey,
}

impl BluffGameState {
    pub fn from_actions(actions: &[BluffActions]) -> Self {
        let mut g = (Bluff::game(2, 2).new)();
        for &a in actions {
            g.apply_action(a.into());
        }

        g
    }

    fn apply_action_rolling(&mut self, a: Action) {
        self.dice[self.cur_player()].push(BluffActions::from(a).get_dice());
    }

    fn apply_action_bids(&mut self, a: Action) {
        // Can't bid without any other bids or after a call
        if a == BluffActions::Call.into() && self.last_bid() == STARTING_BID {
            panic!("invalid action");
        }
        assert!(BluffActions::from(a) > self.last_bid());
    }

    fn legal_actions_rolling(&self, actions: &mut Vec<Action>) {
        // Actions are independent
        actions.push(BluffActions::Roll(Dice::One).into());
        actions.push(BluffActions::Roll(Dice::Two).into());
        actions.push(BluffActions::Roll(Dice::Three).into());
        actions.push(BluffActions::Roll(Dice::Four).into());
        actions.push(BluffActions::Roll(Dice::Five).into());
        actions.push(BluffActions::Roll(Dice::Wild).into());
    }

    fn legal_actions_bids(&self, actions: &mut Vec<Action>) {
        if self.is_terminal() {
            return;
        }

        if self.last_bid() != BluffActions::Call && self.last_bid() != STARTING_BID {
            actions.push(BluffActions::Call.into());
        }

        let max_bets = self.num_dice[0] + self.num_dice[1];
        if self.last_bid() == STARTING_BID {
            for &f in &FACES[0..FACES.len() - 1] {
                // don't include the wild
                for n in 1..max_bets + 1 {
                    actions.push(BluffActions::Bid(n, f).into())
                }
            }
            return;
        }

        for &f in &FACES[0..FACES.len() - 1] {
            // don't include the wild
            for n in 1..max_bets + 1 {
                let b = BluffActions::Bid(n, f);
                if b > self.last_bid() {
                    actions.push(b.into())
                }
            }
        }
    }

    fn update_keys(&mut self, a: Action) {
        // game key gets everything
        self.key.push(a);
    }

    fn phase(&self) -> Phase {
        if self.key.len() < self.num_dice[0] + self.num_dice[1] {
            Phase::RollingDice
        } else {
            Phase::Betting
        }
    }

    fn last_bid(&self) -> BluffActions {
        // at least one action other than the dice rolling
        if self.key.len() <= self.num_dice[0] + self.num_dice[1] {
            STARTING_BID
        } else {
            BluffActions::from(self.key[self.key.len() - 1])
        }
    }
}

impl GameState for BluffGameState {
    fn apply_action(&mut self, a: Action) {
        match self.phase() {
            Phase::RollingDice => self.apply_action_rolling(a),
            Phase::Betting => self.apply_action_bids(a),
        }

        self.update_keys(a);
    }

    fn legal_actions(&self, actions: &mut Vec<Action>) {
        actions.clear();
        match self.phase() {
            Phase::RollingDice => self.legal_actions_rolling(actions),
            Phase::Betting => self.legal_actions_bids(actions),
        }
    }

    fn evaluate(&self, p: Player) -> f64 {
        assert_eq!(self.num_players, 2);
        assert!(self.is_terminal());

        calculate_payoff(
            &self.dice,
            self.key[self.key.len() - 2].into(),
            &self.cur_player(),
            p,
        )
    }

    fn istate_key(&self, player: Player) -> crate::istate::IStateKey {
        let public_start = self.num_dice[0] + self.num_dice[1];
        let dice_start = if player == 0 { 0 } else { self.num_dice[0] };
        let dice_end = dice_start + self.num_dice[player];

        let mut key = IStateKey::default();

        for a in self.key[dice_start..dice_end]
            .iter()
            .chain(self.key[public_start..].iter())
        {
            key.push(*a);
        }

        key
    }

    fn istate_string(&self, player: super::Player) -> String {
        let mut istate = String::new();

        let k = self.istate_key(player);

        // push the dice
        for i in 0..self.num_dice[player] {
            let s = format!("{}", BluffActions::from(k[i]));
            istate.push_str(&s);
        }
        istate.push('|');

        for i in self.num_dice[player]..k.len() {
            let s = format!("{}", BluffActions::from(k[i]));
            istate.push_str(&s);

            if i != k.len() - 1 {
                istate.push('|');
            }
        }

        istate
    }

    fn is_terminal(&self) -> bool {
        !self.key.is_empty() && self.key[self.key.len() - 1] == BluffActions::Call.into()
    }

    fn is_chance_node(&self) -> bool {
        self.phase() == Phase::RollingDice
    }

    fn num_players(&self) -> usize {
        self.num_players
    }

    fn cur_player(&self) -> Player {
        if self.key.len() < self.num_dice[0] {
            0
        } else if self.key.len() < self.num_dice[0] + self.num_dice[1] {
            1
        } else if self.key[self.key.len() - 1] == BluffActions::Call.into() {
            (self.key.len() - self.num_dice[0] + self.num_dice[1] - 1) % 2
        // don't change player at end
        } else {
            (self.key.len() - self.num_dice[0] + self.num_dice[1]) % 2
        }
    }

    fn key(&self) -> IStateKey {
        self.key
    }

    fn undo(&mut self) {
        // see if we're undoing a dice roll
        if self.key.len() <= self.num_dice[0] {
            self.dice[0].pop();
        } else if self.key.len() <= self.num_dice[0] + self.num_dice[1] {
            self.dice[1].pop();
        }

        self.key.pop();
    }
}

impl ResampleFromInfoState for BluffGameState {
    fn resample_from_istate<T: rand::Rng>(&self, player: Player, rng: &mut T) -> Self {
        let mut player_chance = match player {
            0 => self.key[0..self.num_dice[0]].to_vec(),
            1 => self.key[self.num_dice[0]..self.num_dice[0] + self.num_dice[1]].to_vec(),
            _ => panic!("invalid player"),
        };

        let mut ngs = Bluff::new_state(self.num_dice[0], self.num_dice[1]);

        for i in 0..self.key.len() {
            if ngs.is_chance_node() && ngs.cur_player() == player {
                // the player chance node
                ngs.apply_action(player_chance.pop().unwrap());
            } else if ngs.is_chance_node() {
                // other player chance node
                let actions = actions!(ngs);
                let a = actions.choose(rng).unwrap();
                ngs.apply_action(*a);
            } else {
                // public history gets repeated
                ngs.apply_action(self.key[i]);
            }
        }
        ngs
    }
}

fn calculate_payoff(
    dice: &[Vec<Dice>; 2],
    last_bid: BluffActions,
    calling_player: &Player,
    player: Player,
) -> f64 {
    let f = last_bid.get_dice();

    let mut n = 0;

    for p_dice in dice {
        for &d in p_dice {
            if d == f || d == Dice::Wild {
                n += 1;
            }
        }
    }

    let actual = BluffActions::Bid(n, f);
    let caller_right = actual < last_bid;

    match (*calling_player == player, caller_right) {
        (true, true) => 1.0,
        (true, false) => -1.0,
        (false, true) => -1.0,
        (false, false) => 1.0,
    }
}

impl Display for BluffGameState {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, vec};

    use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

    use crate::{
        actions,
        game::{
            bluff::{Bluff, BluffGameState, Phase, FACES},
            Action, GameState,
        },
    };

    use super::{BluffActions, Dice, STARTING_DICE};

    #[test]
    fn test_bluff_bid_compare() {
        assert!(BluffActions::Bid(2, Dice::Three) > BluffActions::Bid(1, Dice::Five));
        assert!(BluffActions::Bid(1, Dice::Three) < BluffActions::Bid(1, Dice::Five));
        assert!(BluffActions::Bid(3, Dice::Three) < BluffActions::Bid(4, Dice::Three));
    }
    /// Ensure the actions are all unique
    #[test]
    fn test_bluff_actions_to_action() {
        let mut values: HashSet<Action> = HashSet::new();

        values.insert(BluffActions::Call.into());

        for &f in &FACES {
            let d = BluffActions::Roll(f).into();
            assert!(!values.contains(&d));
            values.insert(d);
        }

        let max_bets = STARTING_DICE * 2;
        for n in 1..max_bets + 1 {
            for &f in &FACES {
                let a = BluffActions::Bid(n, f).into();
                assert!(!values.contains(&a));
                values.insert(a);
            }
        }

        assert_eq!(values.len(), 31)
    }

    #[test]
    fn test_bluff_legal_actions_and_evaluate() {
        let mut gs = Bluff::new_state(2, 2);

        assert!(gs.is_chance_node());
        assert_eq!(gs.phase(), Phase::RollingDice);

        assert_eq!(
            actions!(gs),
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
        assert_eq!(gs.phase(), Phase::Betting);

        let mut legal_actions = Vec::new();
        let max_bets = STARTING_DICE * 2;
        for &f in &FACES[0..FACES.len() - 1] {
            // don't include the wild
            for n in 1..max_bets + 1 {
                legal_actions.push(BluffActions::Bid(n, f).into())
            }
        }

        assert_eq!(actions!(gs), legal_actions);

        gs.apply_action(BluffActions::Bid(2, Dice::Three).into());
        let mut legal_actions: Vec<Action> = vec![BluffActions::Call.into()];
        let max_bets = STARTING_DICE * 2;

        // can bet 2 dice for four or higher
        for &f in &FACES[3..FACES.len() - 1] {
            legal_actions.push(BluffActions::Bid(2, f).into())
        }

        // can bet any face 3 or higher
        for n in 3..max_bets + 1 {
            for &f in &FACES[0..FACES.len() - 1] {
                legal_actions.push(BluffActions::Bid(n, f).into());
            }
        }

        legal_actions.sort();
        assert_eq!(actions!(gs), legal_actions);

        // player 1 calls, they are right
        assert_eq!(gs.cur_player(), 1);
        gs.apply_action(BluffActions::Call.into());

        assert!(gs.is_terminal());
        assert_eq!(gs.evaluate(0), -1.0);
        assert_eq!(gs.evaluate(1), 1.0);
    }

    #[test]
    fn test_bluff_istate() {
        let mut gs = BluffGameState::from_actions(&[
            BluffActions::Roll(Dice::One),
            BluffActions::Roll(Dice::Two),
            BluffActions::Roll(Dice::Three),
            BluffActions::Roll(Dice::Four),
            BluffActions::Bid(1, Dice::One),
        ]);

        let istate = gs.istate_string(0);
        assert_eq!(istate, "12|11");
        let istate = gs.istate_string(1);
        assert_eq!(istate, "34|11");

        gs.apply_action(BluffActions::Bid(2, Dice::Three).into());
        let istate = gs.istate_string(0);
        assert_eq!(istate, "12|11|23");
        let istate = gs.istate_string(1);
        assert_eq!(istate, "34|11|23");

        gs.apply_action(BluffActions::Call.into());
        let istate = gs.istate_string(0);
        assert_eq!(istate, "12|11|23|C");
        let istate = gs.istate_string(1);
        assert_eq!(istate, "34|11|23|C");
    }

    #[test]
    fn test_bluff_action_into_from() {
        let max_bets = 2 * STARTING_DICE;
        for &f in &FACES[0..FACES.len() - 1] {
            // don't include the wild
            for n in 1..max_bets + 1 {
                let bid = BluffActions::Bid(n, f);
                let a: Action = bid.into();
                let from_bid = BluffActions::from(a);
                assert_eq!(from_bid, bid);
            }
        }
    }

    #[test]
    fn test_bluff_1_1() {
        let mut gs = (Bluff::game(1, 1).new)();
        assert_eq!(
            actions!(gs),
            vec![
                BluffActions::Roll(Dice::One).into(),
                BluffActions::Roll(Dice::Two).into(),
                BluffActions::Roll(Dice::Three).into(),
                BluffActions::Roll(Dice::Four).into(),
                BluffActions::Roll(Dice::Five).into(),
                BluffActions::Roll(Dice::Wild).into()
            ]
        );

        assert_eq!(gs.cur_player(), 0);
        gs.apply_action(BluffActions::Roll(Dice::One).into());
        assert_eq!(gs.cur_player(), 1);

        gs.apply_action(BluffActions::Roll(Dice::Wild).into());
        assert_eq!(gs.cur_player(), 0);

        assert_eq!(
            actions!(gs),
            vec![
                BluffActions::Bid(1, Dice::One).into(),
                BluffActions::Bid(2, Dice::One).into(),
                BluffActions::Bid(1, Dice::Two).into(),
                BluffActions::Bid(2, Dice::Two).into(),
                BluffActions::Bid(1, Dice::Three).into(),
                BluffActions::Bid(2, Dice::Three).into(),
                BluffActions::Bid(1, Dice::Four).into(),
                BluffActions::Bid(2, Dice::Four).into(),
                BluffActions::Bid(1, Dice::Five).into(),
                BluffActions::Bid(2, Dice::Five).into()
            ]
        );

        gs.apply_action(BluffActions::Bid(1, Dice::One).into());

        assert_eq!(
            actions!(gs),
            vec![
                BluffActions::Call.into(),
                BluffActions::Bid(2, Dice::One).into(),
                BluffActions::Bid(1, Dice::Two).into(),
                BluffActions::Bid(2, Dice::Two).into(),
                BluffActions::Bid(1, Dice::Three).into(),
                BluffActions::Bid(2, Dice::Three).into(),
                BluffActions::Bid(1, Dice::Four).into(),
                BluffActions::Bid(2, Dice::Four).into(),
                BluffActions::Bid(1, Dice::Five).into(),
                BluffActions::Bid(2, Dice::Five).into()
            ]
        );

        gs.apply_action(BluffActions::Bid(2, Dice::One).into());
        gs.apply_action(BluffActions::Call.into());

        assert_eq!(gs.evaluate(0), -1.0);
        assert_eq!(gs.evaluate(1), 1.0);

        let istate = gs.istate_string(0);
        assert_eq!(istate, "1|11|21|C");
        let istate = gs.istate_string(1);
        assert_eq!(istate, "*|11|21|C");
    }

    #[test]
    fn test_bluff_2_1() {
        let mut gs = (Bluff::game(2, 1).new)();
        assert_eq!(
            actions!(gs),
            vec![
                BluffActions::Roll(Dice::One).into(),
                BluffActions::Roll(Dice::Two).into(),
                BluffActions::Roll(Dice::Three).into(),
                BluffActions::Roll(Dice::Four).into(),
                BluffActions::Roll(Dice::Five).into(),
                BluffActions::Roll(Dice::Wild).into()
            ]
        );

        assert_eq!(gs.cur_player(), 0);
        gs.apply_action(BluffActions::Roll(Dice::One).into());
        gs.apply_action(BluffActions::Roll(Dice::One).into());
        assert_eq!(gs.cur_player(), 1);

        gs.apply_action(BluffActions::Roll(Dice::Wild).into());
        assert_eq!(gs.cur_player(), 0);

        assert_eq!(
            actions!(gs),
            vec![
                BluffActions::Bid(1, Dice::One).into(),
                BluffActions::Bid(2, Dice::One).into(),
                BluffActions::Bid(3, Dice::One).into(),
                BluffActions::Bid(1, Dice::Two).into(),
                BluffActions::Bid(2, Dice::Two).into(),
                BluffActions::Bid(3, Dice::Two).into(),
                BluffActions::Bid(1, Dice::Three).into(),
                BluffActions::Bid(2, Dice::Three).into(),
                BluffActions::Bid(3, Dice::Three).into(),
                BluffActions::Bid(1, Dice::Four).into(),
                BluffActions::Bid(2, Dice::Four).into(),
                BluffActions::Bid(3, Dice::Four).into(),
                BluffActions::Bid(1, Dice::Five).into(),
                BluffActions::Bid(2, Dice::Five).into(),
                BluffActions::Bid(3, Dice::Five).into()
            ]
        );

        gs.apply_action(BluffActions::Bid(1, Dice::One).into());

        assert_eq!(
            actions!(gs),
            vec![
                BluffActions::Call.into(),
                BluffActions::Bid(2, Dice::One).into(),
                BluffActions::Bid(3, Dice::One).into(),
                BluffActions::Bid(1, Dice::Two).into(),
                BluffActions::Bid(2, Dice::Two).into(),
                BluffActions::Bid(3, Dice::Two).into(),
                BluffActions::Bid(1, Dice::Three).into(),
                BluffActions::Bid(2, Dice::Three).into(),
                BluffActions::Bid(3, Dice::Three).into(),
                BluffActions::Bid(1, Dice::Four).into(),
                BluffActions::Bid(2, Dice::Four).into(),
                BluffActions::Bid(3, Dice::Four).into(),
                BluffActions::Bid(1, Dice::Five).into(),
                BluffActions::Bid(2, Dice::Five).into(),
                BluffActions::Bid(3, Dice::Five).into()
            ]
        );

        gs.apply_action(BluffActions::Bid(3, Dice::One).into());
        gs.apply_action(BluffActions::Call.into());

        assert_eq!(gs.evaluate(0), -1.0);
        assert_eq!(gs.evaluate(1), 1.0);

        let istate = gs.istate_string(0);
        assert_eq!(istate, "11|11|31|C");
        let istate = gs.istate_string(1);
        assert_eq!(istate, "*|11|31|C");
    }

    #[test]
    fn test_undo_bluff11() {
        let mut rng: StdRng = SeedableRng::seed_from_u64(0);
        for _ in 0..1000 {
            let mut gs = Bluff::new_state(1, 1);

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

    #[test]
    fn test_undo_bluff21() {
        let mut rng: StdRng = SeedableRng::seed_from_u64(0);
        for _ in 0..1000 {
            let mut gs = Bluff::new_state(2, 1);

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
