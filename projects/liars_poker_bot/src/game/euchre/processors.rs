use crate::game::{euchre::EuchreGameState, Action};

/// Euchre specific processor for open hand solver
pub fn process_euchre_actions(gs: &EuchreGameState, actions: &mut Vec<Action>) {
    // if have the highest trump, remove all other actions
}
