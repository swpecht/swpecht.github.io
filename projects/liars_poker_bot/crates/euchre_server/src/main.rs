use std::{collections::HashMap, sync::Mutex};

use actix_web::{
    get, post,
    web::{self, Json},
    App, HttpResponse, HttpServer, Responder,
};
use card_platypus::{
    actions,
    game::{
        euchre::{Euchre, EuchreGameState},
        Action, GameState,
    },
};
use log::info;
use rand::{seq::SliceRandom, thread_rng};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Default)]
struct AppState {
    games: Mutex<HashMap<Uuid, GameData>>,
}

#[derive(Serialize, Deserialize, Debug)]
struct GameData {
    gs: String,
    player_one: usize,
    player_two: Option<usize>,
    human_score: usize,
    computer_score: usize,
}

impl GameData {
    pub fn new(gs: EuchreGameState, player_id: usize) -> Self {
        Self {
            gs: gs.to_string(),
            player_one: player_id,
            player_two: None,
            human_score: 0,
            computer_score: 0,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct NewGameResponse {
    id: String,
}

impl NewGameResponse {
    pub fn new(id: Uuid) -> Self {
        Self { id: id.to_string() }
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct NewGameRequest {
    player_id: usize,
}

#[derive(Serialize, Deserialize, Debug)]
struct ActionRequest {
    player: usize,
    action: Action,
}

#[post("/")]
async fn index(json: Json<NewGameRequest>, data: web::Data<AppState>) -> impl Responder {
    let game_id = Uuid::new_v4();
    let gs = new_game();

    let game_date = GameData::new(gs, json.0.player_id);
    data.games.lock().unwrap().insert(game_id, game_date);

    info!("new game created");

    let response = NewGameResponse::new(game_id);

    HttpResponse::Ok().json(response)
}

#[get("/{game_id}")]
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

#[post("/{game_id}")]
async fn post_game(
    req: web::Json<ActionRequest>,
    path: web::Path<String>,
    data: web::Data<AppState>,
) -> impl Responder {
    let game_id_parse = Uuid::parse_str(path.into_inner().as_str());

    if game_id_parse.is_err() {
        return HttpResponse::BadRequest().finish();
    }

    let game_id = game_id_parse.unwrap();

    let mut games = data.games.lock().unwrap();
    if !games.contains_key(&game_id) {
        return HttpResponse::NotFound().finish();
    }

    let game_data = games.get_mut(&game_id).unwrap();
    let mut gs = EuchreGameState::from(game_data.gs.as_str());

    let legal_actions = actions!(gs);
    if !legal_actions.contains(&req.action) {
        return HttpResponse::BadRequest().body("illegal action attempted");
    }

    if gs.cur_player() != req.player {
        return HttpResponse::BadRequest().body("attempted action on wrong players turn");
    }

    gs.apply_action(req.action);

    game_data.gs = gs.to_string();

    HttpResponse::Ok().json(game_data)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let app_state = web::Data::new(AppState::default());

    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .service(index)
            .service(get_game)
            .service(post_game)
    })
    .bind(("127.0.0.1", 4000))?
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
                .service(index)
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
        let mut gs = EuchreGameState::from(game_data.gs.as_str());
        let action = actions!(gs)[0];
        gs.apply_action(action);

        let req = test::TestRequest::post()
            .uri(format!("/{}", new_game.id).as_str())
            .set_json(ActionRequest { player: 0, action })
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let game_data: GameData = deserialize_body(resp).await;
        assert_eq!(game_data.gs, gs.to_string());

        // check that get works as well
        let req = test::TestRequest::default()
            .uri(format!("/{}", new_game.id).as_str())
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let game_data: GameData = deserialize_body(resp).await;
        assert_eq!(game_data.gs, gs.to_string());
    }
}
