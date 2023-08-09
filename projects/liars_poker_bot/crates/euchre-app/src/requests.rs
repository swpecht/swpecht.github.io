use client_server_messages::GameData;
use reqwest::{RequestBuilder, StatusCode};

use crate::in_game::InGameState;

pub async fn make_game_request(req: RequestBuilder) -> InGameState {
    match req.send().await {
        Ok(x) => match x.json::<GameData>().await {
            Ok(gd) => InGameState::Ok(gd),
            Err(x) if x.status().is_some() && x.status().unwrap() == StatusCode::NOT_FOUND => {
                InGameState::NotFound
            }
            Err(e) => InGameState::UnknownError(format!("error parsing json: {}", e)),
        },
        Err(x) if x.status().is_some() && x.status().unwrap() == StatusCode::NOT_FOUND => {
            InGameState::NotFound
        }
        Err(x) => InGameState::UnknownError(x.to_string()),
    }
}
