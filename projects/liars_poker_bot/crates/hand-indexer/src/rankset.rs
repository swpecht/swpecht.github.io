use crate::N;

#[derive(Default)]
pub struct RankSet(u16);

impl RankSet {
    pub fn new(ranks: &[u8]) -> Self {
        let mut set = RankSet::default();
        for r in ranks {
            set.insert(*r);
        }

        set
    }

    pub fn insert(&mut self, rank: u8) {
        assert!(rank <= N, "can't insert a rank>N");
        self.0 |= 1 << rank;
    }

    pub fn len(&self) -> u8 {
        self.0.count_ones() as u8
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Return the rank of the largest item in the set
    pub fn largest(&self) -> u8 {
        // 16 bits for the u16
        // off by 1 on the leading zeros
        16 - 1 - self.0.leading_zeros() as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn largest() {
        let mut set = RankSet::default();

        set.insert(3);
        assert_eq!(set.largest(), 3);
        set.insert(1);
        assert_eq!(set.largest(), 3);

        set.insert(5);
        assert_eq!(set.largest(), 5);
    }
}
