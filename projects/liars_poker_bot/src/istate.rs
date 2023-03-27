use crate::game::Action;

#[derive(Debug, Clone, Copy)]
pub struct IStateKey {
    key: u128,
}

impl IStateKey {
    pub fn new() -> Self {
        Self { key: 0 }
    }

    /// Push a new action of `s` bits into the key)
    pub fn push(&mut self, a: Action, s: usize) {
        assert!(s > 0);
        assert!(f64::sqrt(a as f64) <= s as f64);

        self.key = self.key << s;
        let a = u128::try_from(a).expect("Action could not be converted into a 128");
        self.key |= a;
    }

    /// Sets the top `s` bits to a `phase`
    pub fn set_phase(&mut self, _phase: usize, _s: usize) {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::IStateKey;

    #[test]
    fn test_push() {
        let mut k = IStateKey::new();

        k.push(1, 1);
        assert_eq!(k.key, 0b001);

        k.push(1, 1);
        assert_eq!(k.key, 0b011);

        k.push(1, 2);
        assert_eq!(k.key, 0b01101);
    }
}
