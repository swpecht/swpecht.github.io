use crate::game::Action;

pub type Key = u128;

#[derive(Debug, Clone, Copy)]
pub struct IStateKey {
    pub key: Key,
}

impl IStateKey {
    pub fn new() -> Self {
        Self { key: 1 }
    }

    /// Push a new action of `s` bits into the key)
    pub fn push(&mut self, a: Action, s: usize) {
        assert!(s > 0);

        if f64::sqrt(a as f64) > s as f64 {
            panic!("value too large for size")
        }

        self.key = self.key << s;
        let a = Key::try_from(a).expect("Action could not be converted into a 128");
        self.key |= a;
    }

    /// Sets the top `s` bits to a `phase`
    pub fn set_phase(&mut self, _phase: usize, _s: usize) {
        todo!()
    }

    /// Returns the index of the first bit with data. It ignores the first bit used
    /// to track the first data
    pub fn first_bit(&self) -> usize {
        let mut f = 0;
        let len = std::mem::size_of::<Key>() * 8 - 1;
        for i in 0..len {
            let is_set = self.key & (1 << (len - i)) != 0;
            if is_set {
                f = len - i;
                break;
            }
        }

        return f;
    }

    pub fn read(&self, start: usize, size: usize) -> Action {
        assert!(size > 0);

        let mut mask: Key = match size {
            1 => 0b1,
            2 => 0b11,
            3 => 0b111,
            4 => 0b1111,
            5 => 0b11111,
            _ => todo!("not yet implemented"),
        };
        mask = mask << (start - size);
        let mut v = self.key & mask;
        v = v >> (start - size);

        return v as Action;
    }
}

#[cfg(test)]
mod tests {
    use super::IStateKey;

    #[test]
    fn test_push() {
        let mut k = IStateKey::new();

        k.push(1, 1);
        assert_eq!(k.key, 0b011);

        k.push(1, 1);
        assert_eq!(k.key, 0b111);

        k.push(1, 2);
        assert_eq!(k.key, 0b11101);
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
}
