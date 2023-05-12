use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    marker::PhantomData,
    ops::{Index, IndexMut},
};

use log::trace;
use rand::{rngs::StdRng, seq::SliceRandom, thread_rng, SeedableRng};
use rustc_hash::FxHashMap;

use crate::{
    actions,
    agents::Agent,
    cfragent::cfrnode::ActionVec,
    game::{Action, GameState, Player},
    istate::IStateKey,
};

use super::ismcts::{Evaluator, ResampleFromInfoState};

#[derive(Default)]
struct ChildNode {
    /// Average chance of winning averaged across all worlds in all fronts
    win_sum: FxHashMap<Action, f64>,
}

impl ChildNode {
    fn update_action_value(&mut self, a: Action, f: &AMFront) {
        let sum = self.win_sum.entry(a).or_insert(0.0);
        *sum = f.avg_wins();
    }
}

/// Implementation for AlphaMu from "The αµ Search Algorithm for the Game of Bridge"
///
/// https://arxiv.org/pdf/1911.07960.pdf
pub struct AlphaMuBot<G, E> {
    evaluator: E,
    team: Team,
    num_worlds: usize,
    m: usize,
    _phantom_data: PhantomData<G>,
    nodes: HashMap<IStateKey, ChildNode>,
    rng: StdRng,
}

impl<G: GameState + ResampleFromInfoState, E: Evaluator<G>> AlphaMuBot<G, E> {
    pub fn new(evaluator: E, num_worlds: usize, m: usize) -> Self {
        Self {
            evaluator,
            team: Team::Team1,
            _phantom_data: PhantomData,
            num_worlds,
            m,
            nodes: HashMap::default(),
            rng: SeedableRng::seed_from_u64(42),
        }
    }

    pub fn run_search(&mut self, root_node: &G) -> ActionVec<f64> {
        self.reset();

        let mut rng = thread_rng();
        self.team = self.get_team(root_node);
        let player = root_node.cur_player();
        let mut worlds = Vec::new();
        for _ in 0..self.num_worlds {
            worlds.push(Some(root_node.resample_from_istate(player, &mut rng)))
        }

        self.alphamu(root_node, self.m, worlds);
        self.get_final_policy(root_node)
    }

    /// Returns the policy normalized to the win rates per action
    fn get_final_policy(&self, root_node: &G) -> ActionVec<f64> {
        let key = root_node.istate_key(root_node.cur_player());
        let node = self.nodes.get(&key).unwrap();

        let v_sum: f64 = node.win_sum.values().sum();
        let actions = actions!(root_node);
        let mut policy = ActionVec::new(&actions);

        if v_sum == 0.0 {
            // no wins to play randomly
            let prob = 1.0 / actions.len() as f64;
            for a in actions {
                policy[a] = prob;
            }
            return policy;
        }

        for (a, v) in &node.win_sum {
            policy[*a] = v / v_sum;
        }

        policy
    }

    fn alphamu(&mut self, gs: &G, m: usize, worlds: Vec<Option<G>>) -> AMFront {
        trace!(
            "alpha mu call: gs: {:?}\tm: {}\tworlds: {:?}",
            gs,
            m,
            worlds
        );

        assert!(!gs.is_chance_node());

        let mut result = AMFront::default();
        result.push(AMVector::from_worlds(&worlds));
        if self.stop(gs, m, &worlds, &mut result) {
            trace!("stopping alpha mu for {:?}, found front: {:?}", gs, result);
            return result;
        }

        let mut front = AMFront::default();
        if self.team != self.get_team(gs) {
            // min node
            for a in self.all_moves(&worlds) {
                let s = self.play(gs, a);
                let worlds_1 = self.filter_and_progress_worlds(&worlds, a);
                let f = self.alphamu(&s, m, worlds_1);
                self.save_node(gs, a, &f);
                front = front.min(f);
            }
        } else {
            // max node
            for a in self.all_moves(&worlds) {
                let s = self.play(gs, a);
                let worlds_1 = self.filter_and_progress_worlds(&worlds, a);
                let f = self.alphamu(&s, m - 1, worlds_1);
                self.save_node(gs, a, &f);
                front = front.max(f);
            }
        }

        assert!(!front.is_empty());
        front
    }

    fn save_node(&mut self, gs: &G, a: Action, f: &AMFront) {
        let key = gs.istate_key(gs.cur_player());
        let node = self.nodes.entry(key).or_insert(ChildNode::default());
        node.update_action_value(a, f);
    }

    fn all_moves(&self, worlds: &[Option<G>]) -> HashSet<Action> {
        let mut all_moves = HashSet::new();
        let mut actions = Vec::new();
        for w in worlds.iter().flatten() {
            w.legal_actions(&mut actions);
            for a in &actions {
                all_moves.insert(*a);
            }
        }
        all_moves
    }

    /// Returns the progressed worlds where a was a valid action. Otherwise it
    /// marks that spot as None
    fn filter_and_progress_worlds(&mut self, worlds: &Vec<Option<G>>, a: Action) -> Vec<Option<G>> {
        let mut worlds_1 = Vec::with_capacity(worlds.len());
        let mut actions = Vec::new();
        for w in worlds.iter() {
            if w.is_none() {
                worlds_1.push(None);
            }
            let w = w.as_ref().unwrap();
            w.legal_actions(&mut actions);
            if actions.contains(&a) {
                let nw = self.play(w, a);
                worlds_1.push(Some(nw));
            } else {
                worlds_1.push(None)
            }
        }

        assert_eq!(worlds.len(), worlds_1.len());
        worlds_1
    }

    fn play(&mut self, gs: &G, a: Action) -> G {
        let mut ngs = gs.clone();
        ngs.apply_action(a);
        ngs
    }

    fn stop(&mut self, gs: &G, m: usize, worlds: &[Option<G>], result: &mut AMFront) -> bool {
        let mut all_states_terminal = true;
        for w in worlds.iter().flatten() {
            all_states_terminal &= w.is_terminal();
        }
        if gs.is_terminal() & !all_states_terminal {
            panic!("gs is terminal but other states are not")
        }

        if gs.is_terminal() {
            // Unlike in bridge, we can't determin who won only from the public information at the gamestate
            // instead we need to evaluate each world individually
            for (i, w) in worlds.iter().enumerate() {
                if w.is_none() {
                    continue;
                }
                let w = w.as_ref().unwrap();
                let v = w.evaluate(self.team.into());
                if v > 0.0 {
                    result.set(i, true);
                } else {
                    result.set(i, false);
                }
            }
            return true;
        }

        if m == 0 {
            for (i, w) in worlds.iter().enumerate().filter(|(_, w)| w.is_some()) {
                let w = w.as_ref().unwrap();
                let r = self.evaluator.evaluate(w);
                if r[self.team as usize] > 0.0 {
                    // win most times
                    result.set(i, true);
                } else {
                    result.set(i, false);
                }
            }
            return true;
        }

        false
    }

    fn reset(&mut self) {
        self.nodes.clear();
    }

    fn get_team(&self, gs: &G) -> Team {
        match gs.cur_player() {
            0 | 2 => Team::Team1,
            1 | 3 => Team::Team2,
            _ => panic!("invalid player"),
        }
    }
}

impl<G: GameState + ResampleFromInfoState, E: Evaluator<G>> Agent<G> for AlphaMuBot<G, E> {
    fn step(&mut self, s: &G) -> Action {
        let action_weights = self.run_search(s).to_vec();
        action_weights
            .choose_weighted(&mut self.rng, |item| item.1)
            .unwrap()
            .0
    }

    fn get_name(&self) -> String {
        format!(
            "AlphaMu, worlds: {}, m: {}, evaluator: todo",
            self.num_worlds, self.m
        )
    }
}

/// An alphamu vector
///
/// True means the game is won, false means it is lost
#[derive(PartialEq, Eq, Clone, Copy, Default)]
struct AMVector {
    is_win: [bool; 32],
    is_valid: [bool; 32],
    len: usize,
}

impl AMVector {
    fn _new(size: usize) -> Self {
        let mut is_valid = [false; 32];
        for v in is_valid.iter_mut() {
            *v = true;
        }

        Self {
            is_win: [false; 32],
            is_valid,
            len: size,
        }
    }

    fn from_worlds<T>(worlds: &Vec<Option<T>>) -> Self {
        let mut is_valid = [false; 32];
        for (i, w) in worlds.iter().enumerate() {
            if w.is_some() {
                is_valid[i] = true;
            }
        }

        Self {
            is_win: [false; 32],
            is_valid,
            len: worlds.len(),
        }
    }

    /// Return an AMVector with the same valid worlds
    fn from_other(other: &Self) -> Self {
        Self {
            is_win: [false; 32],
            is_valid: other.is_valid,
            len: other.len,
        }
    }

    fn _push(&mut self, is_win: bool) {
        self.is_valid[self.len] = true;
        self.is_win[self.len] = is_win;
        self.len += 1;
    }

    fn _min(mut self, other: Self) -> Self {
        assert_eq!(self.len, other.len);

        for (i, w) in self.is_win.iter_mut().enumerate() {
            *w = *w && other.is_win[i];
        }

        self
    }

    fn _max(mut self, other: Self) -> Self {
        assert_eq!(self.len, other.len);

        for (i, w) in self.is_win.iter_mut().enumerate() {
            *w = *w || other.is_win[i];
        }

        self
    }

    fn len(&self) -> usize {
        self.len
    }

    fn _is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns if self is dominated by other
    fn is_dominated(&self, other: &AMVector) -> bool {
        assert_eq!(self.len, other.len);

        // A vector is greater or equal to another vector if for all indices it
        // contains a value greater or equal to the value contained at this index
        // in the other vector and if the valid worlds are the same for the two
        let same_valid_worlds = self.is_valid == other.is_valid;
        if !same_valid_worlds {
            panic!("cant compare vectors with different valid worlds")
        }

        // the two are equal
        if self.is_win == other.is_win {
            return false;
        }

        let mut is_greater_or_equal = true;
        for (i, is_win) in self
            .is_win
            .iter()
            .enumerate()
            .filter(|(i, _)| self.is_valid[*i])
        {
            is_greater_or_equal &= other.is_win[i] >= *is_win;
        }

        is_greater_or_equal
    }
}

impl Debug for AMVector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[").unwrap();

        for w in self.is_win.iter().take(self.len) {
            match w {
                true => write!(f, "1").unwrap(),
                false => write!(f, "0").unwrap(),
            }
        }

        write!(f, "]")
    }
}

impl Index<usize> for AMVector {
    type Output = bool;

    fn index(&self, index: usize) -> &Self::Output {
        if !self.is_valid[index] {
            panic!("accessing data for invalid world");
        }
        &self.is_win[index]
    }
}

impl IndexMut<usize> for AMVector {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.len = self.len.max(index + 1);
        if !self.is_valid[index] {
            panic!("setting invalid world value");
        }
        &mut self.is_win[index]
    }
}

#[derive(Default, PartialEq)]
struct AMFront {
    vectors: Vec<AMVector>,
}

impl AMFront {
    fn min(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }

        let mut result = AMFront::default();
        for s in &self.vectors {
            for o in &other.vectors {
                let mut r = AMVector::from_other(s);
                for w in 0..s.len() {
                    // Leave the bool comparison here to match the paper
                    #[allow(clippy::bool_comparison)]
                    if s[w] < o[w] {
                        r[w] = s[w];
                    } else {
                        r[w] = o[w];
                    }
                }
                result.push(r);
            }
        }

        result
    }

    fn max(mut self, other: Self) -> Self {
        for v in other.vectors {
            if !self.vectors.contains(&v) {
                self.push(v);
            }
        }
        self
    }

    fn set(&mut self, idx: usize, is_win: bool) {
        for v in self.vectors.iter_mut() {
            v.is_win[idx] = is_win;
            v.is_valid[idx] = true;
        }
    }

    /// Adds a new vector to the front.
    ///
    /// This method does nothing if the vector is dominated by an existing vector.
    /// And it removes any vectors that are dominated by the new one
    fn push(&mut self, v: AMVector) {
        let mut is_dominated = false;
        for sv in &self.vectors {
            if v.is_dominated(sv) {
                is_dominated = true;
            }
        }

        if !is_dominated {
            let domainted = self.get_dominated_vectors(v);
            // iterate in reverse order to preserve indexes when removing
            for d in domainted.iter().rev() {
                self.vectors.remove(*d);
            }
            self.vectors.push(v);
        }
    }

    fn get_dominated_vectors(&self, v: AMVector) -> Vec<usize> {
        let mut dominated = Vec::new();
        for (i, s) in self.vectors.iter().enumerate() {
            if s.is_dominated(&v) {
                dominated.push(i);
            }
        }
        dominated
    }

    /// Returns the average wins across all vectors in a front
    fn avg_wins(&self) -> f64 {
        assert!(!self.vectors.is_empty());

        let total: usize = self
            .vectors
            .iter()
            .map(|v| v.is_win.iter().filter(|x| **x).count())
            .sum();

        total as f64 / self.vectors.len() as f64
    }

    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Debug for AMFront {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{").unwrap();

        for v in &self.vectors {
            write!(f, "{:?}", v).unwrap();
        }

        write!(f, "}}")
    }
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Team {
    Team1,
    Team2,
}

impl From<Team> for Player {
    fn from(val: Team) -> Self {
        val as usize
    }
}

#[cfg(test)]
mod tests {
    use rand::{seq::SliceRandom, thread_rng, SeedableRng};

    use crate::{
        actions,
        agents::{Agent, RandomAgent},
        algorithms::{
            alphamu::{AMFront, AMVector},
            ismcts::RandomRolloutEvaluator,
        },
        amvec, front,
        game::{kuhn_poker::KuhnPoker, GameState},
    };

    use super::AlphaMuBot;

    #[test]
    fn test_am_vector_ordering() {
        let v1 = amvec!(0, 1, 1);
        let v2 = amvec!(0, 0, 1);

        assert!(v1 != v2);
        assert!(v2.is_dominated(&v1));
        assert!(!v1.is_dominated(&v2));

        let v3 = amvec![0, 1, 1];
        let v4 = amvec![1, 1, 0];
        assert!(!v3.is_dominated(&v4));
        assert!(!v4.is_dominated(&v3));
        assert!(!v3.is_dominated(&v3));
    }

    #[test]
    fn test_am_vector_min_max() {
        let v1 = amvec!(1, 0, 1);
        let v2 = amvec!(1, 1, 0);

        assert_eq!(v1._min(v2), amvec!(1, 0, 0));
        assert_eq!(v1._max(v2), amvec!(1, 1, 1));
    }

    /// Test based on Fig 2 in alphamu paper
    /// https://arxiv.org/pdf/1911.07960.pdf
    #[test]
    fn test_am_front() {
        // define the root nodes
        let f1 = front!(amvec![0, 1, 1]);
        let f2 = front!(amvec![1, 1, 0]);
        let f3 = front!(amvec![1, 1, 0]);
        let f4 = front!(amvec![1, 0, 1]);
        let f5 = front!(amvec![1, 0, 0]);

        assert!(f1 != f2);

        let b = f1.max(f2);
        assert_eq!(b.vectors.len(), 2);
        assert_eq!(b, front!(amvec![0, 1, 1], amvec![1, 1, 0]));

        let c1 = f3.max(f4);
        assert_eq!(c1, front!(amvec![1, 1, 0], amvec![1, 0, 1]));
        let c = c1.max(f5);
        assert_eq!(c, front!(amvec![1, 1, 0], amvec![1, 0, 1]));

        let a = b.min(c);
        assert_eq!(a, front!(amvec![0, 0, 1], amvec![1, 1, 0]));

        let mut x = front!(amvec![1, 0, 0], amvec![0, 1, 1]);
        x.push(amvec!(0, 0, 1));
        assert_eq!(x, front!(amvec![1, 0, 0], amvec![0, 1, 1])); //no change
        x.push(amvec!(1, 1, 0));
        assert_eq!(x, front!(amvec![0, 1, 1], amvec![1, 1, 0]));

        let mut f = AMFront::default();
        let v = AMVector::_new(10);
        f.push(v);
        assert_eq!(f.vectors.len(), 1);

        // test min of an empty vec
        let f1 = AMFront::default();
        let f2 = front!(amvec![1, 1, 1]);
        let f3 = f1.min(f2);
        assert_eq!(f3, front!(amvec![1, 1, 1]));
    }

    #[test]
    fn test_alpha_mu_is_agent() {
        let rng = SeedableRng::seed_from_u64(42);
        let mut alphamu = AlphaMuBot::new(RandomRolloutEvaluator::new(100, rng), 20, 10);
        let mut opponent = RandomAgent::default();

        for _ in 0..10 {
            let mut gs = KuhnPoker::new_state();
            while gs.is_chance_node() {
                let a = *actions!(gs).choose(&mut thread_rng()).unwrap();
                gs.apply_action(a)
            }

            while !gs.is_terminal() {
                let a = match gs.cur_player() {
                    0 => alphamu.step(&gs),
                    1 => opponent.step(&gs),
                    _ => panic!("invalid player"),
                };
                gs.apply_action(a);
            }

            gs.evaluate(0);
        }
    }
}

#[macro_export]
macro_rules! amvec {
    ( $( $x:expr ),* ) => {
        {
            let mut temp_vec = AMVector::default();
            $(
                match $x {
                    0 => temp_vec._push(false),
                    1 => temp_vec._push(true),
                    _ => panic!("invalid input"),
                };
            )*
            temp_vec
        }
    };
}

#[macro_export]
macro_rules! front {
    ( $( $x:expr ),* ) => {
        {
            let mut temp_front = AMFront::default();
            $(
                temp_front.push($x);
            )*
            temp_front
        }
    };
}
