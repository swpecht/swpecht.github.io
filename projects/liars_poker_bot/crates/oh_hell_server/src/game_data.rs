//! Per-game server-side state for the Oh Hell web UI.
//!
//! Models the same processing-state machine as the euchre server but
//! adapted for Oh Hell's multi-hand structure: scores are tracked per
//! seat (Oh Hell isn't a team game) and hands are replayed inside one
//! game until any player crosses `WIN_SCORE`.

use games::gamestates::oh_hell::OhHellGameState;

#[derive(Debug, Clone)]
pub enum GameProcessingState {
    /// Waiting until at least `min_players` humans have joined.
    WaitingPlayerJoin { min_players: usize },
    WaitingHumanMove,
    WaitingMachineMoves,
    WaitingTrickClear { ready_players: Vec<usize> },
    /// Shown once after every player has bid so humans can read the
    /// table's bids before cards start flying.
    WaitingBidClear { ready_players: Vec<usize> },
    /// Shown at end-of-hand so humans see the final tricks/scores before
    /// the next hand is dealt.
    WaitingHandClear { ready_players: Vec<usize> },
    GameOver,
}

/// One Oh Hell game: a sequence of hands deal-size-by-deal-size
/// following the Wikipedia descend-then-ascend schedule (10, 9, ..., 1,
/// 2, ..., 10). `gs` is the raw current hand's state. `players` maps
/// seat index → `Some(player_id)` for humans, `None` for bot-controlled
/// seats.
#[derive(Debug, Clone)]
pub struct GameData {
    pub gs: OhHellGameState,
    pub players: Vec<Option<usize>>,
    /// Cumulative raw scores across hands, one entry per seat (parallel
    /// to `players`). Each hand contributes per-trick points + a
    /// possible exact-bid bonus per the common-scoring rule.
    pub scores: Vec<usize>,
    pub display_state: GameProcessingState,
    /// Number of human seats this game is configured for.
    pub num_humans: usize,
    /// Pre-computed schedule of hand sizes for the entire game. Hand
    /// `hand_idx` uses `hand_sequence[hand_idx]` tricks.
    pub hand_sequence: Vec<usize>,
    /// Index of the currently-running hand inside `hand_sequence`.
    pub hand_idx: usize,
}

impl GameData {
    pub fn new(
        gs: OhHellGameState,
        player_id: usize,
        min_players: usize,
        num_players: usize,
        hand_sequence: Vec<usize>,
    ) -> Self {
        let mut players = vec![None; num_players];
        players[0] = Some(player_id);
        Self {
            gs,
            players,
            scores: vec![0; num_players],
            display_state: GameProcessingState::WaitingPlayerJoin { min_players },
            num_humans: min_players,
            hand_sequence,
            hand_idx: 0,
        }
    }
}
