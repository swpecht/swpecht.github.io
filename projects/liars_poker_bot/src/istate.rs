use serde::{Deserialize, Serialize};

use crate::game::Action;

/// For euchre, need the following bits:
/// 25 for deal: 5 cards * 5 bits
/// 5 for face up: 1 card * 5 bits
/// 4 for pickup: 4 players * bool
/// 12 for choose trump: 4 players * 3 bits for 5 choices
/// 100 for play: 4 players * 5 cards * 5 bits
pub type KeyFragment = u128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IStateKey {
    key: [KeyFragment; 2],
}

impl IStateKey {
    pub fn new() -> Self {
        Self { key: [1, 0] }
    }

    /// Push a new action of `s` bits into the key)
    pub fn push(&mut self, a: Action, s: usize) {
        assert!(s > 0);

        #[cfg(debug_assertions)]
        {
            if (a as f64).log2().ceil() > s as f64 {
                panic!("value too large for size")
            }
        }

        if s > 32 {
            panic!("don't support writes over 32 bits");
        }

        let len = std::mem::size_of::<KeyFragment>() * 8;
        let is_overflow = (self.key[1] >> (len - s)) > 0;
        if is_overflow {
            panic!("overflowing key")
        }

        let overflow = self.key[0] >> (len - s);
        self.key[1] = self.key[1] << s;
        self.key[1] |= overflow;

        self.key[0] = self.key[0] << s;
        let a = KeyFragment::try_from(a).expect("Action could not be converted into a 128");
        self.key[0] |= a;
    }

    /// Returns the index of the first bit with data. It ignores the first bit used
    /// to track the first data
    pub fn first_bit(&self) -> usize {
        let mut f = 0;
        let len = std::mem::size_of::<KeyFragment>() * 8;
        for i in (0..(len * self.key.len())).rev() {
            let idx = i / len;
            let is_set = self.key[idx] & (1 << (i % len)) != 0;
            if is_set {
                f = i;
                break;
            }
        }

        return f;
    }

    pub fn read(&self, start: usize, size: usize) -> Action {
        assert!(size > 0);
        assert!(start + 1 >= size);

        let fragment_length = std::mem::size_of::<KeyFragment>() * 8;
        assert!(start < fragment_length * self.key.len());

        let key_idx = start / fragment_length;
        let mask = get_mask(size);

        let rel_start = start - fragment_length * key_idx;
        let overflow = if size > rel_start {
            size - rel_start
        } else {
            0
        };

        // Reads from highest fragment
        let mut v = (self.key[key_idx] >> (rel_start - (size - overflow))) & (mask >> overflow);
        if overflow > 0 {
            v = v << overflow;
            v |= (self.key[key_idx - 1] >> (fragment_length - overflow))
                & (mask >> (size - overflow));
        }

        return v as Action;
    }

    /// Returns a version of the IStateKey trimmed to a certain number of bits
    ///
    /// Upper bits are set to 0.
    pub fn trim(&self, n: usize) -> IStateKey {
        if n >= self.len() {
            return self.clone();
        }

        let trim_amt = self.first_bit() - n;

        let mut mask = 1;
        for _ in 0..trim_amt - 1 {
            mask = mask << 1;
            mask |= 1;
        }

        let mut k0 = self.key[0] >> trim_amt;
        let overflow = self.key[1] & mask;
        k0 |= overflow << trim_amt;

        let k1 = self.key[1] >> trim_amt;

        return Self { key: [k0, k1] };
    }

    pub fn len(&self) -> usize {
        return self.first_bit();
    }
}

impl ToString for IStateKey {
    fn to_string(&self) -> String {
        format!("{:b}{:b}", self.key[1], self.key[0])
    }
}

/// Returns a mask for bit manipulation
fn get_mask(size: usize) -> KeyFragment {
    match size {
        1 => 0b1,
        2 => 0b11,
        3 => 0b111,
        4 => 0b1111,
        5 => 0b11111,
        6 => 0b111111,
        7 => 0b1111111,
        8 => 0b11111111,
        _ => {
            let mut m = 1;
            for _ in 0..size {
                m = m << 1;
                m |= 1;
            }
            m
        }
    }
}

#[cfg(test)]
mod tests {

    use super::{IStateKey, KeyFragment};

    #[test]
    fn test_push() {
        let mut k = IStateKey::new();

        k.push(1, 1);
        assert_eq!(k.key[0], 0b011);

        k.push(1, 1);
        assert_eq!(k.key[0], 0b111);

        k.push(1, 2);
        assert_eq!(k.key[0], 0b11101);
    }

    #[test]
    fn test_first_bit() {
        let mut k = IStateKey::new();

        k.push(0, 3);
        let f = k.first_bit();
        assert_eq!(f, 3);

        k.push(1, 3);
        let f = k.first_bit();
        assert_eq!(f, 6);
    }

    #[test]
    fn test_read() {
        let mut k = IStateKey::new();
        k.push(0b1010, 4);

        let v = k.read(4, 1);
        assert_eq!(v, 1);

        let v = k.read(3, 1);
        assert_eq!(v, 0);

        let v = k.read(4, 2);
        assert_eq!(v, 2);
    }

    #[test]
    fn test_overflow() {
        let mut k = IStateKey::new();
        assert_eq!(k.read(129, 3), 0);
        k.push(0b101, 3);

        k.push(0, 32);
        k.push(0, 32);
        k.push(0, 32);
        k.push(0, 32);
        assert_eq!(k.read(128 + 3, 3), 0b101);

        assert_eq!(k.first_bit(), 128 + 3);
    }

    #[test]
    fn test_read_write_stress() {
        for n in 0..u8::MAX {
            for offset in 1..(std::mem::size_of::<KeyFragment>() * 8 * 2 - 8 - 1) {
                let mut k = IStateKey::new();
                k.push(n as usize, 8);
                for _ in 0..offset {
                    k.push(0, 1);
                }
                assert_eq!(k.read(offset + 8, 8) as u8, n);
            }
        }
    }

    #[test]
    fn test_len() {
        let mut k = IStateKey::new();
        assert_eq!(k.len(), 0);
        k.push(1, 5);
        assert_eq!(k.len(), 5);

        k.push(1, 32);
        k.push(1, 32);
        k.push(1, 32);
        k.push(1, 32);
        assert_eq!(k.len(), 128 + 5);
    }
}
