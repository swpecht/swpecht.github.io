use std::{
    fmt::Debug,
    ops::{Index, IndexMut},
};

/// An alphamu vector
///
/// True means the game is won, false means it is lost
#[derive(PartialEq, Eq, Clone, Copy)]
pub(super) struct AMVector {
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

    /// Creates a new vec with the given values.
    ///
    /// A value of -1 means the world is invalids
    fn _from_array(values: &[i8]) -> Self {
        let mut vec = AMVector::new(values.len());
        for (i, &v) in values.iter().enumerate() {
            if v == -1 {
                continue;
            } else {
                vec.is_valid[i] = true;
                vec.is_win[i] = v == 1;
            }
        }
        vec
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

        for (w, v) in self.is_win.iter().zip(self.is_valid.iter()).take(self.len) {
            match (v, w) {
                (true, true) => write!(f, "1").unwrap(),
                (true, false) => write!(f, "0").unwrap(),
                (false, _) => write!(f, "-").unwrap(),
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
        self.is_valid[index] = true;
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
                let mut r = AMVector::new(s.len);

                // The Min players can choose different moves in different possible
                // worlds. So they take the minimum outcome over all the possible
                // moves for a possible world. So when they can choose between two
                // vectors they take for each index the minimum between the two values
                // at this index of the two vectors.
                for w in 0..s.len() {
                    r[w] = match (s.is_valid[w], o.is_valid[w]) {
                        (false, false) => continue,
                        (true, false) => s[w],
                        (false, true) => o[w],
                        (true, true) => {
                            // Like this to match paper
                            #[allow(clippy::bool_comparison)]
                            if s[w] < o[w] {
                                s[w]
                            } else {
                                o[w]
                            }
                        }
                    };
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

    use crate::{
        algorithms::alphamu::{front::AMVector, AMFront},
        amvec, front,
    };

    #[test]
    fn test_am_vector_ordering() {
        let v1 = AMVector::_from_array(&[0, 1, 1]);
        // let v1 = amvec!(0, 1, 1);
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
        let v = AMVector::new(10);
        f.push(v);
        assert_eq!(f.vectors.len(), 1);

        // test min of an empty vec
        let f1 = AMFront::default();
        let f2 = front!(amvec![1, 1, 1]);
        let f3 = f1.min(f2);
        assert_eq!(f3, front!(amvec![1, 1, 1]));
    }
}

#[macro_export]
macro_rules! amvec {
    ( $( $x:tt )* ) => {
        {
            let v = vec![$($x)*];
            AMVector::_from_array(&v)
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
