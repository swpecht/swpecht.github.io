//! Oh Hell web server. Mirrors the structure of `euchre_server`: an
//! Actix-web app that renders Maud HTML and uses htmx for polling +
//! form submission. The bot is a PIMCTS + open-hand-solver agent — no
//! pre-trained weights are required.

use std::{collections::HashMap, fs::OpenOptions, sync::Mutex};

use actix_web::{middleware::Logger, web, App, HttpResponse, HttpServer};
use card_platypus::{
    agents::Agent,
    algorithms::{open_hand_solver::OpenHandSolver, pimcts::PIMCTSBot},
};
use games::{
    actions,
    gamestates::oh_hell::{OHPhase, OhHell, OhHellGameState},
    Action, GameState,
};
use log::{info, set_max_level, LevelFilter};
use rand::{rng, rngs::StdRng, SeedableRng};
use simplelog::{
    ColorChoice, CombinedLogger, ConfigBuilder, TermLogger, TerminalMode, WriteLogger,
};
use uuid::Uuid;

mod game_data;
mod html;

pub(crate) use game_data::{GameData, GameProcessingState};

const SERVER_HOST: &str = "0.0.0.0";
const SERVER_PORT: u16 = 4001;
const LOG_FILE: &str = "oh_hell_server.log";

/// Game shape served by this binary: three players. With 3 players ×
/// 10-card hands × 2 hands per ascending/descending step + the face-up
/// card, the deal fits within the 52-card deck.
pub(crate) const NUM_PLAYERS: usize = 3;
/// PIMCTS rollout count per bot decision. Small enough to be quick on
/// every move yet large enough that the bot looks competent.
const BOT_ROLLOUTS: usize = 30;

/// The canonical Wikipedia hand-size schedule: deal 10 cards each
/// hand, decrement to 1, then ascend back to 10. 19 hands total. The
/// final cumulative score (under "common scoring") determines the
/// winner.
pub fn default_hand_sequence() -> Vec<usize> {
    let mut seq: Vec<usize> = (1..=10).rev().collect();
    seq.extend(2..=10);
    seq
}

/// Look up the bot strategy in use for a given hand size. PIMCTS for
/// every size today; this list will grow as CFRES weights for specific
/// hand sizes get trained and wired in.
pub fn strategy_for_hand_size(_n_tricks: usize) -> &'static str {
    "PIMCTS"
}

pub(crate) type Bot = PIMCTSBot<OhHellGameState, OpenHandSolver<OhHellGameState>>;

pub(crate) struct AppState {
    pub(crate) games: Mutex<HashMap<Uuid, GameData>>,
    pub(crate) bot: Mutex<Bot>,
}

impl Default for AppState {
    fn default() -> Self {
        let bot = PIMCTSBot::new(
            BOT_ROLLOUTS,
            OpenHandSolver::new_oh_hell(),
            StdRng::from_rng(&mut rng()),
        );
        Self {
            games: Default::default(),
            bot: Mutex::new(bot),
        }
    }
}

pub(crate) fn handle_ready_clear(
    game_data: &mut GameData,
    player_id: usize,
) -> Result<(), HttpResponse> {
    match &mut game_data.display_state {
        GameProcessingState::WaitingTrickClear { ready_players }
        | GameProcessingState::WaitingBidClear { ready_players }
        | GameProcessingState::WaitingHandClear { ready_players } => {
            if !ready_players.contains(&player_id) {
                ready_players.push(player_id);
            }
            Ok(())
        }
        _ => Err(HttpResponse::BadRequest().body(format!(
            "can't ready to clear in current state: {:?}",
            game_data.display_state
        ))),
    }
}

pub(crate) fn handle_take_action(
    game_data: &mut GameData,
    a: Action,
    player_id: usize,
) -> Result<(), HttpResponse> {
    if !matches!(
        game_data.display_state,
        GameProcessingState::WaitingHumanMove
    ) {
        return Err(HttpResponse::BadRequest().body(format!(
            "cannot take action in current state: {:?}",
            game_data.display_state
        )));
    }

    let legal = actions!(game_data.gs);
    if !legal.contains(&a) {
        return Err(HttpResponse::BadRequest().body("illegal action attempted"));
    }

    let seat = match game_data
        .players
        .iter()
        .position(|x| *x == Some(player_id))
    {
        Some(x) => x,
        None => {
            return Err(HttpResponse::BadRequest()
                .body("attempted to make a move for a player not registered to this game"))
        }
    };

    if game_data.gs.cur_player() != seat {
        return Err(HttpResponse::BadRequest().body(format!(
            "attempted action on wrong players turn. Current player is: {}",
            game_data.gs.cur_player(),
        )));
    }

    game_data.gs.apply_action(a);
    Ok(())
}

pub(crate) fn handle_register_player(
    game_data: &mut GameData,
    player_id: usize,
) -> Result<(), HttpResponse> {
    if game_data.players.contains(&Some(player_id)) {
        return Ok(());
    }
    let humans = game_data.players.iter().flatten().count();
    if humans >= game_data.num_humans {
        return Err(HttpResponse::Forbidden().body("game already has all human seats filled"));
    }
    // Drop the new human into the first empty seat. Other seats stay
    // bot-controlled. With NUM_PLAYERS=3 and num_humans=2 this gives
    // seats [Some(creator), Some(joiner), None] — two humans across
    // from one bot.
    let slot = game_data
        .players
        .iter()
        .position(|x| x.is_none())
        .expect("must have free seat when humans < num_humans");
    game_data.players[slot] = Some(player_id);
    Ok(())
}

/// Drive the state machine forward, applying bot moves as needed, until
/// we land in a state that requires user input (or the game ends).
pub(crate) fn progress_game(
    game_data: &mut GameData,
    bot: &Mutex<Bot>,
    game_id: &Uuid,
) {
    use GameProcessingState::*;

    loop {
        let new_state = match &game_data.display_state {
            WaitingPlayerJoin { min_players } => {
                if game_data.players.iter().filter(|x| x.is_some()).count() < *min_players {
                    WaitingPlayerJoin {
                        min_players: *min_players,
                    }
                } else {
                    advance_state_after_action(game_data)
                }
            }
            WaitingHumanMove | WaitingMachineMoves => advance_state_after_action(game_data),
            WaitingBidClear { ready_players }
            | WaitingTrickClear { ready_players }
            | WaitingHandClear { ready_players } => {
                let humans = game_data.players.iter().flatten().count();
                if ready_players.len() < humans {
                    game_data.display_state.clone()
                } else if matches!(game_data.display_state, WaitingHandClear { .. }) {
                    // Finalise scores for the just-finished hand, then
                    // start the next hand from the schedule. If we've
                    // played the last hand in the schedule, the game is
                    // over and the highest cumulative score wins.
                    finalise_hand(game_data, game_id);
                    game_data.hand_idx += 1;
                    if game_data.hand_idx >= game_data.hand_sequence.len() {
                        info!(
                            "game over|id|{}|scores|{:?}|players|{:?}",
                            game_id, game_data.scores, game_data.players
                        );
                        GameOver
                    } else {
                        let next_size = game_data.hand_sequence[game_data.hand_idx];
                        game_data.gs = new_hand(next_size);
                        next_seat_state(game_data)
                    }
                } else {
                    // BidClear / TrickClear cleared: don't re-enter the
                    // detection paths in advance_state_after_action,
                    // they'd flip us right back. Just hand off to whichever
                    // seat is next.
                    next_seat_state(game_data)
                }
            }
            GameOver => GameOver,
        };
        game_data.display_state = new_state;

        if !matches!(game_data.display_state, WaitingMachineMoves) {
            break;
        }

        // Bot's turn. Drive the chance phases too — a fresh hand starts
        // in DealHands which is technically a chance node, not the
        // bot's turn, but it's not the human's either.
        if game_data.gs.is_chance_node() {
            use rand::seq::IndexedRandom;
            let mut acts = Vec::new();
            game_data.gs.legal_actions(&mut acts);
            let a = *acts.choose(&mut rng()).unwrap();
            game_data.gs.apply_action(a);
        } else {
            let mut agent = bot.lock().unwrap();
            let a = agent.step(&game_data.gs);
            game_data.gs.apply_action(a);
        }
    }
}

/// Pick the next processing state given that the gamestate just had an
/// action applied to it. Detects bid-completion, trick-completion, and
/// hand-completion to pause for the UI.
fn advance_state_after_action(game_data: &GameData) -> GameProcessingState {
    use GameProcessingState::*;
    let gs = &game_data.gs;

    if gs.is_terminal() {
        return WaitingHandClear {
            ready_players: vec![],
        };
    }
    if gs.is_trick_over() {
        return WaitingTrickClear {
            ready_players: vec![],
        };
    }
    // Show bids once, when bidding has just finished and we're about to
    // start play. Detect: phase==Play AND no cards played yet.
    if gs.phase() == OHPhase::Play && gs.cards_played() == 0 {
        return WaitingBidClear {
            ready_players: vec![],
        };
    }
    next_seat_state(game_data)
}

/// Pick a state based purely on whose turn it is (no clear-state
/// detection). Used after a clear has been acknowledged or a fresh hand
/// has begun — those cases would otherwise re-fire the trick/bid
/// detection logic above.
fn next_seat_state(game_data: &GameData) -> GameProcessingState {
    use GameProcessingState::*;
    let gs = &game_data.gs;
    if gs.is_chance_node() {
        return WaitingMachineMoves;
    }
    match game_data.players[gs.cur_player()] {
        Some(_) => WaitingHumanMove,
        None => WaitingMachineMoves,
    }
}

/// At end-of-hand, add per-seat raw scores (common scoring: 1 point per
/// trick + 10 bonus if the bid matched exactly) to the cumulative
/// running totals. Delegates the formula to `OhHellGameState::raw_scores`
/// so the server and game state stay in sync.
fn finalise_hand(game_data: &mut GameData, game_id: &Uuid) {
    let gs = &game_data.gs;
    let np = gs.num_players();
    let raw = gs.raw_scores();
    for p in 0..np {
        game_data.scores[p] += raw[p] as usize;
    }
    info!(
        "hand ended|id|{}|bids|{:?}|tricks|{:?}|raw|{:?}|cumulative|{:?}|players|{:?}",
        game_id,
        gs.bids(),
        gs.tricks_won(),
        &raw[..np],
        game_data.scores,
        game_data.players
    );
}

pub(crate) fn new_hand(n_tricks: usize) -> OhHellGameState {
    OhHell::new_state(NUM_PLAYERS, n_tricks)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    set_max_level(LevelFilter::Trace);
    let config = ConfigBuilder::new().set_time_format_rfc3339().build();

    CombinedLogger::init(vec![
        TermLogger::new(
            LevelFilter::Debug,
            config.clone(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ),
        WriteLogger::new(
            LevelFilter::Info,
            config,
            OpenOptions::new()
                .append(true)
                .create(true)
                .open(LOG_FILE)
                .expect("failed to open log file for writing"),
        ),
    ])
    .expect("failed to initialize logger");

    info!("starting oh_hell_server on {}:{}", SERVER_HOST, SERVER_PORT);
    let app_state = web::Data::new(AppState::default());

    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .wrap(Logger::default())
            .configure(html::configure)
    })
    .bind((SERVER_HOST, SERVER_PORT))?
    .run()
    .await
}

#[cfg(test)]
mod tests {
    //! Fuzz-style integration test: simulate full games end-to-end via the
    //! same code paths the HTTP handlers use. Catches state-machine bugs
    //! before they hit production.
    use std::sync::Mutex;

    use card_platypus::algorithms::{open_hand_solver::OpenHandSolver, pimcts::PIMCTSBot};
    use games::GameState;
    use rand::{rng, rngs::StdRng, SeedableRng};
    use uuid::Uuid;

    use crate::{
        handle_ready_clear, handle_take_action, html::render_game_view, new_hand, progress_game,
        Bot, GameData, GameProcessingState,
    };

    fn make_test_bot() -> Mutex<Bot> {
        Mutex::new(PIMCTSBot::new(
            // 1 rollout keeps the fuzz fast; the bot's policy quality is
            // not what we're testing here.
            1,
            OpenHandSolver::new_oh_hell(),
            StdRng::from_rng(&mut rng()),
        ))
    }

    /// Test-only hand sequence: 3 → 2 → 1 → 2 → 3. Mirrors the shape of
    /// the production schedule (descend-then-ascend) without burning
    /// the wall-clock budget on full 10-card hands × 19 rounds.
    fn test_hand_sequence() -> Vec<usize> {
        vec![3, 2, 1, 2, 3]
    }

    fn play_random_game(bot: &Mutex<Bot>, human_id: usize) {
        let game_id = Uuid::new_v4();
        let sequence = test_hand_sequence();
        let first_size = sequence[0];
        let mut gd = GameData::new(
            new_hand(first_size),
            human_id,
            1,
            crate::NUM_PLAYERS,
            sequence,
        );
        progress_game(&mut gd, bot, &game_id);

        for _ in 0..6000 {
            // Rendering runs on every HTTP response — include it so
            // "passes invalid input to renderer" bugs surface.
            let _ = render_game_view(&gd, human_id, &game_id).into_string();

            match &gd.display_state {
                GameProcessingState::WaitingHumanMove => {
                    let mut legal = Vec::new();
                    gd.gs.legal_actions(&mut legal);
                    assert!(!legal.is_empty(), "no legal actions for human turn");
                    let a = legal[rand::random::<u32>() as usize % legal.len()];
                    handle_take_action(&mut gd, a, human_id).expect("take action");
                }
                GameProcessingState::WaitingBidClear { .. }
                | GameProcessingState::WaitingTrickClear { .. }
                | GameProcessingState::WaitingHandClear { .. } => {
                    handle_ready_clear(&mut gd, human_id).expect("ready clear");
                }
                GameProcessingState::GameOver => return,
                GameProcessingState::WaitingMachineMoves
                | GameProcessingState::WaitingPlayerJoin { .. } => {}
            }
            progress_game(&mut gd, bot, &game_id);
        }
        panic!("game did not reach GameOver within iteration cap");
    }

    #[test]
    fn random_play_does_not_panic() {
        let bot = make_test_bot();
        for _ in 0..40 {
            play_random_game(&bot, 0);
        }
        assert!(!bot.is_poisoned(), "bot mutex got poisoned during fuzz");
    }
}
