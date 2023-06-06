#[derive(Default, Clone, Copy, PartialEq, Hash, Eq)]
pub struct BitArray {
    pub values: u32,
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
