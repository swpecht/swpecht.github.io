use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use async_std::stream::StreamExt;
use async_std::task;
use client_server_messages::{GameAction, GameData};
use dioxus::prelude::{
    use_coroutine, use_coroutine_handle, use_shared_state, use_shared_state_provider, use_state,
    Coroutine, Scope, UnboundedReceiver,
};
use js_sys::JsString;
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

struct WsSendMessage {
    msg: String,
}

/// Send a msg on the websocket
pub fn send_msg(cx: &Scope, msg: String) {
    let send_task = use_coroutine_handle::<WsSendMessage>(cx).expect("error getting send task");
    send_task.send(WsSendMessage { msg });
}

/// Set up a websocket connection and all call backs
pub fn set_up_ws(cx: &Scope, url: &str) {
    info!("starting web socket connection to {} ...", url);
    let ws = WebSocket::new(url).unwrap();

    let _ws_send_task = use_coroutine(cx, |mut rx: UnboundedReceiver<WsSendMessage>| {
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

    let (send, mut recv) = futures::channel::mpsc::unbounded();

    let _ws_recv_task: &Coroutine<JsString> = use_coroutine(cx, |_| async move {
        while let Some(msg) = recv.next().await {
            debug!("message received on update routine: {}", msg)
        }

        error!("error receiving message on recv routine");
    });

    let on_msg_callback = Closure::<dyn FnMut(_)>::new(move |e: MessageEvent| {
        match e.data().dyn_into::<js_sys::JsString>() {
            Ok(msg) => {
                debug!("message received by websocket: {}", msg);
                match send.unbounded_send(msg) {
                    Ok(_) => {}
                    Err(e) => error!("error sending message to update routine: {:?}", e),
                };
            }
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
