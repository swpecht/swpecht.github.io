use std::fmt::Debug;

use crate::collections::bitarray::BitArray;

/// An alphamu vector
///
/// True means the game is won, false means it is lost
#[derive(PartialEq, Eq, Clone, Copy)]
pub(super) struct AMVector {
    is_win: BitArray,
    is_valid: BitArray,
    len: usize,
}

impl AMVector {
    fn new(size: usize) -> Self {
        Self {
            is_win: BitArray::default(),
            is_valid: BitArray::default(),
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
                vec.is_valid.set(i, true);
                vec.is_win.set(i, v == 1);
            }
        }
        vec
    }

    pub fn from_worlds<T>(worlds: &Vec<Option<T>>) -> Self {
        let mut is_valid = BitArray::default();
        for (i, w) in worlds.iter().enumerate() {
            if w.is_some() {
                is_valid.set(i, true);
            }
        }

        Self {
            is_win: BitArray::default(),
            is_valid,
            len: worlds.len(),
        }
    }

    fn _push(&mut self, is_win: bool) {
        self.is_valid.set(self.len, true);
        self.is_win.set(self.len, is_win);
        self.len += 1;
    }

    fn _min(mut self, other: Self) -> Self {
        assert_eq!(self.len, other.len);

        for i in 0..self.len() {
            let is_win = self.is_win.get(i);
            self.is_win.set(i, is_win && other.is_win.get(i));
        }

        self
    }

    fn _max(mut self, other: Self) -> Self {
        assert_eq!(self.len, other.len);

        for i in 0..self.len {
            let is_win = self.is_win.get(i);
            self.is_win.set(i, is_win || other.is_win.get(i));
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

        let s_wins = self.is_win.values & self.is_valid.values;
        let o_wins = other.is_win.values & other.is_valid.values;
        // we check if o gives us any other wins, if not, self is dominated by other
        (s_wins | o_wins) == o_wins
    }

    pub fn set(&mut self, index: usize, value: bool) {
        self.is_valid.set(index, true);
        self.is_win.set(index, value);
    }

    pub fn get(&self, index: usize) -> bool {
        if !self.is_valid.get(index) {
            panic!("accessing invalid world index")
        }

        self.is_win.get(index)
    }
}

impl Debug for AMVector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[").unwrap();

        for i in 0..self.len {
            match (self.is_valid.get(i), self.is_win.get(i)) {
                (true, true) => write!(f, "1").unwrap(),
                (true, false) => write!(f, "0").unwrap(),
                (false, _) => write!(f, "-").unwrap(),
            }
        }

        write!(f, "]")
    }
}

#[derive(Default, PartialEq, Clone)]
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
                    let v = match (s.is_valid.get(w), o.is_valid.get(w)) {
                        (false, false) => continue,
                        (true, false) => s.get(w),
                        (false, true) => o.get(w),
                        (true, true) => {
                            // Like this to match paper
                            #[allow(clippy::bool_comparison)]
                            if s.get(w) < o.get(w) {
                                s.get(w)
                            } else {
                                o.get(w)
                            }
                        }
                    };
                    r.set(w, v);
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
            v.is_win.set(idx, is_win);
            v.is_valid.set(idx, true);
        }
    }

    /// Adds a new vector to the front.
    ///
    /// This method does nothing if the vector is dominated by an existing vector.
    /// And it removes any vectors that are dominated by the new one
    pub fn push(&mut self, v: AMVector) {
        if self.vectors.contains(&v) {
            return; // nothing to do if already contained
        }

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

        let mut total = 0;
        for v in &self.vectors {
            total += (v.is_valid.values & v.is_win.values).count_ones()
        }

        total as f64 / self.vectors.len() as f64
    }

    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn less_than_or_equal(&self, other: AMFront) -> bool {
        todo!()
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

        // test min and max with all zeros
        let f1 = front!(amvec![0, 0, 0]);
        let f2 = front!(amvec![0, 0, 0]);
        assert_eq!(f1.min(f2), front!(amvec![0, 0, 0]));

        let f1 = front!(amvec![0, 0, 0]);
        let f2 = front!(amvec![0, 0, 0]);
        assert_eq!(f1.max(f2), front!(amvec![0, 0, 0]));
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
