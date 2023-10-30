use std::sync::mpsc;
use std::time::Duration;

use async_std::stream::StreamExt;
use async_std::task;
use client_server_messages::{GameAction, GameData};
use dioxus::prelude::{
    use_coroutine, use_coroutine_handle, use_shared_state, use_shared_state_provider, use_state,
    Scope, UnboundedReceiver,
};
use log::{debug, error, info};
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

/// Holds the channel to aynchronously send messages on the websocket
struct WsMessage {
    msg: String,
}

pub fn send_msg(cx: &Scope, msg: String) {
    let send_task = use_coroutine_handle::<WsMessage>(cx).expect("error getting send task");
    send_task.send(WsMessage { msg });
}

pub fn set_up_ws(cx: &Scope) {
    let url = "ws://localhost:4000/ws/";
    info!("starting web socket connection to {} ...", url);
    let ws = WebSocket::new(url).unwrap();

    let _ws_send_task = use_coroutine(cx, |mut rx: UnboundedReceiver<WsMessage>| {
        let ws = ws.clone();

        async move {
            while let Some(msg) = rx.next().await {
                let mut wait_time = 1;
                loop {
                    match ws.ready_state() {
                        0 => {
                            info!("websocket still connecting. trying again in {}s", wait_time);
                            task::sleep(Duration::from_secs(wait_time)).await;
                            wait_time *= 2;
                        }
                        1 => {
                            break;
                        }
                        _ => {
                            error!("unexpected websocket state");
                            panic!("unexpected websocket state");
                        }
                    }
                }

                match ws.send_with_str(&msg.msg) {
                    Ok(_) => debug!("sent message on websocket: {}", msg.msg),
                    Err(e) => error!("error sending message on web socket: {:?}", e),
                };
            }
        }
    });

    // let state = use_state(cx, || InGameState::Loading);
    // let player_id = use_shared_state::<PlayerId>(cx).unwrap().read().id;

    // let (snd_to_ws, rcv_to_ws) = mpsc::channel();
    // let (snd_from_ws, rcv_from_ws) = mpsc::channel();

    // let action_task = use_coroutine_handle::<GameAction>(cx).expect("error getting action task");
    let on_msg_callback = Closure::<dyn FnMut(_)>::new(move |e: MessageEvent| {
        match e.data().dyn_into::<js_sys::JsString>() {
            Ok(msg) => debug!("message received by websocket: {}", msg),
            Err(e) => error!("error turning websocket msg to string: {:?}", e),
        };
    });
    ws.set_onmessage(Some(on_msg_callback.as_ref().unchecked_ref()));
    on_msg_callback.forget();

    let on_error_callback = Closure::<dyn FnMut(_)>::new(|e: ErrorEvent| {});
    ws.set_onerror(Some(on_error_callback.as_ref().unchecked_ref()));
    on_error_callback.forget();

    let onopen_callback =
        Closure::<dyn FnMut()>::new(move || info!("websocket connected to server"));

    ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
    onopen_callback.forget();

    send_msg(cx, "test message".to_string());
}

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
