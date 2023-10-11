use games::{gamestates::euchre::EuchreGameState, Action};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum GameProcessingState {
    WaitingHumanMove,
    WaitingMachineMoves,
    WaitingTrickClear { ready_players: Vec<usize> },
    WaitingBidClear { ready_players: Vec<usize> },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GameData {
    pub gs: String,
    pub players: Vec<Option<usize>>,
    pub human_score: usize,
    pub computer_score: usize,
    pub display_state: GameProcessingState,
}

impl GameData {
    pub fn new(gs: EuchreGameState, player_id: usize) -> Self {
        Self {
            gs: gs.to_string(),
            players: vec![Some(player_id), None, None, None],
            human_score: 0,
            computer_score: 0,
            display_state: GameProcessingState::WaitingHumanMove,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NewGameResponse {
    pub id: String,
}

impl NewGameResponse {
    pub fn new(id: Uuid) -> Self {
        Self { id: id.to_string() }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct NewGameRequest {
    pub player_id: usize,
}

impl NewGameRequest {
    pub fn new(player_id: usize) -> Self {
        Self { player_id }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum GameAction {
    TakeAction(Action),
    ReadyTrickClear,
    ReadyBidClear,
    RegisterPlayer,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ActionRequest {
    pub player_id: usize,
    pub action: GameAction,
}

impl ActionRequest {
    pub fn new(player_id: usize, action: GameAction) -> Self {
        Self { player_id, action }
    }
}
