use tinyvec::ArrayVec;

use super::actions::{Card, EAction};

struct EuchreIState {
    face_up: EAction,
    hand: ArrayVec<[EAction; 5]>,
    bids: ArrayVec<[EAction; 8]>,
    is_discard: bool,
    plays: ArrayVec<[EAction; 4]>,
}

/// Do this is stages
///
/// Is there a way to see what the valid actions are for extending a given istate? We can use the logic to check if resampling is possible?
/// Use the "search for deal" function
/// Then we can just try all possible euchre actions and see which ones are valid?
///
/// for a in all_actions {
///     istate.append(a)
///     if istate.is_valid() { // re-sample logic
///         save(istate)
///         find_all(istate)
///     }
///     istate.pop(a)
/// }
///
/// Can use the EuchreIState function for this, rather than needing to construct a gamestate -- we can pull the constraints from it
/// should be pretty simple since only doing the first round of play
///
/// Start with an empty one, then slowly append -- might need to re-create some of the logic for phase changes, but should be minimal
///
/// Make a naive gamestate iterator -- for bluff, and kuhn poker, just go over all actions
/// Then make a euchre specific one that has all the optimizations, base it on validating istates using the re-sample logic
///
/// Consider re-writing based on the sudoku solver example -- constraint propogation
pub fn find_child_istates() {
    todo!()
}
