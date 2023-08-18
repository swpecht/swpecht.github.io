use std::{collections::HashMap, fs::OpenOptions, path::PathBuf, sync::Mutex};

use actix_files::NamedFile;
use actix_web::{
    get,
    middleware::Logger,
    post,
    web::{self, Json},
    App, HttpResponse, HttpServer, Responder,
};
use card_platypus::{
    actions,
    agents::Agent,
    cfragent::cfres::CFRES,
    game::{
        euchre::{Euchre, EuchreGameState},
        Action, GameState,
    },
};
use client_server_messages::{
    ActionRequest, GameData, GameProcessingState, NewGameRequest, NewGameResponse,
};
use log::{info, set_max_level, LevelFilter};
use rand::{rngs::StdRng, seq::SliceRandom, thread_rng, SeedableRng};
use simplelog::{
    ColorChoice, CombinedLogger, ConfigBuilder, TermLogger, TerminalMode, WriteLogger,
};
use uuid::Uuid;

struct AppState {
    games: Mutex<HashMap<Uuid, GameData>>,
    bot: Mutex<CFRES<EuchreGameState>>,
}

impl Default for AppState {
    fn default() -> Self {
        let mut bot = CFRES::new(
            || panic!("training not supported"),
            StdRng::from_rng(thread_rng()).unwrap(),
        );

        let n = bot.load("default.infostates");
        info!("loaded bot with {n} infostates");

        Self {
            games: Default::default(),
            bot: Mutex::new(bot),
        }
    }
}

#[post("/api")]
async fn api_index(json: Json<NewGameRequest>, data: web::Data<AppState>) -> impl Responder {
    let game_id = Uuid::new_v4();
    let gs = new_game();

    let game_date = GameData::new(gs, json.0.player_id);
    data.games.lock().unwrap().insert(game_id, game_date);

    info!("new game created");

    let response = NewGameResponse::new(game_id);

    HttpResponse::Ok().json(response)
}

#[get("/api/{game_id}")]
async fn get_game(path: web::Path<String>, data: web::Data<AppState>) -> impl Responder {
    let game_id_parse = Uuid::parse_str(path.into_inner().as_str());

    if game_id_parse.is_err() {
        return HttpResponse::BadRequest().finish();
    }

    let game_id = game_id_parse.unwrap();

    let games = data.games.lock().unwrap();
    if !games.contains_key(&game_id) {
        return HttpResponse::NotFound().finish();
    }

    let game_data = games.get(&game_id).unwrap();

    HttpResponse::Ok().json(game_data)
}

#[post("/api/{game_id}")]
async fn post_game(
    req: web::Json<ActionRequest>,
    path: web::Path<String>,
    data: web::Data<AppState>,
) -> impl Responder {
    info!("received request: {:?}", req);

    let game_id = match parse_game_id(path.into_inner().as_str()) {
        Ok(x) => x,
        Err(x) => return x,
    };

    let mut games = data.games.lock().unwrap();
    let game_data = match games.get_mut(&game_id) {
        Some(x) => x,
        None => return HttpResponse::NotFound().finish(),
    };

    // redo this to move the progress game call to the end of this call? Have functions all return results with the error
    use client_server_messages::GameAction::*;
    let result = match req.action {
        TakeAction(a) => handle_take_action(game_data, a, req.player_id),
        ReadyTrickClear | ReadyBidClear => handle_ready_clear(game_data, req.player_id),

        RegisterPlayer => handle_register_player(game_data, req.player_id),
    };

    if let Err(x) = result {
        return x;
    }

    progress_game(game_data, &data.bot);

    HttpResponse::Ok().json(&game_data)
}

fn handle_ready_clear(game_data: &mut GameData, player_id: usize) -> Result<(), HttpResponse> {
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

fn handle_take_action(
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

fn handle_register_player(game_data: &mut GameData, player_id: usize) -> Result<(), HttpResponse> {
    let num_humans = game_data.players.iter().flatten().count();
    if num_humans >= 2 {
        return Err(HttpResponse::BadRequest().body("game alrady has 2 human players"));
    }

    let cur_player_index = game_data
        .players
        .iter()
        .position(|x| x.is_some())
        .expect("error finding current player");
    game_data.players[(cur_player_index + 2) % 4] = Some(player_id);

    Ok(())
}

fn progress_game(game_data: &mut GameData, bot: &Mutex<CFRES<EuchreGameState>>) {
    let mut gs = EuchreGameState::from(game_data.gs.as_str());

    use GameProcessingState::*;
    // set the current state
    let num_humans = game_data.players.iter().flatten().count();

    loop {
        let new_state = match &game_data.display_state {
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
                        let human_team = game_data
                            .players
                            .iter()
                            .position(|x| x.is_some())
                            .expect("couldn't find human player");
                        game_data.human_score += gs.evaluate(human_team).max(0.0) as usize;
                        game_data.computer_score +=
                            gs.evaluate((human_team + 1) % 4).max(0.0) as usize;
                        info!(
                            "game ended\thuman score:\t{}\tgame:\t{}\thuman players:\t{}",
                            game_data.human_score,
                            gs,
                            game_data.players.iter().flatten().count()
                        );

                        gs = new_game();
                        // todo: change who dealer is
                        game_data.players.rotate_left(1);
                    }

                    if game_data.players[gs.cur_player()].is_none() {
                        WaitingMachineMoves
                    } else {
                        WaitingHumanMove
                    }
                } else {
                    game_data.display_state.clone()
                }
            }
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

fn parse_game_id(game_id: &str) -> Result<Uuid, HttpResponse> {
    let game_id_parse = Uuid::parse_str(game_id);

    if let Ok(uuid) = game_id_parse {
        Ok(uuid)
    } else {
        Err(HttpResponse::BadRequest().body("couldn't parse game id"))
    }
}

/// Returns the index page on not found
///
/// Necessary for dioxus to work
async fn not_found() -> actix_web::Result<NamedFile> {
    let path: PathBuf = "./static/index.html".parse().unwrap();
    Ok(NamedFile::open(path)?)
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
                .write(true)
                .create(true)
                .open("euchre_server.log")
                .unwrap(),
        ),
    ])
    .unwrap();

    info!("starting load of initial app state...");
    let app_state = web::Data::new(AppState::default());

    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .wrap(Logger::default())
            .service(api_index)
            .service(get_game)
            .service(post_game)
            // Need to register this last so other services are accessible
            .service(actix_files::Files::new("/", "./static").index_file("index.html"))
            .default_service(web::get().to(not_found))
    })
    .bind(("localhost", 4000))?
    .run()
    .await
}

fn new_game() -> EuchreGameState {
    let mut gs = Euchre::new_state();

    let mut actions = Vec::new();
    while gs.is_chance_node() {
        gs.legal_actions(&mut actions);
        let a = actions.choose(&mut thread_rng()).unwrap();
        gs.apply_action(*a);
    }

    gs
}

#[cfg(test)]
mod tests {
    use actix_web::{dev::ServiceResponse, test, web, App};
    use card_platypus::actions;
    use client_server_messages::GameAction;
    use serde::de::DeserializeOwned;

    use super::*;

    async fn deserialize_body<T: DeserializeOwned>(resp: ServiceResponse) -> T {
        let body = test::read_body(resp).await;
        serde_json::from_str(std::str::from_utf8(body.as_ref()).unwrap()).unwrap()
    }

    #[actix_web::test]
    async fn test_index_get() {
        let app_state = web::Data::new(AppState::default());

        let app = test::init_service(
            App::new()
                .app_data(app_state)
                .service(api_index)
                .service(get_game)
                .service(post_game),
        )
        .await;

        let game_request = NewGameRequest { player_id: 42 };
        let req = test::TestRequest::post()
            .uri("/")
            .set_json(game_request)
            .to_request();
        let resp = test::call_service(&app, req).await;

        let new_game: NewGameResponse = deserialize_body(resp).await;

        let req = test::TestRequest::default()
            .uri(format!("/{}", new_game.id).as_str())
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());

        let game_data: GameData = deserialize_body(resp).await;

        // try applying an action
        let gs = EuchreGameState::from(game_data.gs.as_str());
        let action = actions!(gs)[0];

        let req = test::TestRequest::post()
            .uri(format!("/{}", new_game.id).as_str())
            .set_json(ActionRequest {
                player_id: 42,
                action: GameAction::TakeAction(action),
            })
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let _game_data: GameData = deserialize_body(resp).await;

        // check that get works as well
        let req = test::TestRequest::default()
            .uri(format!("/{}", new_game.id).as_str())
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let _game_data: GameData = deserialize_body(resp).await;
    }
}
