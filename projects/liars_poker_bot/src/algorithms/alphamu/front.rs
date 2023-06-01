use std::{
    fmt::Debug,
    ops::{Index, IndexMut},
};

/// An alphamu vector
///
/// True means the game is won, false means it is lost
#[derive(PartialEq, Eq, Clone, Copy, Default)]
pub(super) struct AMVector {
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

    pub fn from_worlds<T>(worlds: &Vec<Option<T>>) -> Self {
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
            return false;
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
pub(super) struct AMFront {
    vectors: Vec<AMVector>,
}

impl AMFront {
    pub fn min(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }

        let mut result = AMFront::default();
        for s in &self.vectors {
            for o in &other.vectors {
                if s.is_valid != o.is_valid {
                    panic!("attempting to take min of vectors with different valid worlds")
                }

                let mut r = AMVector::from_other(s);
                for w in 0..s.len() {
                    if !s.is_valid[w] || !o.is_valid[w] {
                        continue;
                    }
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

    pub fn max(mut self, other: Self) -> Self {
        for v in other.vectors {
            if !self.vectors.contains(&v) {
                self.push(v);
            }
        }
        self
    }

    pub fn set(&mut self, idx: usize, is_win: bool) {
        for v in self.vectors.iter_mut() {
            v.is_win[idx] = is_win;
            v.is_valid[idx] = true;
        }
    }

    /// Adds a new vector to the front.
    ///
    /// This method does nothing if the vector is dominated by an existing vector.
    /// And it removes any vectors that are dominated by the new one
    pub fn push(&mut self, v: AMVector) {
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
    pub fn avg_wins(&self) -> f64 {
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

#[cfg(test)]
mod tests {
    use rand::{seq::SliceRandom, thread_rng, SeedableRng};

    use crate::{
        actions,
        agents::{Agent, RandomAgent},
        algorithms::{
            alphamu::{front::AMVector, AMFront, AlphaMuBot},
            ismcts::RandomRolloutEvaluator,
        },
        amvec, front,
        game::{kuhn_poker::KuhnPoker, GameState},
    };

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
