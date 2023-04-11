use crate::{
    database::{file_backend::FileBackend, FileNodeStore},
    game::{Action, Game, GameState},
};

/// Populates a nodestore to always pick a given action index
pub fn populate_ns<T: GameState>(ns: &mut FileNodeStore<FileBackend>, g: Game<T>, action: Action) {
    // todo -- implement a memory nodestore -- can switch all the tests to use it where appropriate. Just a hashmap
}
