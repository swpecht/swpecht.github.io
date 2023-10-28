use client_server_messages::{GameAction, GameData};
use dioxus::prelude::{use_coroutine_handle, use_shared_state, use_state, Scope};
use reqwest::{RequestBuilder, StatusCode};
use wasm_bindgen::prelude::*;
use web_sys::console::log;
use web_sys::{ErrorEvent, MessageEvent, WebSocket};

use crate::in_game::InGameState;
use crate::PlayerId;

// macro_rules! console_log {
//     ($($t:tt)*) => (log(&format_args!($($t)*).to_string()))
// }

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

pub fn set_up_ws(cx: &Scope) {
    let ws = WebSocket::new("ws://localhost:4000/ws/").unwrap();

    let state = use_state(cx, || InGameState::Loading);
    let player_id = use_shared_state::<PlayerId>(cx).unwrap().read().id;

    let action_task = use_coroutine_handle::<GameAction>(cx).expect("error getting action task");
    let on_msg_callback = Closure::<dyn FnMut(_)>::new(move |e: MessageEvent| {
        // action_task.send(GameAction::ReadyTrickClear);
        // let msg = e.data().dyn_into::<js_sys::JsString>();

        // let mut new_state: InGameState = match serde_json::from_str(msg.as_str()) {
        //     Ok(gd) => InGameState::Ok(gd),
        //     Err(_) => panic!("error parsing game data or invalid game data"),
        // };

        // // make sure we're an active player, and try to register as one if we can
        // new_state = match new_state {
        //     InGameState::Ok(gd) if gd.players.contains(&Some(player_id)) => InGameState::Ok(gd),
        //     InGameState::Ok(gd) if gd.players.len() < 2 => InGameState::Ok(gd),
        //     InGameState::Ok(_) => {
        //         // make_game_request(
        //         //     client
        //         //         .post(game_url.clone())
        //         //         .json(&ActionRequest::new(player_id, GameAction::RegisterPlayer)),
        //         // )
        //         // .await
        //         panic!("registering player not yet supported")
        //     }
        //     _ => new_state,
        // };

        // game_data.set(new_state);
    });
    ws.set_onmessage(Some(on_msg_callback.as_ref().unchecked_ref()));
    on_msg_callback.forget();

    let on_error_callback = Closure::<dyn FnMut(_)>::new(|e: ErrorEvent| {});
    ws.set_onerror(Some(on_error_callback.as_ref().unchecked_ref()));
    on_error_callback.forget();

    let cloned_ws = ws.clone();
    let onopen_callback = Closure::<dyn FnMut()>::new(move || {
        // console_log!("connected to server");
        cloned_ws.send_with_str("ping").unwrap();
    });

    ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
    onopen_callback.forget();
}
