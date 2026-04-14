use std::{
    collections::HashMap,
    fs::OpenOptions,
    path::Path,
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

mod game_data;
mod html;

pub(crate) use game_data::{GameData, GameProcessingState};

const DEFAULT_WEIGHTS_PATH: &str = "/home/steven/card_platypus/infostate.three_card_played_f32";
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
}

impl Default for AppState {
    fn default() -> Self {
        let bot = CFRES::new_euchre(
            StdRng::from_rng(&mut rng()),
            MAX_CARDS_PLAYED,
            Some(Path::new(DEFAULT_WEIGHTS_PATH)),
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

        Self {
            games,
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
                } else if gs.bidding_ended() {
                    WaitingBidClear {
                        ready_players: vec![],
                    }
                } else if game_data.players[gs.cur_player()].is_none() {
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
                    } else if game_data.players[gs.cur_player()].is_none() {
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
mod tests {}
