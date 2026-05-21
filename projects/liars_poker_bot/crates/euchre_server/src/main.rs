use std::{
    collections::HashMap,
    env,
    fs::OpenOptions,
    path::PathBuf,
    str::FromStr,
    sync::Mutex,
};

use actix_web::{middleware::Logger, web, App, HttpResponse, HttpServer};
use card_platypus::{agents::Agent, algorithms::cfres::CFRES};
use games::{
    actions,
    gamestates::euchre::{Euchre, EuchreGameState},
    Action, GameState,
};
use log::{info, set_max_level, LevelFilter};
use rand::{rng, rngs::StdRng, seq::IndexedRandom, SeedableRng};
use simplelog::{
    ColorChoice, CombinedLogger, ConfigBuilder, TermLogger, TerminalMode, WriteLogger,
};
use uuid::Uuid;

mod bench;
mod game_data;
mod html;

pub(crate) use game_data::{GameData, GameProcessingState};

const DEFAULT_WEIGHTS_PATH: &str = "/home/steven/card_platypus/infostate.three_card_played_f32";
const WEIGHTS_PATH_ENV: &str = "EUCHRE_WEIGHTS_PATH";
const MAX_CARDS_PLAYED: usize = 3;
const SERVER_HOST: &str = "0.0.0.0";
const SERVER_PORT: u16 = 4000;
pub(crate) const WIN_SCORE: usize = 10;
const LOG_FILE: &str = "euchre_server.log";

/// Shared application state protected by mutexes.
///
/// Note on `.lock().unwrap()`: We intentionally unwrap mutex locks throughout this module.
/// A poisoned mutex indicates a prior panic while the lock was held, which means the
/// application state may be corrupt. In that case, panicking is the correct behavior
/// rather than attempting to recover from potentially inconsistent state.
pub(crate) struct AppState {
    pub(crate) games: Mutex<HashMap<Uuid, GameData>>,
    pub(crate) bot: Mutex<CFRES<EuchreGameState>>,
    pub(crate) bench: bench::BenchState,
}

impl Default for AppState {
    fn default() -> Self {
        let weights_path: PathBuf = env::var(WEIGHTS_PATH_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(DEFAULT_WEIGHTS_PATH));
        info!("loading weights from {}", weights_path.display());

        let bot = CFRES::new_euchre(
            StdRng::from_rng(&mut rng()),
            MAX_CARDS_PLAYED,
            Some(weights_path.as_path()),
        );

        let n = bot.num_info_states();
        info!(
            "loaded bot with {n} infostates and {MAX_CARDS_PLAYED} max cards played"
        );

        let games: Mutex<HashMap<Uuid, GameData>> = Default::default();
        let pick_suit_game = GameData {
            gs: "AsJhJdQdAd|QcTs9h9dTd|TcKcJsQsKd|9sKsThQhAh|9c|PPPP".to_string(),
            players: vec![Some(0), None, Some(42), None],
            human_score: 2,
            computer_score: 0,
            display_state: GameProcessingState::WaitingHumanMove,
        };
        games.lock().unwrap().insert(
            Uuid::from_str("e8aa648a-9483-4bcf-8f81-292222a30557").unwrap(),
            pick_suit_game,
        );

        info!("loaded debugging gamestates: {:?}", games.lock().unwrap());

        let bench = bench::BenchState {
            agents: bench::load_bench_agents(),
            ..Default::default()
        };

        Self {
            games,
            bot: Mutex::new(bot),
            bench,
        }
    }
}

pub(crate) fn handle_ready_clear(
    game_data: &mut GameData,
    player_id: usize,
) -> Result<(), HttpResponse> {
    match &mut game_data.display_state {
        GameProcessingState::WaitingTrickClear { ready_players }
        | GameProcessingState::WaitingBidClear { ready_players } => {
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

    let mut gs = EuchreGameState::from(game_data.gs.as_str());

    let legal_actions = actions!(gs);
    if !legal_actions.contains(&a) {
        return Err(HttpResponse::BadRequest().body("illegal action attempted"));
    }

    let player = match game_data
        .players
        .iter()
        .position(|x| x.is_some() && x.unwrap() == player_id)
    {
        Some(x) => x,
        None => {
            return Err(HttpResponse::BadRequest()
                .body("attempted to make a move for a player not registered to this game"))
        }
    };

    if gs.cur_player() != player {
        return Err(HttpResponse::BadRequest().body(format!(
            "attempted action on wrong players turn. Current player is: {}.\n request: {:?}\ngs: {}",
            gs.cur_player(),
            a, gs
        )));
    }

    gs.apply_action(a);
    game_data.gs = gs.to_string();

    Ok(())
}

pub(crate) fn handle_register_player(
    game_data: &mut GameData,
    player_id: usize,
) -> Result<(), HttpResponse> {
    let num_humans = game_data.players.iter().flatten().count();
    if num_humans >= 2 {
        return Err(HttpResponse::Forbidden().body("game already has 2 human players"));
    }

    let cur_player_index = match game_data.players.iter().position(|x| x.is_some()) {
        Some(idx) => idx,
        None => {
            return Err(HttpResponse::InternalServerError()
                .body("error finding current player: no human player registered"))
        }
    };
    game_data.players[(cur_player_index + 2) % 4] = Some(player_id);

    Ok(())
}

pub(crate) fn progress_game(
    game_data: &mut GameData,
    bot: &Mutex<CFRES<EuchreGameState>>,
    game_id: &Uuid,
) {
    let mut gs = EuchreGameState::from(game_data.gs.as_str());

    use GameProcessingState::*;
    // set the current state
    let num_humans = game_data.players.iter().flatten().count();

    loop {
        let new_state = match &game_data.display_state {
            WaitingPlayerJoin { min_players } => {
                if game_data.players.iter().filter(|x| x.is_some()).count() < *min_players {
                    WaitingPlayerJoin {
                        min_players: *min_players,
                    }
                } else {
                    match game_data.players[gs.cur_player()] {
                        Some(_) => WaitingHumanMove,
                        None => WaitingMachineMoves,
                    }
                }
            }
            WaitingHumanMove | WaitingMachineMoves => {
                if gs.is_trick_over() {
                    WaitingTrickClear {
                        ready_players: vec![],
                    }
                } else if gs.play_phase_entered() {
                    // Single pause at the bidding→play boundary so humans
                    // see who called trump / who's going alone before cards
                    // start flying. (Previously bidding_ended() also fired
                    // inside the Discard and Alone sub-phases, forcing the
                    // user to click Continue 2–3 times in a row.)
                    WaitingBidClear {
                        ready_players: vec![],
                    }
                } else if game_data.players[gs.cur_player()].is_none()
                    || gs.sitting_out_player() == Some(gs.cur_player())
                {
                    // Sitting-out partner's "turn" is just the Pass sentinel —
                    // no human input is meaningful. Route it through the bot
                    // path so the only legal action (Pass) gets applied
                    // automatically. Without this, the renderer falls through
                    // to the regular-play branch and EAction::card() panics on
                    // the Pass variant, poisoning the per-app Mutex.
                    WaitingMachineMoves
                } else {
                    WaitingHumanMove
                }
            }
            WaitingTrickClear { ready_players } | WaitingBidClear { ready_players } => {
                if ready_players.len() == num_humans {
                    if gs.is_terminal() {
                        if let Some(human_team) =
                            game_data.players.iter().position(|x| x.is_some())
                        {
                            game_data.human_score +=
                                gs.evaluate(human_team).max(0.0) as usize;
                            game_data.computer_score +=
                                gs.evaluate((human_team + 1) % 4).max(0.0) as usize;
                        } else {
                            log::warn!(
                                "no human player found for game {game_id}, skipping score update"
                            );
                        }
                        info!(
                            "hand ended|id|{}|human:|{}|game:|{}|human players:|{}|player ids|{:?}",
                            game_id,
                            game_data.human_score,
                            gs,
                            game_data.players.iter().flatten().count(),
                            game_data.players,
                        );

                        gs = new_game();
                        game_data.players.rotate_left(1);
                    }

                    if game_data.human_score >= WIN_SCORE || game_data.computer_score >= WIN_SCORE {
                        info!(
                            "game over|id|{}|human:|{}|computer|{}|player ids|{:?}",
                            game_id,
                            game_data.human_score,
                            game_data.computer_score,
                            game_data.players
                        );
                        GameOver
                    } else if game_data.players[gs.cur_player()].is_none()
                        || gs.sitting_out_player() == Some(gs.cur_player())
                    {
                        WaitingMachineMoves
                    } else {
                        WaitingHumanMove
                    }
                } else {
                    game_data.display_state.clone()
                }
            }
            // this is a terminal state
            GameOver => GameOver,
        };
        game_data.display_state = new_state;

        if !matches!(game_data.display_state, WaitingMachineMoves) {
            break;
        }

        // Apply bot actions for all non players
        let mut agent = bot.lock().unwrap();

        let a = agent.step(&gs);
        gs.apply_action(a);
    }

    game_data.gs = gs.to_string();
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

    info!("starting load of initial app state...");
    let app_state = web::Data::new(AppState::default());

    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .wrap(Logger::default())
            .configure(bench::configure)
            .configure(html::configure)
    })
    .bind((SERVER_HOST, SERVER_PORT))?
    .run()
    .await
}

pub(crate) fn new_game() -> EuchreGameState {
    let mut gs = Euchre::new_state();

    let mut actions = Vec::new();
    while gs.is_chance_node() {
        gs.legal_actions(&mut actions);
        let a = actions.choose(&mut rng()).unwrap();
        gs.apply_action(*a);
    }

    gs
}

#[cfg(test)]
mod tests {
    //! Fuzz-style integration test that simulates a bunch of games end-to-end
    //! using the same code paths the HTTP handlers do (handle_take_action,
    //! handle_ready_clear, progress_game, render_game_view). Goal: catch
    //! panics from rare gameplay paths — e.g. EAction::card() on a non-card
    //! variant — before they hit production and poison the per-app Mutex.
    //!
    //! Uses CFRES with no weights so the bot returns uniform random over
    //! legal actions. Weights aren't required to exercise the state-machine
    //! and rendering paths where panics tend to hide.
    use std::sync::Mutex;
    use uuid::Uuid;
    use rand::{rng, rngs::StdRng, SeedableRng};
    use games::GameState;
    use games::gamestates::euchre::EuchreGameState;
    use card_platypus::algorithms::cfres::CFRES;
    use crate::html::render_game_view;
    use crate::{
        GameData, GameProcessingState, MAX_CARDS_PLAYED, handle_ready_clear,
        handle_take_action, new_game, progress_game,
    };

    fn make_test_bot() -> Mutex<CFRES<EuchreGameState>> {
        // None path → in-memory NodeStore, uniform-random policy.
        Mutex::new(CFRES::new_euchre(
            StdRng::from_rng(&mut rng()),
            MAX_CARDS_PLAYED,
            None,
        ))
    }

    fn play_random_game(bot: &Mutex<CFRES<EuchreGameState>>, human_id: usize) {
        let game_id = Uuid::new_v4();
        let mut gd = GameData::new(new_game(), human_id, 1);
        progress_game(&mut gd, bot, &game_id);

        // Hard cap so a state-machine bug can't spin forever.
        for _ in 0..2000 {
            // Rendering runs on every HTTP response in production, so include
            // it in the loop — most "passes invalid input to renderer" bugs
            // surface here.
            let _ = render_game_view(&gd, human_id, &game_id).into_string();

            match &gd.display_state {
                GameProcessingState::WaitingHumanMove => {
                    let gs = gd.to_state();
                    let mut legal = Vec::new();
                    gs.legal_actions(&mut legal);
                    assert!(!legal.is_empty(), "no legal actions for human turn");
                    let a = legal[rand::random::<u32>() as usize % legal.len()];
                    handle_take_action(&mut gd, a, human_id).expect("take action");
                }
                GameProcessingState::WaitingTrickClear { .. }
                | GameProcessingState::WaitingBidClear { .. } => {
                    handle_ready_clear(&mut gd, human_id).expect("ready clear");
                }
                GameProcessingState::GameOver => return,
                // After progress_game, the state shouldn't be machine-waiting
                // or player-joining (min_players=1 and progress_game drives
                // bots in a loop), but tolerate them defensively.
                GameProcessingState::WaitingMachineMoves
                | GameProcessingState::WaitingPlayerJoin { .. } => {}
            }
            progress_game(&mut gd, bot, &game_id);
        }
        panic!("game did not reach GameOver within 2000 iterations");
    }

    #[test]
    fn random_play_does_not_panic() {
        let bot = make_test_bot();
        // 200 games × ~50 turns ≈ 10k random transitions per run. Catches
        // most state-machine corners without blowing test runtime.
        for _ in 0..200 {
            play_random_game(&bot, 0);
        }
        // If we got here without panicking, no path through random play
        // tripped a Mutex-poisoning panic.
        assert!(!bot.is_poisoned(), "bot mutex got poisoned during fuzz");
    }
}
