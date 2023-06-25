use std::{
    fmt::Display,
    ops::{BitAnd, BitOr, BitOrAssign, Not},
};

#[derive(Default, Clone, Copy, PartialEq, Hash, Eq, Debug)]
pub struct BitArray {
    values: u32,
}

impl BitArray {
    pub fn get(&self, index: usize) -> bool {
        (self.values >> index) & 0b1 == 1
    }

    pub fn set(&mut self, index: usize, value: bool) {
        match value {
            true => self.values |= 1 << index,
            false => self.values &= !(1 << index),
        }
    }
}

impl Display for BitArray {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:b}", self.values)
    }
}

impl From<BitArray> for u32 {
    fn from(value: BitArray) -> Self {
        value.values
    }
}

impl From<u32> for BitArray {
    fn from(value: u32) -> Self {
        BitArray { values: value }
    }
}

impl BitOr for BitArray {
    type Output = BitArray;

    fn bitor(mut self, rhs: Self) -> Self::Output {
        self.values |= rhs.values;
        self
    }
}

impl BitAnd for BitArray {
    type Output = BitArray;

    fn bitand(mut self, rhs: Self) -> Self::Output {
        self.values &= rhs.values;
        self
    }
}

impl Not for BitArray {
    type Output = BitArray;

    fn not(mut self) -> Self::Output {
        self.values = !self.values;
        self
    }
}

impl BitOrAssign for BitArray {
    fn bitor_assign(&mut self, rhs: Self) {
        self.values |= rhs.values
    }
}
