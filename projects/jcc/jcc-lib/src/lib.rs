#![no_std]

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

/// State machine to keep track of tone changes based on signal
///
/// Could change this to implement the morse code converstion itself, and jsut take signal to text
pub struct ToneDetector {}

impl ToneDetector {
    pub fn process_signal(reading: u8) {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
