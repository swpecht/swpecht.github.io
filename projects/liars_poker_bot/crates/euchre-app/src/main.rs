#![allow(non_snake_case)]

use std::fmt::Display;

use client_server_messages::{GameData, NewGameRequest, NewGameResponse};
// import the prelude to get access to the `rsx!` macro and the `Scope` and `Element` types
use dioxus::prelude::*;
use dioxus_router::prelude::*;

use euchre_app::{
    base_url, hide_element,
    in_game::InGame,
    settings::{get_player_id, min_players, register_settings, set_event_id, set_min_players},
    show_element, ACTION_BUTTON_CLASS, SERVER,
};
use log::{debug, error, info};
use rand::{thread_rng, Rng};

#[derive(Routable, Clone, PartialEq)]
pub enum Route {
    // if the current location is "/home", render the Home component
    #[route("/")]
    Index {},

    #[route("/game/:game_id")]
    InGame { game_id: String },

    #[route("/game")]
    NewGame,

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

    register_settings(cx);

    // set_up_ws(&cx);
    // let send_task = use_coroutine_handle::<WsSendMessage>(cx).expect("error getting ws task");
    // send_msg(send_task, "test message".to_string());

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
fn Index(cx: Scope) -> Element {
    show_element("intro");

    render!(
        div { class: "max-w-xlg grid space-y-4 mx-4 my-4",

            div { class: "grid justify-items-center",
                div {
                    button {
                        class: "{ACTION_BUTTON_CLASS} font-medium px-2 mx-2",
                        onclick: move |_| {
                            set_min_players(cx, 1);
                            let nav = use_navigator(cx);
                            nav.push("/game");
                        },
                        "One Player"
                    }

                    button {
                        class: "{ACTION_BUTTON_CLASS} font-medium px-2 mx-2",
                        onclick: move |_| {
                            set_min_players(cx, 2);
                            let nav = use_navigator(cx);
                            nav.push("/game");
                        },
                        "Two player"
                    }
                }
            }
        }
    )
}

#[inline_props]
fn NewGame(cx: Scope) -> Element {
    hide_element("intro");

    let player_id = get_player_id(cx).unwrap();
    let min_players = min_players(cx);
    let new_game_req = NewGameRequest::new(player_id, min_players);
    info!("requesting a new game: {:?}", new_game_req);

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
            info!("new game created. id: {}", response.id);
            nav.replace(format!("/game/{}", response.id));
            render!({})
        }
        Some(Err(e)) => render!(
            div { format!("Error getting new game: {:?}", e) }
        ),
        None => render!( div { class: "text-xl", "Loading new game..." } ),
    }
}
