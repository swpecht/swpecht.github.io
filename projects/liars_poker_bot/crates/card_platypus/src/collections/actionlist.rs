use serde::{Deserialize, Serialize};

use games::istate::NormalizedAction;

/// Compact representation of what actions are present in a list. Each
/// possible Action discriminant (u8) is encoded as a single bit.
///
/// Backed by `u64` so Action IDs up to 63 fit. The previous `u32` backing
/// silently wrapped IDs ≥ 32 modulo 32, corrupting any game whose action
/// space exceeded that (e.g. Oh Hell's 52-card deck + bid actions, which
/// occupy IDs 0..=63).
#[derive(Serialize, Deserialize, Default, Clone, Copy)]
pub struct ActionList(u64);

impl ActionList {
    pub fn new(actions: &[NormalizedAction]) -> Self {
        let mut list = Self::default();
        for a in actions {
            list.insert(*a);
        }
        list
    }

    /// Returns the index of a particular id in the current mask
    pub fn index(&self, a: NormalizedAction) -> Option<usize> {
        if !self.contains(a) {
            return None;
        }

        let id = a.get().0;
        // we want to count the number of 1s before our target index
        // to do this, we mask all the top ones, and then count what remains
        let id_mask = !(!0u64 << id);
        Some((self.0 & id_mask).count_ones() as usize)
    }

    pub fn contains(&self, a: NormalizedAction) -> bool {
        let id = a.get().0;
        self.0 & (1u64 << id) > 0
    }

    pub fn insert(&mut self, a: NormalizedAction) {
        let id = a.get().0;
        debug_assert!(id < 64, "ActionList only supports Action IDs in 0..64");
        self.0 |= 1u64 << id;
    }

    pub fn len(&self) -> usize {
        self.0.count_ones() as usize
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn to_vec(&self) -> Vec<NormalizedAction> {
        let mut mask = self.0;
        let mut actions = Vec::with_capacity(self.len());

        while mask.count_ones() > 0 {
            let id = mask.trailing_zeros();
            actions.push(NormalizedAction::new_from_id(id as u8));
            mask &= !(1u64 << id)
        }

        actions
    }
}

#[cfg(test)]
mod tests {
    use games::istate::NormalizedAction;

    use super::ActionList;

    #[test]
    fn test_action_list() {
        let mut list = ActionList::default();

        assert_eq!(list.len(), 0);

        list.insert(NormalizedAction::new_from_id(1));

        assert!(list.contains(NormalizedAction::new_from_id(1)));
        assert_eq!(list.len(), 1);
        assert_eq!(list.index(NormalizedAction::new_from_id(1)), Some(0));

        list.insert(NormalizedAction::new_from_id(0));
        assert!(list.contains(NormalizedAction::new_from_id(1)));
        assert_eq!(list.len(), 2);
        assert_eq!(list.index(NormalizedAction::new_from_id(1)), Some(1));
        assert_eq!(list.index(NormalizedAction::new_from_id(0)), Some(0));

        assert_eq!(
            list.to_vec(),
            vec![
                NormalizedAction::new_from_id(0),
                NormalizedAction::new_from_id(1)
            ]
        )
    }
}
