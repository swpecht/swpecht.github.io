#![allow(non_snake_case)]

use client_server_messages::{GameData, NewGameRequest, NewGameResponse};
// import the prelude to get access to the `rsx!` macro and the `Scope` and `Element` types
use dioxus::prelude::*;
use dioxus_router::prelude::*;

use euchre_app::{
    base_url, hide_element,
    in_game::{InGame, InGameState},
    player_id,
    requests::{send_msg, set_up_ws, WsRecvChannel, WsSendMessage},
    set_player_id, show_element, ACTION_BUTTON_CLASS, SERVER,
};
use futures_util::StreamExt;
use log::{debug, error};
use rand::{thread_rng, Rng};

const PLAYER_ID_KEY: &str = "PLAYER_ID";

#[derive(Routable, Clone, PartialEq)]
enum Route {
    // if the current location is "/home", render the Home component
    #[route("/")]
    Index {},

    #[route("/event")]
    Event {},

    #[route("/game")]
    NewGame {},
    // if the current location is "/blog", render the Blog component
    #[route("/game/:game_id")]
    InGame { game_id: String },

    #[route("/:..route")]
    NotFound { route: Vec<String> },
}

fn main() {
    // launch the web app
    wasm_logger::init(wasm_logger::Config::default());
    dioxus_web::launch(App);
}

fn App(cx: Scope) -> Element {
    hide_element("loading");

    // set_up_ws(&cx);
    // let send_task = use_coroutine_handle::<WsSendMessage>(cx).expect("error getting ws task");
    // send_msg(send_task, "test message".to_string());

    let local_storage = web_sys::window().unwrap().local_storage().unwrap().unwrap();
    let stored_id = local_storage.get_item(PLAYER_ID_KEY);
    if let Ok(Some(player_id)) = stored_id {
        set_player_id(
            cx,
            player_id
                .parse()
                .expect("error parsing previously saved player id"),
        );
    } else {
        let player_id: usize = thread_rng().gen();
        local_storage
            .set_item(PLAYER_ID_KEY, player_id.to_string().as_str())
            .expect("error storing player id");
        set_player_id(cx, player_id);
    }

    render! { Router::<Route> {} }
}

#[inline_props]
fn NotFound(cx: Scope, route: Vec<String>) -> Element {
    hide_element("intro");
    render! {
        div { format!("Error: page not found: {:?}", route) }
    }
}

#[inline_props]
fn Event(cx: Scope) -> Element {
    hide_element("intro");
    render!(

        div { class: "mt-10 sm:mx-auto sm:w-full sm:max-w-sm",
            form { class: "space-y-6", action: "#", method: "POST",
                label { "Event ID" }
                input {
                    class: "block w-full rounded-md border-0 py-1.5 text-gray-900 shadow-sm ring-1 ring-inset ring-gray-300 placeholder:text-gray-400 focus:ring-2 focus:ring-inset focus:ring-indigo-600 sm:text-sm sm:leading-6",
                    id: "event"
                }
                button { class: "{ACTION_BUTTON_CLASS} font-medium px-2 mx-2", "Submit" }
            }
        }
    )
}

#[inline_props]
fn Index(cx: Scope) -> Element {
    show_element("intro");
    render!(
        div { class: "max-w-xlg grid space-y-4 mx-4 my-4",

            div { class: "grid justify-items-center",
                div {
                    button {
                        class: "{ACTION_BUTTON_CLASS} font-medium px-2 mx-2",
                        onclick: move |_| {
                            let nav = use_navigator(cx);
                            nav.push("/game");
                        },
                        "New game"
                    }

                    button {
                        class: "{ACTION_BUTTON_CLASS} font-medium px-2 mx-2",
                        onclick: move |_| {
                            let nav = use_navigator(cx);
                            nav.push("/event");
                        },
                        "Event"
                    }
                }
            }
        }
    )
}

#[inline_props]
fn NewGame(cx: Scope) -> Element {
    hide_element("intro");

    let player_id = player_id(cx).unwrap();
    let new_game_req = NewGameRequest::new(player_id);

    let client = reqwest::Client::new();
    let new_game_response = use_future(cx, (), |_| async move {
        client
            .post(base_url() + "/" + SERVER)
            .json(&new_game_req)
            .send()
            .await
            .expect("error unwraping response")
            .json::<NewGameResponse>()
            .await
    });

    let nav = use_navigator(cx);
    match new_game_response.value() {
        Some(Ok(response)) => {
            // use replace here since we want to return to the index page
            // not the game page on back
            nav.replace(format!("/game/{}", response.id));
            render!({})
        }
        Some(Err(e)) => render!(
            div { format!("Error getting new game: {:?}", e) }
        ),
        None => render!( div { class: "text-xl", "Loading new game..." } ),
    }
}
