use std::collections::HashSet;

use itertools::Itertools;

use crate::{istate::IStateKey, pool::Pool, Action, GameState};

/// Naively iterate over all possible non-terminal and non-chance istates
pub struct IStateIterator {
    istates: Vec<IStateKey>,
}

impl IStateIterator {
    /// Create a new istate iterator that iterates over all possible children istates
    pub fn new<G: GameState>(mut gs: G) -> Self {
        let mut pool = Pool::new(Vec::new);
        let mut istates = HashSet::new();
        walk_istates(&mut gs, &mut istates, &mut pool);

        Self {
            istates: istates.into_iter().collect_vec(),
        }
    }
}

impl Iterator for IStateIterator {
    type Item = IStateKey;

    fn next(&mut self) -> Option<Self::Item> {
        self.istates.pop()
    }
}

fn walk_istates<G: GameState>(
    gs: &mut G,
    istates: &mut HashSet<IStateKey>,
    pool: &mut Pool<Vec<Action>>,
) {
    let mut actions = pool.detach();
    gs.legal_actions(&mut actions);
    for a in &actions {
        gs.apply_action(*a);
        if !gs.is_terminal() && !gs.is_chance_node() {
            let istate = gs.istate_key(gs.cur_player());
            istates.insert(istate);
        }
        walk_istates(gs, istates, pool);
        gs.undo()
    }

    actions.clear();
    pool.attach(actions);
}

#[cfg(test)]
mod tests {
    use crate::{
        gamestates::{
            bluff::Bluff,
            kuhn_poker::{KPAction, KuhnPoker},
        },
        translate_istate,
    };

    use super::*;

    #[test]
    fn test_kuhn_poker_iterator() {
        let gs = KuhnPoker::new_state();
        let mut istates = IStateIterator::new(gs).collect_vec();
        // 3 cards and p, pb, b + 3 cards on their own
        assert_eq!(
            istates.len(),
            12,
            "{:?}",
            istates
                .iter()
                .map(|x| translate_istate!(x, KPAction))
                .collect_vec()
        );
        istates.sort();
        istates.dedup();
        assert_eq!(istates.len(), 12);
    }

    #[test]
    fn test_bluff_iterator() {
        let gs = Bluff::new_state(1, 1);
        let mut istates = IStateIterator::new(gs).collect_vec();
        assert_eq!(istates.len(), 6144,);
        istates.sort();
        istates.dedup();
        assert_eq!(istates.len(), 6144);
    }
}
