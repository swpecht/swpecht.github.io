#![allow(non_snake_case)]

use std::{fmt::format, time::Duration};

use async_std::task;
use card_platypus::{
    actions,
    algorithms::{open_hand_solver::OpenHandSolver, pimcts::PIMCTSBot},
    game::{
        euchre::{
            actions::{Card, EAction},
            Euchre, EuchreGameState,
        },
        Action, GameState,
    },
};
use client_server_messages::{ActionRequest, GameData, NewGameRequest, NewGameResponse};
// import the prelude to get access to the `rsx!` macro and the `Scope` and `Element` types
use dioxus::{
    html::{table, tr},
    prelude::*,
};
use dioxus_router::prelude::*;

use futures_util::StreamExt;
use rand::{rngs::StdRng, thread_rng, Rng, SeedableRng};

const SERVER: &str = "http://127.0.0.1:4000";

#[derive(Routable, Clone, PartialEq)]
enum Route {
    // if the current location is "/home", render the Home component
    #[route("/")]
    NewGame {},
    // if the current location is "/blog", render the Blog component
    #[route("/:game_id")]
    InGame { game_id: String },
}

fn main() {
    // launch the web app
    dioxus_web::launch(App);
}

fn App(cx: Scope) -> Element {
    // set the random player id
    use_shared_state_provider(cx, || PlayerId {
        id: thread_rng().gen(),
    });

    render! { Router::<Route> {} }
}

#[inline_props]
fn NewGame(cx: Scope) -> Element {
    let player_id = use_shared_state::<PlayerId>(cx).unwrap().read().id;
    let new_game_req = NewGameRequest::new(player_id);

    let client = reqwest::Client::new();

    let new_game_response = use_future(cx, (), |_| async move {
        client
            .post(SERVER)
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
            nav.push(format!("/{}", response.id));
            render!({})
        }
        Some(Err(e)) => render!(
            div { format!("Error getting new game: {:?}", e) }
        ),
        None => render!( div { class: "text-xl", "Loading new game..." } ),
    }
}

#[inline_props]
fn InGame(cx: Scope, game_id: String) -> Element {
    let player_id = use_shared_state::<PlayerId>(cx).unwrap().read().id;
    let client = reqwest::Client::new();
    let target = format!("{}/{}", SERVER, game_id);

    let game_data = use_state(cx, || GameData::new(Euchre::new_state(), player_id));
    let _gs_polling_task = use_coroutine(cx, |_rx: UnboundedReceiver<()>| {
        let game_data = game_data.to_owned();
        async move {
            loop {
                let new_state = client
                    .get(target.clone())
                    .send()
                    .await
                    .expect("error unwraping response")
                    .json::<GameData>()
                    .await
                    .unwrap();
                game_data.set(new_state);
                task::sleep(Duration::from_secs(5)).await;
            }
        }
    });

    let target = format!("{}/{}", SERVER, game_id);
    let _action_task = use_coroutine(cx, |mut rx: UnboundedReceiver<EAction>| {
        let game_data = game_data.to_owned();
        let south_player = game_data
            .players
            .iter()
            .position(|x| x.is_some() && x.unwrap() == player_id)
            .unwrap();

        async move {
            let client = reqwest::Client::new();

            while let Some(a) = rx.next().await {
                let action_req = ActionRequest::new(south_player, a.into());
                let new_state = client
                    .post(target.clone())
                    .json(&action_req)
                    .send()
                    .await
                    .expect("error sending action")
                    .json::<GameData>()
                    .await
                    .unwrap();
                game_data.set(new_state);
            }

            task::sleep(Duration::from_secs(1)).await;
        }
    });

    render!(PlayArea(cx, game_data.get().clone()))
}

fn PlayArea(cx: Scope<InGameProps>, game_data: GameData) -> Element {
    let gs = EuchreGameState::from(game_data.gs.as_str());

    let player_id = use_shared_state::<PlayerId>(cx).unwrap().read().id;
    let south_player = game_data
        .players
        .iter()
        .position(|x| x.is_some() && x.unwrap() == player_id)
        .unwrap();

    let west_player = (south_player + 1) % 4;
    let north_player = (south_player + 2) % 4;
    let east_player = (south_player + 3) % 4;

    cx.render(rsx! {

        h1 { "High-Five counter: {south_player}" }
        div { "west player: {west_player}" }
        div { "north player: {north_player}" }
        div { "east player: {east_player}" }
        div { format!("humans: {}", game_data.human_score) }
        div { format!("machines: {}", game_data.computer_score) }
        table {
            tr {
                td {}
                td {}
                td { OpponentHand(cx, gs.get_hand(north_player).len()) }
            }
            tr {
                td {}
                td {}
                td { style: "text-align:center", PlayedCard(cx, gs.played_card(north_player)) }
            }
            tr {
                td { OpponentHand(cx, gs.get_hand(west_player).len()) }
                td { style: "text-align:center", PlayedCard(cx, gs.played_card(west_player)) }
                td { style: "text-align:center",
                    FaceUpCard(cx, gs.displayed_face_up_card()),
                    TurnTracker(cx, gs.clone(), south_player)
                }
                td { style: "text-align:center", PlayedCard(cx, gs.played_card(east_player)) }
                td { OpponentHand(cx, gs.get_hand(east_player).len()) }
            }
            tr {
                td {}
                td {}
                td { style: "text-align:center", PlayedCard(cx, gs.played_card(south_player)) }
            }
            tr {
                td {}
                td {}
                td { style: "text-align:center",
                    div { PlayerHand(cx, gs.get_hand(south_player)) }
                    div { PlayerActions(cx, gs.clone(), south_player) }
                }
            }
        }
        GameLog(cx, gs)
    })
}

fn OpponentHand(cx: Scope<InGameProps>, num_cards: usize) -> Element {
    let mut s = String::new();
    for _ in 0..num_cards {
        s.push('🂠')
    }

    cx.render(rsx! {
        div { font_size: "75px", s.as_str() }
    })
}

fn PlayerHand(cx: Scope<InGameProps>, hand: Vec<Card>) -> Element {
    cx.render(rsx! {
        for c in hand.iter() {
            CardIcon(cx, *c)
        }
    })
}

fn PlayedCard(cx: Scope<InGameProps>, c: Option<Card>) -> Element {
    if let Some(c) = c {
        cx.render(rsx! {CardIcon(cx, c)})
    } else {
        cx.render(rsx! { div { font_size: "60px" } })
    }
}

fn TurnTracker(cx: Scope<InGameProps>, gs: EuchreGameState, south_player: usize) -> Element {
    let arrow = match gs.cur_player() {
        x if x == (south_player + 1) % 4 => "←",
        x if x == (south_player + 2) % 4 => "↑",
        x if x == (south_player + 3) % 4 => "→",
        _ => "↓",
    };
    cx.render(rsx! { div { font_size: "60px", "{arrow}" } })
}

fn FaceUpCard(cx: Scope<InGameProps>, c: Option<Card>) -> Element {
    if let Some(c) = c {
        cx.render(rsx! {CardIcon(cx, c)})
    } else {
        cx.render(rsx! { div {} })
    }
}

fn CardIcon(cx: Scope<InGameProps>, c: Card) -> Element {
    use card_platypus::game::euchre::actions::Suit::*;
    let color = match c.suit() {
        Clubs | Spades => "black",
        Hearts | Diamonds => "red",
    };

    cx.render(rsx! {
        span { color: color, font_size: "75px", c.icon() }
    })
}

fn GameLog(cx: Scope<InGameProps>, gs: EuchreGameState) -> Element {
    let mut log = Vec::new();

    for (p, a) in gs.history().into_iter() {
        use EAction::*;
        let description = match a {
            DealFaceUp { c } => format!("{} is the faceup card", c.icon()),
            Pickup => format!("{p} told the dealer to pickup"),
            Pass => format!("{p} passed\n"),
            Clubs | Spades | Hearts | Diamonds => format!("{p} called {a} as trump"),
            DealPlayer { c } | Discard { c } => "".to_string(), // nothing reported, hidden action
            Play { c } => format!("{p} played {}\n", c.icon()),
            DiscardMarker => panic!("should not encounter a discard marker in gamestate"),
        };

        log.push(description);
    }

    cx.render(rsx! {
        div { font_size: "30px", "Log:" }
        for item in log.iter() {
            div { font_size: "30px", "{item}" }
        }
    })
}

fn PlayerActions(cx: Scope<InGameProps>, gs: EuchreGameState, south_player: usize) -> Element {
    if gs.cur_player() != south_player || gs.is_chance_node() {
        return cx.render(rsx! { div {} });
    }

    let actions: Vec<EAction> = actions!(gs).into_iter().map(EAction::from).collect();
    let action_task = use_coroutine_handle::<EAction>(cx).expect("error getting action task");

    cx.render(rsx! {
        for a in actions.into_iter() {
            button { onclick: move |_| { action_task.send(a) }, font_size: "75px", "{a}" }
        }
    })
}

struct PlayerId {
    pub id: usize,
}
