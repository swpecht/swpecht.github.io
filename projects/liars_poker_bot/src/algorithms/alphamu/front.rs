use std::{collections::HashMap, fmt::Debug};

use log::trace;

use crate::collections::bitarray::BitArray;

use super::WorldState;

/// An alphamu vector
///
/// True means the game is won, false means it is lost
#[derive(PartialEq, Eq, Clone, Copy)]
pub(super) struct AMVector {
    values: [i8; 32],
    is_valid: BitArray,
    len: usize,
}

impl AMVector {
    fn new(size: usize) -> Self {
        Self {
            values: [0; 32],
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
                vec.values[i] = values[i];
            }
        }
        vec
    }

    pub fn from_worlds<T>(worlds: &Vec<WorldState<T>>) -> Self {
        let mut is_valid = BitArray::default();
        for (i, w) in worlds.iter().enumerate() {
            if w.is_some() {
                is_valid.set(i, true);
            }
        }

        Self {
            values: [0; 32],
            is_valid,
            len: worlds.len(),
        }
    }

    fn _push(&mut self, value: i8) {
        self.is_valid.set(self.len, true);
        self.values[self.len] = value;
        self.len += 1;
    }

    fn len(&self) -> usize {
        self.len
    }

    fn _is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns if self is dominated by other
    fn is_dominated(&self, other: &AMVector) -> bool {
        // A vector is greater or equal to another vector if for all indices it
        // contains a value greater or equal to the value contained at this index
        // in the other vector and if the valid worlds are the same for the two
        if self.is_valid != other.is_valid {
            return false;
        }

        assert_eq!(self.len, other.len);
        // as an optimization, we don't only check values for valid worlds. This should be ok if invalid values are
        // always 0. But care should be take to ensure this invariant holds in the future.
        other
            .values
            .into_iter()
            .zip(self.values)
            .all(|(o, s)| o >= s)
    }

    pub fn set(&mut self, index: usize, value: i8) {
        self.is_valid.set(index, true);
        self.values[index] = value;
    }

    pub fn get(&self, index: usize) -> i8 {
        if !self.is_valid.get(index) {
            panic!("accessing invalid world index")
        }

        self.values[index]
    }

    /// The score of a vector is the average among all possible
    // worlds of the values contained in the vector.
    pub fn score(&self) -> f64 {
        let mut valid_worlds = 0;
        let mut total_score = 0;

        for i in 0..self.len {
            if self.is_valid.get(i) {
                valid_worlds += 1;
                total_score += self.values[i];
            }
        }
        total_score as f64 / valid_worlds as f64
    }
}

impl Debug for AMVector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[").unwrap();

        for i in 0..self.len {
            match self.is_valid.get(i) {
                true => write!(f, "{}", self.values[i]).unwrap(),
                false => write!(f, "x").unwrap(),
            }
        }

        write!(f, "]")
    }
}

#[derive(Default, PartialEq, Clone)]
pub(super) struct AMFront {
    vectors: HashMap<BitArray, Vec<AMVector>>,
}

impl AMFront {
    pub fn min(self, other: Self) -> Self {
        trace!(
            "min call started on vectors of sizes: {} and {}",
            self.len(),
            other.len(),
        );

        if self.is_empty() {
            return other;
        }

        let mut result = AMFront::default();
        for s in self.vectors.values().flatten() {
            for o in other.vectors.values().flatten() {
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
                        (true, true) => s.get(w).min(o.get(w)),
                    };
                    r.set(w, v);
                }
                result.push(r);
            }
        }

        trace!(
            "min called on vectors of sizes: {} and {}, new size: {}, {} buckets",
            self.len(),
            other.len(),
            result.len(),
            result.vectors.len()
        );

        result
    }

    pub fn max(mut self, other: Self) -> Self {
        for (is_valid, vectors) in other.vectors {
            let svectors = self.vectors.entry(is_valid).or_insert(Vec::new()).clone();
            for v in vectors {
                if !svectors.contains(&v) {
                    self.push(v);
                }
            }
        }

        self
    }

    pub fn set(&mut self, idx: usize, value: i8) {
        for v in self.vectors.values_mut().flatten() {
            v.values[idx] = value;
            v.is_valid.set(idx, true);
        }
    }

    /// Adds a new vector to the front.
    ///
    /// This method does nothing if the vector is dominated by an existing vector.
    /// And it removes any vectors that are dominated by the new one
    pub fn push(&mut self, v: AMVector) {
        // we only need to compare things with the same set of valid world since
        // a vector will never be dominated by a vector with a different set of valid worlds
        let svectors = self.vectors.entry(v.is_valid).or_default();

        // Remove vectors from result <= r
        svectors.retain(|sv| !sv.is_dominated(&v));

        // If no vector from result >= r
        let is_v_dominated = svectors.iter().any(|sv| v.is_dominated(sv));
        if !is_v_dominated {
            svectors.push(v);
        }
    }

    /// Score of a front
    ///
    /// From alpha mu paper:
    /// The score of a move for the declarer is the score of
    /// the vector that has the best score among the vectors in the Pareto front
    /// of the move.
    pub fn score(&self) -> f64 {
        assert!(!self.vectors.is_empty());

        let mut max_score = f64::NEG_INFINITY;
        for v in self.vectors.values().flatten() {
            max_score = max_score.max(v.score());
        }
        max_score
    }

    pub fn len(&self) -> usize {
        self.vectors.values().map(|x| x.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn less_than_or_equal(&self, other: AMFront) -> bool {
        for s in self.vectors.values().flatten() {
            let mut one_greater_or_equal = false;
            for v in other.vectors.values().flatten() {
                if s.is_dominated(v) {
                    one_greater_or_equal = true;
                    break;
                }
            }
            if !one_greater_or_equal {
                return false;
            }
        }
        true
    }
}

impl Debug for AMFront {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{").unwrap();

        for v in self.vectors.values().flatten() {
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
        // an equal vector dominates another vector
        assert!(v3.is_dominated(&v3));
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
        assert_eq!(b.vectors.values().map(|x| x.len()).sum::<usize>(), 2);
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
