use client_server_messages::GameData;

use reqwest::{RequestBuilder, StatusCode};

use crate::in_game::InGameState;

pub async fn make_game_request(req: RequestBuilder) -> InGameState {
    match req.send().await {
        Ok(res) => match res.status() {
            StatusCode::OK => match res.json::<GameData>().await {
                Ok(gd) => InGameState::Ok(gd),
                Err(x) =>
                     InGameState::UnknownError(format!("error parsing json response: {:?}", x)),

            },
            StatusCode::NOT_FOUND => InGameState::NotFound,
            StatusCode::FORBIDDEN => InGameState::UnknownError("failed to join game, there are already two other human players in the game. try starting a new game".to_string()),
            StatusCode::BAD_REQUEST => InGameState::UnknownError(format!("error joining game. the url may be incorrect. try going back and starting a new game: {:?}", res)),
            _ => InGameState::UnknownError(format!("error occured while updating game state: {:?}", res)),
        },
        Err(e) => InGameState::UnknownError(format!("encountered an unexpected error fetching the game state. try refreshing the page or checking tour internet, {:?}", e)),
    }
}
