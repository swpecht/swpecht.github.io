//! Count Euchre iso-canonical istates per (face_up, max_cards_played)
//! for use as Waugh-Euchre validation targets.

use games::gamestates::euchre::{actions::EAction, iterator::EuchreIsomorphicIStateIterator};

fn main() {
    for max_cards in [0, 1, 2] {
        let total: usize = [EAction::NS, EAction::TS, EAction::JS, EAction::QS, EAction::KS, EAction::AS]
            .iter()
            .map(|fu| {
                let n = EuchreIsomorphicIStateIterator::with_face_up(max_cards, &[*fu]).count();
                println!("max_cards={} face_up={:?}: {}", max_cards, fu, n);
                n
            })
            .sum();
        println!("max_cards={} TOTAL across 6 face_ups: {}", max_cards, total);
        println!();
    }
}
