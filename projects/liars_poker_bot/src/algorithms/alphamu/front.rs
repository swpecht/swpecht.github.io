use std::fmt::Debug;

use rustc_hash::FxHashMap;

use crate::collections::bitarray::BitArray;

use super::WorldState;

#[derive(PartialEq, Eq, Clone, Copy, PartialOrd, Ord)]
pub enum VectorValue {
    BigLoss,
    Loss,
    Win,
    BigWin,
}

impl From<VectorValue> for i8 {
    fn from(value: VectorValue) -> Self {
        match value {
            VectorValue::BigLoss => -2,
            VectorValue::Loss => -1,
            VectorValue::Win => 1,
            VectorValue::BigWin => 2,
        }
    }
}

impl From<i8> for VectorValue {
    fn from(value: i8) -> Self {
        use VectorValue::*;
        match value {
            2 => BigWin,
            1 => Win,
            -1 => Loss,
            -2 => BigLoss,
            _ => panic!("cannot convert value: {}", value),
        }
    }
}

impl Debug for VectorValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", i8::from(*self))
    }
}

/// An alphamu vector
///
/// True means the game is won, false means it is lost
#[derive(PartialEq, Eq, Clone, Copy)]
pub(super) struct AMVector {
    is_valid: BitArray,
    is_win: BitArray,
    is_big: BitArray,
    len: usize,
}

impl AMVector {
    fn new(size: usize) -> Self {
        Self {
            is_valid: BitArray::default(),
            is_win: BitArray::default(),
            is_big: BitArray::default(),
            len: size,
        }
    }

    /// Creates a new vec with the given values.
    ///
    /// A value of -1 means the world is invalid
    fn _from_array(values: &[i8]) -> Self {
        let mut vec = AMVector::new(values.len());
        for (i, &v) in values.iter().enumerate() {
            if v == -1 {
                continue;
            } else {
                vec.is_valid.set(i, true);
                let value = match v {
                    0 => VectorValue::Loss,
                    1 => VectorValue::Win,
                    _ => panic!("invalid value: {}", v),
                };
                vec.set(i, value)
            }
        }
        vec
    }

    pub fn from_worlds<T>(worlds: &Vec<WorldState<T>>) -> Self {
        let mut is_valid = BitArray::default();
        for (i, w) in worlds.iter().enumerate() {
            if !w.is_invalid() {
                is_valid.set(i, true);
            }
        }

        Self {
            is_valid,
            is_big: BitArray::default(),
            is_win: BitArray::default(),
            len: worlds.len(),
        }
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

        // We can use a truth table to do bit manipulation to check if each value on Other >= Self.
        // Truth table: https://docs.google.com/spreadsheets/d/1L1wcisMe_e0_dOrFLyEGl2AScWFEzfFzRABxT_HSXyE/edit#gid=0
        // Calculator: https://www.dcode.fr/boolean-truth-table
        // Output: (a && b) || (a && ~d) || ( ~b && ~c) || ( ~c && d)
        //  a       b       c       d
        //  O_win	O_big	S_win	S_big
        let a: u32 = other.is_win.into();
        let b: u32 = other.is_big.into();
        let c: u32 = self.is_win.into();
        let d: u32 = self.is_big.into();
        let valid_mask: u32 = self.is_valid.into();

        // each bit will be 1 if greater than or equal to the value at the same index
        let mut o_gte_s = (a & b) | (a & !d) | (!b & !c) | (!c & d);

        // set all invalid wolrds to 1, they shouldn't impact the outcome
        o_gte_s |= !valid_mask;

        !o_gte_s == 0
    }

    pub fn set(&mut self, index: usize, value: VectorValue) {
        self.is_valid.set(index, true);

        use VectorValue::*;
        if value == BigLoss || value == BigWin {
            self.is_big.set(index, true);
        }

        if value == BigWin || value == Win {
            self.is_win.set(index, true);
        }
    }

    pub fn get(&self, index: usize) -> VectorValue {
        if !self.is_valid.get(index) {
            panic!("accessing invalid world index")
        }

        use VectorValue::*;
        match (self.is_win.get(index), self.is_big.get(index)) {
            (true, true) => BigWin,
            (true, false) => Win,
            (false, true) => BigLoss,
            (false, false) => Loss,
        }
    }

    /// The score of a vector is the average among all possible
    // worlds of the values contained in the vector.
    pub fn score(&self) -> f64 {
        let mut valid_worlds = 0;
        let mut total_score = 0;

        for i in 0..self.len {
            if self.is_valid.get(i) {
                valid_worlds += 1;
                total_score += i8::from(self.get(i)) as i32;
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
                true => write!(f, "{:?}", self.get(i)).unwrap(),
                false => write!(f, "x").unwrap(),
            }
        }

        write!(f, "]")
    }
}

#[derive(Default, PartialEq, Clone)]
pub(super) struct AMFront {
    vectors: FxHashMap<u32, Vec<AMVector>>,
}

impl AMFront {
    pub fn new(v: AMVector) -> Self {
        let mut vectors = FxHashMap::default();
        vectors.insert(v.is_valid.into(), vec![v]);
        Self { vectors }
    }

    pub fn min(self, other: Self) -> Self {
        // trace!(
        //     "min call started on vectors of sizes: {} and {}",
        //     self.len(),
        //     other.len(),
        // );

        if self.is_empty() {
            return other;
        }

        if other.is_empty() {
            return self;
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

                // Remove vectors from result <= r
                let same_worlds = result
                    .vectors
                    .entry(r.is_valid.into())
                    .or_insert(Vec::new());
                same_worlds.retain(|x| !x.is_dominated(&r));

                // If no vector from result >= r
                let is_r_dominated = same_worlds.iter().any(|x| r.is_dominated(x));
                if !is_r_dominated && !same_worlds.contains(&r) {
                    same_worlds.push(r);
                }
            }
        }

        // trace!(
        //     "min called on vectors of sizes: {} and {}, new size: {}, {} buckets",
        //     self.len(),
        //     other.len(),
        //     result.len(),
        //     result.vectors.len()
        // );

        result
    }

    pub fn max(mut self, other: Self) -> Self {
        for (valid, other_vecs) in other.vectors {
            let same_worlds = self.vectors.entry(valid).or_insert(Vec::new());
            for v in &other_vecs {
                if !same_worlds.contains(v) && !same_worlds.iter().any(|x| v.is_dominated(x)) {
                    same_worlds.retain(|x| !x.is_dominated(v));
                    same_worlds.push(v.to_owned());
                }
            }
        }
        self
    }

    pub fn set(&mut self, idx: usize, value: VectorValue) {
        for v in self.vectors.values_mut().flatten() {
            v.set(idx, value);
            v.is_valid.set(idx, true);
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

    pub fn less_than_or_equal(&self, other: &AMFront) -> bool {
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

    /// returns the maximum value of a given world if there is atleast one useful world remaining
    pub fn world_max(&self, i: usize) -> Option<VectorValue> {
        let mut max = None;
        for v in self.vectors.values().flatten() {
            if v.is_valid.get(i) {
                max = Some(max.unwrap_or(VectorValue::BigLoss).max(v.get(i)))
            }
        }
        max
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
        algorithms::alphamu::{
            front::{AMVector, VectorValue},
            AMFront,
        },
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
        assert_eq!(b.len(), 2);
        assert_eq!(b, front!(amvec![0, 1, 1], amvec![1, 1, 0]));

        let c1 = f3.max(f4);
        assert_eq!(c1, front!(amvec![1, 1, 0], amvec![1, 0, 1]));
        let c = c1.max(f5);
        assert_eq!(c, front!(amvec![1, 1, 0], amvec![1, 0, 1]));

        let a = b.min(c);
        assert_eq!(a, front!(amvec![0, 0, 1], amvec![1, 1, 0]));

        // test min of an empty vec
        let f1 = AMFront::default();
        let f2 = front!(amvec![1, 1, 1]);
        let f3 = f1.min(f2);
        assert_eq!(f3, front!(amvec![1, 1, 1]));

        // And reverse order for min
        let f1 = AMFront::default();
        let f2 = front!(amvec![1, 1, 1]);
        let f3 = f2.min(f1);
        assert_eq!(f3, front!(amvec![1, 1, 1]));

        // test min and max with all zeros
        let f1 = front!(amvec![0, 0, 0]);
        let f2 = front!(amvec![0, 0, 0]);
        assert_eq!(f1.min(f2), front!(amvec![0, 0, 0]));

        let f1 = front!(amvec![0, 0, 0]);
        let f2 = front!(amvec![0, 0, 0]);
        assert_eq!(f1.max(f2), front!(amvec![0, 0, 0]));
    }

    #[test]
    fn test_front_less_than_equal() {
        let f1 = front!(amvec![0, 0, 0]);
        assert!(f1.less_than_or_equal(&f1));

        let f2 = front!(amvec![1, 0, 0]);
        assert!(f1.less_than_or_equal(&f2));
        assert!(!f2.less_than_or_equal(&f1));

        let f3 = front!(amvec![1, 0, 0], amvec![0, 0, 1]);
        assert!(f2.less_than_or_equal(&f3));
        assert!(!f3.less_than_or_equal(&f2));
    }

    #[test]
    fn test_world_max() {
        use VectorValue::*;
        let f1 = front!(amvec![0, 0, 0]);
        assert_eq!(f1.world_max(0), Some(Loss));

        let f1 = front!(amvec![1, 0, 0], amvec![0, 0, 1]);
        assert_eq!(f1.world_max(0), Some(Win));
        assert_eq!(f1.world_max(1), Some(Loss));
        assert_eq!(f1.world_max(2), Some(Win));
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
                let w = temp_front.vectors.entry($x.is_valid.into()).or_insert(Vec::new());
                w.push($x);
            )*
            temp_front
        }
    };
}
