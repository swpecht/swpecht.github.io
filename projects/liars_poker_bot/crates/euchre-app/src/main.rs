#![allow(non_snake_case)]

use std::fmt::format;

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
use client_server_messages::{GameData, NewGameRequest, NewGameResponse};
// import the prelude to get access to the `rsx!` macro and the `Scope` and `Element` types
use dioxus::{
    html::{table, tr},
    prelude::*,
};
use dioxus_router::prelude::*;

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
    let mut agent = PIMCTSBot::new(
        20,
        OpenHandSolver::new_euchre(),
        StdRng::from_rng(thread_rng()).unwrap(),
    );

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
            // .header("Content-Type", "application/json")
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
        None => render!( div { "Loading new game..." } ),
    }
}

#[inline_props]
fn InGame(cx: Scope, game_id: String) -> Element {
    let player_id = use_shared_state::<PlayerId>(cx).unwrap().read().id;
    let client = reqwest::Client::new();
    let target = format!("{}/{}", SERVER, game_id);
    let game_data = use_future(cx, (), |_| async move {
        client
            .get(target)
            .send()
            .await
            .expect("error unwraping response")
            .json::<GameData>()
            .await
    });

    let mut gs = Euchre::new_state();
    match game_data.value() {
        Some(Ok(response)) => {
            gs = EuchreGameState::from(response.gs.as_str());
            render!(PlayArea(cx, gs))
        }
        Some(Err(e)) => render!(
            div { format!("Error getting game data: {:?}", e) }
        ),
        None => render!( div { "Loading game data..." } ),
    }
}

fn PlayArea(cx: Scope<InGameProps>, gs: EuchreGameState) -> Element {
    let south_player = **use_state(cx, || 0);
    let west_player = (south_player + 1) % 4;
    let north_player = (south_player + 2) % 4;
    let east_player = (south_player + 3) % 4;

    cx.render(rsx! {

        h1 { "High-Five counter: {south_player}" }
        div { "west player: {west_player}" }
        div { "north player: {north_player}" }
        div { "east player: {east_player}" }
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
        button { onclick: move |_| {}, "go to next player: {south_player}" }
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

fn CardButton(cx: Scope, c: Card) -> Element {
    use card_platypus::game::euchre::actions::Suit::*;
    let color = match c.suit() {
        Clubs | Spades => "black",
        Hearts | Diamonds => "red",
    };

    cx.render(rsx! {
        button { color: color, font_size: "75px", c.icon() }
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
    if gs.cur_player() != south_player {
        return cx.render(rsx! { div {} });
    }

    let actions: Vec<EAction> = actions!(gs).into_iter().map(EAction::from).collect();

    cx.render(rsx! {
        for a in actions.iter() {
            button { font_size: "75px", "{a}" }
        }
    })
}

struct PlayerId {
    pub id: usize,
}
