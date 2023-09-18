use serde::{Deserialize, Serialize};

use crate::game::Action;

/// Compact representation of what actions are present in a list
#[derive(Serialize, Deserialize, Default)]
struct ActionList(u32);

impl ActionList {
    /// Returns the index of a particular id in the current mask
    fn index(&self, a: Action) -> usize {
        let id = a.0;
        // we want to count the number of 1s before our target index
        // to do this, we mask all the top ones, and then count what remains
        let id_mask = !(!0 << id);
        (self.0 & id_mask).count_ones() as usize
    }

    fn contains(&self, a: Action) -> bool {
        let id = a.0;
        self.0 & (1 << id) > 0
    }

    fn insert(&mut self, a: Action) {
        let id = a.0;
        self.0 |= 1 << id;
    }

    fn len(&self) -> usize {
        self.0.count_ones() as usize
    }
}

#[cfg(test)]
mod tests {
    use crate::game::Action;

    use super::ActionList;

    #[test]
    fn test_action_list() {
        let mut list = ActionList::default();

        assert_eq!(list.len(), 0);

        list.insert(Action(1));

        assert!(list.contains(Action(1)));
        assert_eq!(list.len(), 1);
        assert_eq!(list.index(Action(1)), 0);

        list.insert(Action(0));
        assert!(list.contains(Action(1)));
        assert_eq!(list.len(), 2);
        assert_eq!(list.index(Action(1)), 1);
        assert_eq!(list.index(Action(1)), 1);
    }
}
