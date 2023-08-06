#![allow(non_snake_case)]

use std::time::Duration;

use async_std::task;
use card_platypus::{
    actions,
    game::{
        euchre::{
            actions::{Card, EAction},
            Euchre, EuchreGameState,
        },
        GameState, Player,
    },
};
use client_server_messages::{
    ActionRequest, GameAction, GameData, GameProcessingState, NewGameRequest, NewGameResponse,
};
// import the prelude to get access to the `rsx!` macro and the `Scope` and `Element` types
use dioxus::{
    html::{button, table, tr},
    prelude::*,
};
use dioxus_router::prelude::*;

use futures_util::StreamExt;
use rand::{thread_rng, Rng};

const SERVER: &str = "api";
const PLAYER_ID_KEY: &str = "PLAYER_ID";

pub fn base_url() -> String {
    web_sys::window().unwrap().location().origin().unwrap()
}

#[derive(Routable, Clone, PartialEq)]
enum Route {
    // if the current location is "/home", render the Home component
    #[route("/")]
    NewGame {},
    // if the current location is "/blog", render the Blog component
    #[route("/game/:game_id")]
    InGame { game_id: String },

    #[route("/:..route")]
    NotFound { route: Vec<String> },
}

fn main() {
    // launch the web app
    dioxus_web::launch(App);
}

fn App(cx: Scope) -> Element {
    let local_storage = web_sys::window().unwrap().local_storage().unwrap().unwrap();

    let stored_id = local_storage.get_item(PLAYER_ID_KEY);

    if let Ok((Some(player_id))) = stored_id {
        use_shared_state_provider(cx, || PlayerId {
            id: player_id
                .parse()
                .expect("error parsing previously saved player id"),
        });
    } else {
        let player_id: usize = thread_rng().gen();
        local_storage
            .set_item(PLAYER_ID_KEY, player_id.to_string().as_str())
            .expect("error storing player id");
        use_shared_state_provider(cx, || PlayerId { id: player_id });
    }

    render! { Router::<Route> {} }
}

#[inline_props]
fn NotFound(cx: Scope, route: Vec<String>) -> Element {
    render! {
        div { format!("Error: page not found: {:?}", route) }
    }
}

#[inline_props]
fn NewGame(cx: Scope) -> Element {
    let player_id = use_shared_state::<PlayerId>(cx).unwrap().read().id;
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
            nav.push(format!("/game/{}", response.id));
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
    let game_url = format!("{}/{}/{}", base_url(), SERVER, game_id);

    let game_data = use_state(cx, || GameData::new(Euchre::new_state(), player_id));
    let _gs_polling_task = use_coroutine(cx, |_rx: UnboundedReceiver<()>| {
        let game_data = game_data.to_owned();
        async move {
            loop {
                let mut new_state = client
                    .get(game_url.clone())
                    .send()
                    .await
                    .expect("error unwraping response")
                    .json::<GameData>()
                    .await
                    .unwrap();

                // register the player if needed
                if !new_state.players.contains(&Some(player_id)) {
                    let req = ActionRequest::new(player_id, GameAction::RegisterPlayer);

                    new_state = client
                        .post(game_url.clone())
                        .json(&req)
                        .send()
                        .await
                        .expect("error registering player")
                        .json::<GameData>()
                        .await
                        .unwrap();
                }

                game_data.set(new_state);
                task::sleep(Duration::from_secs(5)).await;
            }
        }
    });

    let player_id = use_shared_state::<PlayerId>(cx).unwrap().read().id;
    let target = format!("{}/{}/{}", base_url(), SERVER, game_id);
    let _action_task = use_coroutine(cx, |mut rx: UnboundedReceiver<GameAction>| {
        let game_data = game_data.to_owned();

        async move {
            let client = reqwest::Client::new();

            while let Some(action) = rx.next().await {
                let req = ActionRequest::new(player_id, action);

                let new_state = client
                    .post(target.clone())
                    .json(&req)
                    .send()
                    .await
                    .expect("error sending action")
                    .json::<GameData>()
                    .await
                    .expect("error parsing game data");
                game_data.set(new_state);
            }
        }
    });

    let south_player = game_data
        .players
        .iter()
        .position(|x| x.is_some() && x.unwrap() == player_id)
        .unwrap();

    render!(
        div { class: "h-screen grid grid-cols-2",
            PlayArea(cx, game_data.get().clone(), south_player),
            div {
                GameData(cx, game_data.gs.clone(), south_player),
                RunningStats(cx, game_data.computer_score, game_data.human_score)
            }
        }
    )
}

fn GameData(cx: Scope<InGameProps>, gs: String, south_player: usize) -> Element {
    let gs = EuchreGameState::from(gs.as_str());
    let trump_details = gs.trump();

    let trump_string = if let Some((suit, caller)) = trump_details {
        let caller_seat = match caller {
            x if x == south_player => "South",
            x if x == (south_player + 1) % 4 => "West",
            x if x == (south_player + 2) % 4 => "North",
            x if x == (south_player + 3) % 4 => "East",
            _ => "Error finding caller seat",
        };

        format!("Trump is {}. Called by {caller_seat}", suit.icon())
    } else {
        "Trump has not been called".to_string()
    };

    let south_trick_wins = gs.trick_score()[south_player % 2];
    let east_trick_wins = gs.trick_score()[(south_player + 1) % 2];

    render!(
        div {
            div { class: "pt-8 font-bold text-xl font-large text-black", "Game information" }
            div { trump_string }
            div { class: "font-bold", "Tricks taken:" }
            div { class: "grid grid-cols-2",
                div { "North/South" }
                div { "East/West" }
                div { "{south_trick_wins}" }
                div { "{east_trick_wins}" }
            }
        }
    )
}

fn LastTrick(cx: Scope<InGameProps>, game_data: GameData, player: Player) -> Element {
    let gs = EuchreGameState::from(game_data.gs.as_str());
    if !matches!(
        game_data.display_state,
        GameProcessingState::WaitingTrickClear { ready_players: _ }
    ) {
        return render!({});
    }

    let last_trick = gs.last_trick();
    if let Some((starter, mut trick)) = last_trick {
        trick.rotate_left(4 - starter);

        render!(CardIcon(cx, trick[player]))
    } else {
        render!({})
    }
}

fn RunningStats(cx: Scope<InGameProps>, machine_score: usize, human_score: usize) -> Element {
    render!(
        div {
            div { class: "pt-8 font-bold text-xl font-large text-black", "Running stats" }
            div { class: "grid grid-cols-2",
                div { "humans" }
                div { "machines" }
                div { "{human_score}" }
                div { "{machine_score}" }
            }
        }
    )
}

fn PlayArea(cx: Scope<InGameProps>, game_data: GameData, south_player: usize) -> Element {
    let gs = EuchreGameState::from(game_data.gs.as_str());

    let west_player = (south_player + 1) % 4;
    let north_player = (south_player + 2) % 4;
    let east_player = (south_player + 3) % 4;

    let north_label = if north_player == 3 {
        "North (Dealer)"
    } else {
        "North"
    };

    let south_label = if south_player == 3 {
        "South (Dealer)"
    } else {
        "South"
    };

    let east_label = if east_player == 3 {
        "East (Dealer)"
    } else {
        "East"
    };

    let west_label = if west_player == 3 {
        "West (Dealer)"
    } else {
        "West"
    };

    cx.render(rsx! {

        div { class: "grid content-between",
            div { class: "grid",
                div { class: "justify-self-center", north_label }
                OpponentHand(cx, gs.get_hand(north_player).len(), true)
            }
            div {
                div { class: "flex justify-center",
                    PlayedCard(cx, gs.played_card(north_player)),
                    LastTrick(cx, game_data.clone(), north_player)
                }
                div { class: "flex justify-between",
                    div { class: "flex content-center",
                        div {
                            div { style: "text-align:center", west_label }
                            OpponentHand(cx, gs.get_hand(west_player).len(), false)
                        }
                        div { class: "grid content-center",
                            PlayedCard(cx, gs.played_card(west_player)),
                            LastTrick(cx, game_data.clone(), west_player)
                        }
                    }
                    div { class: "grid content-center",
                        div { style: "text-align:center",
                            FaceUpCard(cx, gs.displayed_face_up_card()),
                            ClearTrickButton(cx, game_data.clone().display_state),
                            TurnTracker(cx, gs.clone(), south_player)
                        }
                    }
                    div { class: "flex content-center",
                        div { class: "grid content-center",
                            PlayedCard(cx, gs.played_card(east_player)),
                            LastTrick(cx, game_data.clone(), east_player)
                        }
                        div { class: "grid",
                            div { class: "justify-self-center", east_label }
                            OpponentHand(cx, gs.get_hand(east_player).len(), false)
                        }
                    }
                }
                div { class: "flex justify-center",
                    PlayedCard(cx, gs.played_card(south_player)),
                    LastTrick(cx, game_data.clone(), south_player)
                }
            }
            div { class: "grid content-between",
                div { class: "justify-self-center", south_label }
                div { class: "justify-self-center", PlayerHand(cx, gs.get_hand(south_player)) }
                div { class: "justify-self-center", PlayerActions(cx, gs.clone(), south_player) }
            }
        }
    })
}

fn ClearTrickButton(cx: Scope<InGameProps>, display_state: GameProcessingState) -> Element {
    let action_task = use_coroutine_handle::<GameAction>(cx).expect("error getting action task");
    let player_id = use_shared_state::<PlayerId>(cx).unwrap().read().id;

    match display_state {
        GameProcessingState::WaitingTrickClear { ready_players } => {
            if ready_players.contains(&player_id) {
                render!( div { "waiting on other players..." } )
            } else {
                render!(
                    button {
                        class: "bg-violet-500 hover:bg-violet-600 focus:outline-none focus:ring focus:ring-violet-300 active:bg-violet-700 px-5 py-2 text-sm leading-5 rounded-full font-semibold text-white",
                        onclick: move |_| { action_task.send(GameAction::ReadyTrickClear) },
                        "Clear trick"
                    }
                )
            }
        }
        _ => render!({}),
    }
}

fn OpponentHand(cx: Scope<InGameProps>, num_cards: usize, is_north: bool) -> Element {
    if is_north {
        let mut s = String::new();
        for _ in 0..num_cards {
            s.push('🂠')
        }

        cx.render(rsx! {
            div { style: "text-align:center", font_size: "60px", s.as_str() }
        })
    } else {
        cx.render(rsx! {
            div { class: "grid gap-4",
                for _ in 0..num_cards {
                    div { class: "text-6xl", "🂠" }
                }
            }
        })
    }
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
        span { class: "text-6xl", color: color, c.icon() }
    })
}

fn PlayerActions(cx: Scope<InGameProps>, gs: EuchreGameState, south_player: usize) -> Element {
    if gs.cur_player() != south_player || gs.is_chance_node() {
        return cx.render(rsx! { div {} });
    }

    let actions: Vec<EAction> = actions!(gs).into_iter().map(EAction::from).collect();
    let action_task = use_coroutine_handle::<GameAction>(cx).expect("error getting action task");

    cx.render(rsx! {
        div { class: "",
            for a in actions.into_iter() {
                button {
                    class: "text-6xl outline hover:outline-2",
                    onclick: move |_| { action_task.send(GameAction::TakeAction(a.into())) },
                    "{a}"
                }
            }
        }
    })
}

struct PlayerId {
    pub id: usize,
}

enum PlayerLocation {
    North,
    South,
    East,
    West,
}
