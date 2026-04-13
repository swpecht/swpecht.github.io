//! Game state types used by the server. Previously lived in the
//! `client-server-messages` crate when the JSON API was shared with a
//! separate WASM frontend; now inlined since the server renders HTML.

use games::gamestates::euchre::EuchreGameState;

#[derive(Debug, Clone)]
pub enum GameProcessingState {
    /// When min_players has been specified but there aren't that many players in the game yet
    WaitingPlayerJoin { min_players: usize },
    WaitingHumanMove,
    WaitingMachineMoves,
    WaitingTrickClear { ready_players: Vec<usize> },
    WaitingBidClear { ready_players: Vec<usize> },
    GameOver,
}

#[derive(Debug, Clone)]
pub struct GameData {
    pub gs: String,
    pub players: Vec<Option<usize>>,
    pub human_score: usize,
    pub computer_score: usize,
    pub display_state: GameProcessingState,
}

impl GameData {
    pub fn new(gs: EuchreGameState, player_id: usize, min_players: usize) -> Self {
        Self {
            gs: gs.to_string(),
            players: vec![Some(player_id), None, None, None],
            human_score: 0,
            computer_score: 0,
            display_state: GameProcessingState::WaitingPlayerJoin { min_players },
        }
    }

    pub fn to_state(&self) -> EuchreGameState {
        EuchreGameState::from(self.gs.as_str())
    }
}
