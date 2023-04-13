use std::ops::Index;

use super::Action;

/// Array backed card storage that implements Vector-like features and is copyable
/// It also always remains sorted
#[derive(Clone, Copy, Debug)]
pub struct CardVec<const N: usize> {
    len: usize,
    cards: [Action; N],
}

impl<const N: usize> CardVec<N> {
    pub fn new() -> Self {
        Self {
            len: 0,
            cards: [0; N],
        }
    }

    pub fn push(&mut self, c: Action) {
        assert!(self.len < self.cards.len());

        if self.len == 0 || self.cards[self.len - 1] <= c {
            // put it on the end
            self.cards[self.len] = c;
        } else {
            for i in 0..self.len {
                if c < self.cards[i] {
                    self.shift_right(i);
                    self.cards[i] = c;
                    break;
                }
            }
        }

        self.len += 1;
    }

    /// shifts all elements right starting at the item in idx, so idx will become idx+1
    fn shift_right(&mut self, idx: usize) {
        for i in (idx..self.len).rev() {
            self.cards[i + 1] = self.cards[i];
        }
    }

    // shifts all elements left starting at the item in idx, so idx will become idx-1
    fn shift_left(&mut self, idx: usize) {
        for i in idx..self.len {
            self.cards[i - 1] = self.cards[i];
        }
    }

    pub fn remove(&mut self, c: Action) {
        for i in 0..self.len {
            if self.cards[i] == c {
                self.shift_left(i + 1);
                self.len -= 1;
                return;
            }
        }

        panic!("attempted to remove item not in list")
    }

    pub fn len(&self) -> usize {
        return self.len;
    }

    pub fn to_vec(&self) -> Vec<Action> {
        let mut v = Vec::with_capacity(self.len);
        for i in 0..self.len {
            v.push(self.cards[i]);
        }

        return v;
    }

    pub fn contains(&self, c: &Action) -> bool {
        let mut contains = false;

        for i in 0..self.len {
            if self.cards[i] == *c {
                contains = true;
            }
        }

        return contains;
    }
}

impl<const N: usize> Index<usize> for CardVec<N> {
    type Output = Action;

    fn index(&self, index: usize) -> &Self::Output {
        assert!(index <= self.len);
        return &self.cards[index];
    }
}

#[cfg(test)]
mod tests {
    use super::CardVec;

    #[test]
    fn test_card_vec() {
        let mut h: CardVec<5> = CardVec::new();

        // test basic add and index
        h.push(0);
        h.push(1);
        assert_eq!(h[0], 0);
        assert_eq!(h[1], 1);
        assert!(h.contains(&1));
        assert!(!h.contains(&10));
        assert_eq!(h.len(), 2);

        // test sorting
        h.push(10);
        h.push(2);
        assert_eq!(h[2], 2);
        assert_eq!(h[3], 10);
        assert_eq!(h.len(), 4);

        h.remove(1);
        assert_eq!(h[0], 0);
        assert_eq!(h[1], 2);
        assert_eq!(h[2], 10);
        assert_eq!(h.len(), 3);
    }
}
