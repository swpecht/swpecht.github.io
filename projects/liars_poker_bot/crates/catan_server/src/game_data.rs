//! Per-game server-side state for the Catan web UI.
//!
//! Catan is one continuous game (no hand schedule like Oh Hell), so the
//! processing state machine is just: wait for humans to join, then
//! alternate human/machine moves until the game state is terminal.

use games::gamestates::catan::CatanGameState;

#[derive(Debug, Clone)]
pub enum GameProcessingState {
    /// Waiting until `min_players` humans have joined.
    WaitingPlayerJoin { min_players: usize },
    WaitingHumanMove,
    WaitingMachineMoves,
    GameOver,
}

/// One Catan game. `players` maps seat index → `Some(player_id)` for
/// humans, `None` for bot-controlled seats.
#[derive(Debug, Clone)]
pub struct GameData {
    pub gs: CatanGameState,
    pub players: Vec<Option<usize>>,
    pub display_state: GameProcessingState,
    /// Number of human seats this game is configured for.
    pub num_humans: usize,
}

impl GameData {
    pub fn new(
        gs: CatanGameState,
        player_id: usize,
        num_humans: usize,
        num_players: usize,
    ) -> Self {
        let mut players = vec![None; num_players];
        players[0] = Some(player_id);
        Self {
            gs,
            players,
            display_state: GameProcessingState::WaitingPlayerJoin {
                min_players: num_humans,
            },
            num_humans,
        }
    }
}
