#![allow(non_snake_case)]

use std::time::Duration;

use async_std::task;
use card_platypus::{
    actions,
    game::{
        euchre::{
            actions::{Card, EAction, Suit},
            EuchreGameState,
        },
        GameState, Player,
    },
};
use client_server_messages::{ActionRequest, GameAction, GameData, GameProcessingState};
use dioxus::prelude::*;
use futures_util::StreamExt;

use crate::{base_url, requests::make_game_request, PlayerId, ACTION_BUTTON_CLASS, SERVER};

#[derive(Debug, Clone)]
pub enum InGameState {
    Loading,
    NotFound,
    GameFull,
    UnknownError(String),
    Ok(GameData),
}

#[inline_props]
pub fn InGame(cx: Scope, game_id: String) -> Element {
    let player_id = use_shared_state::<PlayerId>(cx).unwrap().read().id;
    let client = reqwest::Client::new();
    let game_url = format!("{}/{}/{}", base_url(), SERVER, game_id);

    let state = use_state(cx, || InGameState::Loading);
    let _gs_polling_task = use_coroutine(cx, |_rx: UnboundedReceiver<()>| {
        let game_data = state.to_owned();
        async move {
            loop {
                // get the latest state
                let mut new_state = make_game_request(client.get(game_url.clone())).await;

                // make sure we're an active player, and try to register as one if we can
                new_state = match new_state {
                    InGameState::Ok(gd) if gd.players.contains(&Some(player_id)) => {
                        InGameState::Ok(gd)
                    }
                    InGameState::Ok(gd) if gd.players.len() < 2 => InGameState::Ok(gd),
                    InGameState::Ok(_) => {
                        make_game_request(
                            client
                                .post(game_url.clone())
                                .json(&ActionRequest::new(player_id, GameAction::RegisterPlayer)),
                        )
                        .await
                    }

                    _ => new_state,
                };

                game_data.set(new_state);
                task::sleep(Duration::from_secs(5)).await;
            }
        }
    });

    let player_id = use_shared_state::<PlayerId>(cx).unwrap().read().id;
    let target = format!("{}/{}/{}", base_url(), SERVER, game_id);
    let _action_task = use_coroutine(cx, |mut rx: UnboundedReceiver<GameAction>| {
        let game_data = state.to_owned();

        async move {
            let client = reqwest::Client::new();

            while let Some(action) = rx.next().await {
                let req = ActionRequest::new(player_id, action);

                let new_state = make_game_request(client.post(target.clone()).json(&req)).await;

                // only set the state to this if it's a valid response. We could get 400 errors
                // for trying to play a move multiple times
                if let InGameState::Ok(_) = new_state {
                    game_data.set(new_state);
                }
            }
        }
    });

    match state.get() {
        InGameState::Ok(gd) => {
            let south_player = gd
                .players
                .iter()
                .position(|x| x.is_some() && x.unwrap() == player_id)
                .unwrap();

            render!(
                div { class: "h-screen grid sm:flex sm:flex-row m-1",
                    div { class: "sm:basis-3/4", PlayArea(cx, gd.clone(), south_player) }
                    div { class: "sm:basis-1/4",
                        GameData(cx, gd.gs.clone(), south_player),
                        RunningStats(cx, gd.computer_score, gd.human_score)
                    }
                }
            )
        }
        InGameState::NotFound => GameNotFound(cx),
        InGameState::Loading => Loading(cx),
        InGameState::UnknownError(msg) => UnknownError(cx, msg),
        InGameState::GameFull => GameFull(cx),
    }
}

fn Loading<T>(cx: Scope<T>) -> Element {
    render!("loading...")
}

fn GameNotFound<T>(cx: Scope<T>) -> Element {
    render!("error, the request game wasn't found. Try going back and starting a new one...")
}

fn GameFull<T>(cx: Scope<T>) -> Element {
    render!("game is full. Try creating a new game instead")
}

fn UnknownError<'a, T>(cx: Scope<'a, T>, msg: &'a String) -> Element<'a> {
    render!("Encountered an unexpected error: {msg}")
}

fn GameData<T>(cx: Scope<T>, gs: String, south_player: usize) -> Element {
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

fn LastTrick<T>(cx: Scope<T>, game_data: GameData, player: Player) -> Element {
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

fn RunningStats<T>(cx: Scope<T>, machine_score: usize, human_score: usize) -> Element {
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

fn PlayArea<T>(cx: Scope<T>, game_data: GameData, south_player: usize) -> Element {
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

        div { class: "grid grid-cols-5 content-between",
            // North area
            div { class: "col-start-2 col-span-3 grid",
                div { class: "justify-self-center", north_label }
                OpponentHand(cx, gs.get_hand(north_player).len(), true)
            }

            // Middle area
            div { class: "row-start-2 grid justify-items-center",
                div { class: "pb-4", west_label }
                OpponentHand(cx, gs.get_hand(west_player).len(), false)
            }

            div { class: "col-span-3 grid grid-cols-3 items-center justify-items-center space-y-4",
                div { class: "col-start-2",
                    PlayedCard(cx, gs.played_card(north_player)),
                    LastTrick(cx, game_data.clone(), north_player)
                }
                div { class: "row-start-2",
                    PlayedCard(cx, gs.played_card(west_player)),
                    LastTrick(cx, game_data.clone(), west_player)
                }
                div { class: "row-start-2 col-start-2 grid justify-items-center",
                    FaceUpCard(cx, gs.displayed_face_up_card()),
                    TurnTracker(cx, gs.clone(), south_player),
                    ClearTrickButton(cx, game_data.clone().display_state)
                }

                div { class: "row-start-2 col-start-3",
                    PlayedCard(cx, gs.played_card(east_player)),
                    LastTrick(cx, game_data.clone(), east_player)
                }

                div { class: "row-start-3 col-start-2",
                    PlayedCard(cx, gs.played_card(south_player)),
                    LastTrick(cx, game_data.clone(), south_player)
                }
            }
            div { class: "grid justify-items-center",
                div { class: "pb-4", east_label }
                OpponentHand(cx, gs.get_hand(east_player).len(), false)
            }

            // bottom area
            div { class: "row-start-3 col-span-5 grid justify-items-center",
                div { class: "self-end", south_label }
                PlayerActions(cx, gs.clone(), south_player)
            }
        }
    })
}

fn ClearTrickButton<T>(cx: Scope<T>, display_state: GameProcessingState) -> Element {
    let action_task = use_coroutine_handle::<GameAction>(cx).expect("error getting action task");
    let player_id = use_shared_state::<PlayerId>(cx).unwrap().read().id;

    match display_state {
        GameProcessingState::WaitingTrickClear { ready_players } => {
            if ready_players.contains(&player_id) {
                render!( div { "waiting on other players..." } )
            } else {
                render!(
                    button {
                        class: "bg-white outline outline-black hover:bg-slate-100 focus:outline-none focus:ring focus:bg-slate-100 active:bg-slate-200 px-5 py-2 text-sm leading-5 rounded-full font-semibold text-black",
                        onclick: move |_| { action_task.send(GameAction::ReadyTrickClear) },
                        "Clear trick"
                    }
                )
            }
        }
        _ => render!({}),
    }
}

fn OpponentHand<T>(cx: Scope<T>, num_cards: usize, is_north: bool) -> Element {
    if is_north {
        let mut s = String::new();
        for _ in 0..num_cards {
            s.push('🂠')
        }

        cx.render(rsx! {
            div { class: "text-2xl lg:text-6xl", style: "text-align:center", s.as_str() }
        })
    } else {
        cx.render(rsx! {
            div { class: "grid grid-cols-2 gap-1 lg:gap-4",
                for _ in 0..num_cards {
                    div { class: "text-2xl lg:text-6xl", "🂠" }
                }
            }
        })
    }
}

fn PlayedCard<T>(cx: Scope<T>, c: Option<Card>) -> Element {
    if let Some(c) = c {
        cx.render(rsx! {CardIcon(cx, c)})
    } else {
        cx.render(rsx! { div { font_size: "60px" } })
    }
}

fn TurnTracker<T>(cx: Scope<T>, gs: EuchreGameState, south_player: usize) -> Element {
    let arrow = match gs.cur_player() {
        x if x == (south_player + 1) % 4 => "←",
        x if x == (south_player + 2) % 4 => "↑",
        x if x == (south_player + 3) % 4 => "→",
        _ => "↓",
    };
    cx.render(rsx! { div { class: "text-2xl lg:text-6xl", "{arrow}" } })
}

fn FaceUpCard<T>(cx: Scope<T>, c: Option<Card>) -> Element {
    if let Some(c) = c {
        cx.render(rsx! {CardIcon(cx, c)})
    } else {
        render!({})
    }
}

fn CardIcon<T>(cx: Scope<T>, c: Card) -> Element {
    use card_platypus::game::euchre::actions::Suit::*;
    let color = match c.suit() {
        Clubs | Spades => "black",
        Hearts | Diamonds => "red",
    };

    cx.render(rsx! {
        span { class: "text-2xl lg:text-6xl", color: color, c.icon() }
    })
}

fn PlayerActions<T>(cx: Scope<T>, gs: EuchreGameState, south_player: usize) -> Element {
    if gs.cur_player() != south_player || gs.is_chance_node() {
        return render!({});
    }

    let actions: Vec<EAction> = actions!(gs).into_iter().map(EAction::from).collect();
    let action_task = use_coroutine_handle::<GameAction>(cx).expect("error getting action task");

    if actions.contains(&EAction::Pickup) {
        // special case for play pickup and pass
        let hand = gs.get_hand(south_player);
        render!(
            div { class: "grid gap-y-4 justify-items-center",
                div { class: "flex gap-x-4",
                    for c in hand.into_iter() {
                        CardIcon(cx, c)
                    }
                }
                div { class: "flex gap-x-4",
                    button {
                        class: "basis-1/2 text-xl {ACTION_BUTTON_CLASS}",
                        onclick: move |_| { action_task.send(GameAction::TakeAction(EAction::Pickup.into())) },
                        "Tell dealer to take card"
                    }

                    button {
                        class: "basis-1/2 text-xl {ACTION_BUTTON_CLASS}",
                        onclick: move |_| { action_task.send(GameAction::TakeAction(EAction::Pass.into())) },
                        "Pass"
                    }
                }
            }
        )
    } else if actions.contains(&EAction::Clubs) {
        // special case for choosing suit
        let hand = gs.get_hand(south_player);
        render!(
            div { class: "grid gap-y-4",
                div { class: "flex gap-x-4",
                    for c in hand.into_iter() {
                        CardIcon(cx, c)
                    }
                }
                div { class: "flex gap-x-4",
                    button {
                        class: "text-xl text-black {ACTION_BUTTON_CLASS}",
                        onclick: move |_| { action_task.send(GameAction::TakeAction(EAction::Spades.into())) },
                        Suit::Spades.icon()
                    }

                    button {
                        class: "text-xl text-black {ACTION_BUTTON_CLASS}",
                        onclick: move |_| { action_task.send(GameAction::TakeAction(EAction::Clubs.into())) },
                        Suit::Clubs.icon()
                    }

                    button {
                        class: "text-xl text-red-500 {ACTION_BUTTON_CLASS}",
                        onclick: move |_| { action_task.send(GameAction::TakeAction(EAction::Hearts.into())) },
                        Suit::Hearts.icon()
                    }

                    button {
                        class: "text-xl text-red-500 {ACTION_BUTTON_CLASS}",
                        onclick: move |_| { action_task.send(GameAction::TakeAction(EAction::Diamonds.into())) },
                        Suit::Diamonds.icon()
                    }

                    button {
                        class: "text-xl {ACTION_BUTTON_CLASS}",
                        onclick: move |_| { action_task.send(GameAction::TakeAction(EAction::Pass.into())) },
                        "Pass"
                    }
                }
            }
        )
    } else {
        let hand: Vec<(Card, Option<EAction>)> = gs
            .get_hand(south_player)
            .into_iter()
            .map(|c| (c, actions.iter().find(|a| a.card() == c).cloned()))
            .collect();

        render!(
            div { class: "flex space-x-4",
                for (c , a) in hand.into_iter() {
                    ActionButton(cx, c, a)
                }
            }
        )
    }
}

fn ActionButton<T>(cx: Scope<T>, card: Card, action: Option<EAction>) -> Element {
    use card_platypus::game::euchre::actions::Suit::*;
    let color = match card.suit() {
        Clubs | Spades => "text-black",
        Hearts | Diamonds => "text-red-500",
    };
    let action_task = use_coroutine_handle::<GameAction>(cx).expect("error getting action task");

    if let Some(a) = action {
        render!(
            button {
                class: "text-2xl {ACTION_BUTTON_CLASS} {color}",
                onclick: move |_| { action_task.send(GameAction::TakeAction(a.into())) },
                card.icon()
            }
        )
    } else {
        render!(
            button { disabled: "true", class: "text-2xl {ACTION_BUTTON_CLASS} {color}", card.icon() }
        )
    }
}
