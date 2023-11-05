use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use async_std::stream::StreamExt;
use async_std::task;
use client_server_messages::{ActionRequest, GameAction, GameData};
use dioxus::prelude::*;

use log::{debug, error, info};
use reqwest::{RequestBuilder, StatusCode};
use wasm_bindgen::prelude::*;

use web_sys::{ErrorEvent, MessageEvent, WebSocket};

use crate::base_url;
use crate::in_game::InGameState;

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

pub struct WsRecvChannel {
    pub recv: UnboundedReceiver<String>,
}

pub struct WsSendMessage {
    msg: String,
}

pub struct WsResponseMessage {
    msg: String,
}

pub fn send_action(
    ws_task: &Coroutine<WsSendMessage>,
    action_task: &Coroutine<GameAction>,
    action: GameAction,
) {
    info!("sending action: {:?}", action);

    match serde_json::to_string(&action) {
        Ok(msg) => send_msg(ws_task, msg),
        Err(e) => error!(
            "failed to serialize action request. error: {:?} action request: {:?}",
            e, action
        ),
    };

    // Still do the old method for now
    action_task.send(action);
}

/// Send a msg on the websocket
pub fn send_msg(send_task: &Coroutine<WsSendMessage>, msg: String) {
    send_task.send(WsSendMessage { msg });
}

/// Set up a websocket connection
///
/// Messages can be sent on the websocket using the `send_msg` functions or by using
/// the co-routine of `WsSendMessage`
///
/// Responses are saved to the shared state `WsResponseMsg`
pub fn set_up_ws<T>(cx: &Scope<T>) {
    let base_url = base_url();
    let url = format!("ws://{}/ws/", &base_url["https://".len() - 1..]);
    info!("starting web socket connection to {} ...", url);

    use_shared_state_provider(cx, || WsResponseMessage {
        msg: "".to_string(),
    });

    let ws = WebSocket::new(url.as_str()).unwrap();

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

    let on_msg_callback = Closure::<dyn FnMut(_)>::new(move |e: MessageEvent| {
        match e.data().dyn_into::<js_sys::JsString>() {
            Ok(msg) => {
                debug!("message received by websocket: {}", msg);
                match send.unbounded_send(
                    msg.as_string()
                        .expect("error converting JsString to String"),
                ) {
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

    let response_state = use_shared_state::<WsResponseMessage>(cx).unwrap();

    let _ws_recv_task: &Coroutine<String> = use_coroutine(cx, |_| {
        debug!("started ws receive routine");
        let response_state = response_state.to_owned();
        async move {
            while let Some(msg) = recv.next().await {
                debug!("response_state updated: {}", msg);
                response_state.write_silent().msg = msg;
            }

            error!("error receiving message on recv routine");
        }
    });
}
