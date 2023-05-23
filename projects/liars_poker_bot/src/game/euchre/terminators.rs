use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::{algorithms::open_hand_solver::Terminator, game::Action};

use super::{actions::Suit, deck::Deck, EuchreGameState};

type TranspositionKey = (Suit, Deck);
type AlphaBetaResult = (f64, Option<Action>);

#[derive(Default)]
pub struct EuchreTranspositionTable {
    data: Arc<Mutex<HashMap<TranspositionKey, AlphaBetaResult>>>,
}

impl Terminator<EuchreGameState> for EuchreTranspositionTable {
    fn evaluate(&mut self, gs: &mut EuchreGameState) {
        todo!()
    }
}

#[derive(Default)]
pub struct EuchreEarlyEnd {}

impl Terminator<EuchreGameState> for EuchreEarlyEnd {
    fn evaluate(&mut self, gs: &mut EuchreGameState) {
        todo!()
    }
}
