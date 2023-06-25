use std::fmt::Display;

#[derive(Default, Clone, Copy, PartialEq, Hash, Eq)]
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
