//! Settlers of Catan web server. Mirrors the structure of
//! `oh_hell_server`: an Actix-web app that renders Maud HTML and uses
//! htmx for polling + posting actions. Bot seats are driven by PIMCTS
//! with random-rollout evaluation — no pre-trained weights required.

use std::{collections::HashMap, fs::OpenOptions, sync::Mutex};

use actix_web::{middleware::Logger, web, App, HttpResponse, HttpServer};
use card_platypus::{
    agents::Agent,
    algorithms::{ismcts::RandomRolloutEvaluator, pimcts::PIMCTSBot},
};
use games::{actions, gamestates::catan::CatanGameState, Action, GameState};
use log::{info, set_max_level, LevelFilter};
use rand::{rng, rngs::StdRng, seq::IndexedRandom, SeedableRng};
use simplelog::{
    ColorChoice, CombinedLogger, ConfigBuilder, TermLogger, TerminalMode, WriteLogger,
};
use uuid::Uuid;

mod game_data;
mod html;

pub(crate) use game_data::{GameData, GameProcessingState};

const SERVER_HOST: &str = "0.0.0.0";
const SERVER_PORT: u16 = 4002;
const LOG_FILE: &str = "catan_server.log";

/// Player counts this binary serves.
pub(crate) const SUPPORTED_PLAYERS: [usize; 3] = [2, 3, 4];
pub(crate) const DEFAULT_PLAYERS: usize = 4;
/// Maximum human seats per game; the rest are bot-controlled. A game may
/// be all-human (4 players, 4 humans = no bots).
pub(crate) const MAX_HUMANS: usize = 4;

/// PIMCTS worlds sampled per bot decision and random rollouts per world.
/// Measured end-to-end on the deploy-shaped build: full bot turns complete
/// in well under a second at these settings (rollouts are ~0.5ms and run
/// rayon-parallel across worlds).
const BOT_WORLDS: usize = 20;
const BOT_EVAL_ROLLOUTS: usize = 3;

/// The serving bot. `Random` exists for the state-machine fuzz tests,
/// where PIMCTS would be needlessly slow.
#[allow(dead_code)]
pub(crate) enum Bot {
    Pimcts(PIMCTSBot<CatanGameState, RandomRolloutEvaluator>),
    Random(StdRng),
}

impl Bot {
    pub(crate) fn production() -> Self {
        Bot::Pimcts(PIMCTSBot::new(
            BOT_WORLDS,
            RandomRolloutEvaluator::new(BOT_EVAL_ROLLOUTS),
            StdRng::from_rng(&mut rng()),
        ))
    }

    #[allow(dead_code)]
    pub(crate) fn random() -> Self {
        Bot::Random(StdRng::from_rng(&mut rng()))
    }

    pub(crate) fn step(&mut self, gs: &CatanGameState) -> Action {
        match self {
            Bot::Pimcts(agent) => agent.step(gs),
            Bot::Random(rng) => *actions!(gs).choose(rng).unwrap(),
        }
    }
}

pub(crate) struct AppState {
    pub(crate) games: Mutex<HashMap<Uuid, GameData>>,
    pub(crate) bot: Mutex<Bot>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            games: Default::default(),
            bot: Mutex::new(Bot::production()),
        }
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

    // cur_player covers the discard phase, where the acting player is not
    // the turn player.
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
    let slot = game_data
        .players
        .iter()
        .position(|x| x.is_none())
        .expect("must have free seat when humans < num_humans");
    game_data.players[slot] = Some(player_id);
    Ok(())
}

/// Drive the state machine forward, applying bot and chance moves, until
/// we land in a state that requires human input (or the game ends).
pub(crate) fn progress_game(game_data: &mut GameData, bot: &Mutex<Bot>, game_id: &Uuid) {
    use GameProcessingState::*;

    loop {
        let new_state = match &game_data.display_state {
            WaitingPlayerJoin { min_players } => {
                if game_data.players.iter().filter(|x| x.is_some()).count() < *min_players {
                    WaitingPlayerJoin {
                        min_players: *min_players,
                    }
                } else {
                    next_seat_state(game_data)
                }
            }
            WaitingHumanMove | WaitingMachineMoves => next_seat_state(game_data),
            GameOver => GameOver,
        };
        game_data.display_state = new_state;

        if matches!(game_data.display_state, GameOver) {
            let np = game_data.gs.num_players();
            let vps: Vec<u8> = (0..np).map(|p| game_data.gs.victory_points(p)).collect();
            info!(
                "game over|id|{}|vp|{:?}|players|{:?}",
                game_id, vps, game_data.players
            );
        }
        if !matches!(game_data.display_state, WaitingMachineMoves) {
            break;
        }

        // Machine's move: chance nodes (dice, card draws, steals) resolve
        // randomly; bot seats consult the agent.
        if game_data.gs.is_chance_node() {
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

/// Pick a state based on whose turn it is.
fn next_seat_state(game_data: &GameData) -> GameProcessingState {
    use GameProcessingState::*;
    let gs = &game_data.gs;
    if gs.is_terminal() {
        return GameOver;
    }
    if gs.is_chance_node() {
        return WaitingMachineMoves;
    }
    match game_data.players[gs.cur_player()] {
        Some(_) => WaitingHumanMove,
        None => WaitingMachineMoves,
    }
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

    info!("starting catan_server on {}:{}", SERVER_HOST, SERVER_PORT);
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
    //! same code paths the HTTP handlers use, rendering every state. Uses
    //! the random bot — these tests target the server state machine and
    //! renderer, not bot strength (PIMCTS is covered in card_platypus).
    use std::sync::Mutex;

    use games::{gamestates::catan::Catan, GameState};
    use uuid::Uuid;

    use crate::{
        handle_take_action, html::render_game_view, progress_game, Bot, GameData,
        GameProcessingState,
    };

    fn play_random_game(bot: &Mutex<Bot>, human_id: usize, num_players: usize) {
        let game_id = Uuid::new_v4();
        let mut gd = GameData::new(Catan::new_state(num_players), human_id, 1, num_players);
        progress_game(&mut gd, bot, &game_id);

        for _ in 0..40_000 {
            // Rendering runs on every HTTP response — include it (for both
            // a seated player and a spectator) so renderer panics surface.
            let _ = render_game_view(&gd, human_id, &game_id).into_string();
            let _ = render_game_view(&gd, usize::MAX, &game_id).into_string();

            match &gd.display_state {
                GameProcessingState::WaitingHumanMove => {
                    let mut legal = Vec::new();
                    gd.gs.legal_actions(&mut legal);
                    assert!(!legal.is_empty(), "no legal actions for human turn");
                    let a = legal[rand::random::<u32>() as usize % legal.len()];
                    handle_take_action(&mut gd, a, human_id).expect("take action");
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
    fn random_play_does_not_panic_all_player_counts() {
        let bot = Mutex::new(Bot::random());
        for num_players in [2, 3, 4] {
            for _ in 0..3 {
                play_random_game(&bot, 7, num_players);
            }
        }
        assert!(!bot.is_poisoned(), "bot mutex got poisoned during fuzz");
    }
}
