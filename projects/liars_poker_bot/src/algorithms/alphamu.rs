use std::{
    cmp::Ordering,
    collections::HashSet,
    fmt::Debug,
    ops::{Index, IndexMut},
};

use crate::game::{Action, GameState, Player};

/// Implementation for AlphaMu from "The αµ Search Algorithm for the Game of Bridge"
///
/// https://arxiv.org/pdf/1911.07960.pdf
pub struct AlphaMuBot<G> {
    worlds: Vec<G>,
    team: Team,
}

impl<G: GameState> AlphaMuBot<G> {
    pub fn run_search(&mut self, root_node: &G, m: usize, num_worlds: usize) {
        self.reset();

        self.team = self.get_team(root_node);

        self.alphamu(root_node, m, Vec::new());
    }

    fn alphamu(&mut self, gs: &G, m: usize, worlds: Vec<Option<G>>) -> AMFront {
        assert!(!gs.is_chance_node());

        let mut result = AMFront::default();
        if self.stop(gs, m, &worlds, &mut result) {
            return result;
        }

        let mut front = AMFront::default();
        if self.team != self.get_team(gs) {
            // min node
            for a in self.all_moves(&worlds) {
                let s = self.play(gs, a);
                let worlds_1 = self.filter_and_progress_worlds(&worlds, a);
                let f = self.alphamu(&s, m, worlds_1);
                front = front.min(f);
            }
        } else {
            // max node
            for a in self.all_moves(&worlds) {
                let s = self.play(gs, a);
                let worlds_1 = self.filter_and_progress_worlds(&worlds, a);
                let f = self.alphamu(&s, m - 1, worlds_1);
                front = front.max(f);
            }
        }

        front
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
        let mut worlds_1 = vec![None; worlds.len()];
        let mut actions = Vec::new();
        for w in worlds.iter().flatten() {
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
        if gs.is_terminal() {
            let value = gs.evaluate(self.team.into());
            if value > 0.0 {
                // we won
                for (i, _) in worlds.iter().enumerate() {
                    result.set(i, true);
                }
            } else {
                // we lost
                for (i, _) in worlds.iter().enumerate() {
                    result.set(i, false);
                }
            }

            return true;
        }

        if m == 0 {
            todo!()
        }

        false
    }

    fn reset(&mut self) {
        self.worlds.clear();
        todo!()
    }

    fn get_team(&self, gs: &G) -> Team {
        match gs.cur_player() {
            0 | 2 => Team::Team1,
            1 | 3 => Team::Team2,
            _ => panic!("invalid player"),
        }
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
    fn new(size: usize) -> Self {
        Self {
            is_win: [false; 32],
            is_valid: [false; 32],
            len: size,
        }
    }

    fn push(&mut self, is_win: bool) {
        self.is_valid[self.len] = true;
        self.is_win[self.len] = is_win;
        self.len += 1;
    }

    fn min(mut self, other: Self) -> Self {
        assert_eq!(self.len, other.len);

        for (i, w) in self.is_win.iter_mut().enumerate() {
            *w = *w && other.is_win[i];
        }

        self
    }

    fn max(mut self, other: Self) -> Self {
        assert_eq!(self.len, other.len);

        for (i, w) in self.is_win.iter_mut().enumerate() {
            *w = *w || other.is_win[i];
        }

        self
    }

    fn len(&self) -> usize {
        self.len
    }

    fn is_empty(&self) -> bool {
        self.len == 0
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

impl PartialOrd for AMVector {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        assert_eq!(self.len, other.len);

        // A vector is greater or equal to another vector if for all indices it
        // contains a value greater or equal to the value contained at this index
        // in the other vector and if the valid worlds are the same for the two
        let same_valid_worlds = self.is_valid == other.is_valid;
        if !same_valid_worlds {
            return Some(std::cmp::Ordering::Less);
        }

        if self.is_win == other.is_win {
            return Some(std::cmp::Ordering::Equal);
        }

        let mut is_greater_or_equal = true;
        for (i, is_win) in self
            .is_win
            .iter()
            .enumerate()
            .filter(|(i, _)| self.is_valid[*i])
        {
            is_greater_or_equal &= *is_win >= other.is_win[i];
        }

        if is_greater_or_equal {
            return Some(Ordering::Greater);
        }

        Some(Ordering::Less)
    }
}

impl Index<usize> for AMVector {
    type Output = bool;

    fn index(&self, index: usize) -> &Self::Output {
        if !self.is_valid[index] {
            panic!("accessing data for invalid world")
        }
        &self.is_win[index]
    }
}

impl IndexMut<usize> for AMVector {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.is_valid[index] = true;
        &mut self.is_win[index]
    }
}

#[derive(Default, PartialEq)]
struct AMFront {
    vectors: Vec<AMVector>,
}

impl AMFront {
    fn min(self, other: Self) -> Self {
        // A Pareto front is greater or
        // equal to another Pareto front if for each element of the second Pareto
        // front there is an element in the first Pareto front which is greater or
        // equal to the element of the second Pareto front.
        let mut result = AMFront::default();
        for s in &self.vectors {
            for o in &other.vectors {
                let mut r = AMVector::default();
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
        let mut is_dominated = true;
        for sv in &self.vectors {
            if v > *sv {
                is_dominated = false;
            }
        }

        if !is_dominated {
            let domainted = self.get_dominated_vectors(v);
            for d in domainted {
                self.vectors.remove(d);
            }
            self.vectors.push(v);
        }
    }

    fn get_dominated_vectors(&self, v: AMVector) -> Vec<usize> {
        let mut dominated = Vec::new();
        for (i, s) in self.vectors.iter().enumerate() {
            if v >= *s {
                dominated.push(i);
            }
        }
        dominated
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
    use crate::{
        algorithms::alphamu::{AMFront, AMVector},
        amvec, front,
    };

    #[test]
    fn test_am_vector_ordering() {
        let mut v1 = amvec!(0, 1, 1);
        let v2 = amvec!(0, 0, 1);

        assert!(v1 == v1);
        assert!(v1 >= v2);
        assert!(v2 <= v1);

        v1.is_valid[2] = false;

        // Different valid worlds, each is less than the other
        assert!(v1 < v2);
        assert!(v2 < v1);
    }

    #[test]
    fn test_am_vector_min_max() {
        let v1 = amvec!(1, 0, 1);
        let v2 = amvec!(1, 1, 0);

        assert_eq!(v1.min(v2), amvec!(1, 0, 0));
        assert_eq!(v1.max(v2), amvec!(1, 1, 1));
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

        let b = f1.max(f2);
        assert_eq!(b, front!(amvec![0, 1, 1], amvec![1, 1, 0]));

        let c = f3.max(f4).max(f5);
        assert_eq!(c, front!(amvec![1, 1, 0], amvec![1, 0, 1]));

        let a = b.min(c);
        assert_eq!(a, front!(amvec![0, 0, 1], amvec![1, 1, 0]));
    }
}

#[macro_export]
macro_rules! amvec {
    ( $( $x:expr ),* ) => {
        {
            let mut temp_vec = AMVector::default();
            $(
                match $x {
                    0 => temp_vec.push(false),
                    1 => temp_vec.push(true),
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
