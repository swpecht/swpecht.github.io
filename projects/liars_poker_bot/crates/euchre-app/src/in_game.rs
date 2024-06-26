#![allow(non_snake_case)]

use std::{fmt::Display, time::Duration};

use async_std::task;
use client_server_messages::{ActionRequest, GameAction, GameData, GameProcessingState};
use dioxus::prelude::*;
use dioxus_router::prelude::*;
use futures_util::StreamExt;
use games::{
    actions,
    gamestates::euchre::{
        actions::{Card, EAction, Suit},
        EPhase, EuchreGameState,
    },
    GameState, Player,
};
use log::info;

use crate::{
    app::Route, base_url, hide_element, requests::make_game_request, settings::get_player_id,
    ACTION_BUTTON_CLASS, SERVER,
};

#[derive(Debug, Clone)]
pub enum TableLocation {
    North,
    South,
    East,
    West,
}

impl TableLocation {
    pub fn to_location(player_id: usize, gd: &GameData, player: Player) -> TableLocation {
        let south_player = TableLocation::south_player(player_id, gd);

        use TableLocation::*;
        match player {
            x if x == south_player => South,
            x if x == (south_player + 1) % 4 => West,
            x if x == (south_player + 2) % 4 => North,
            _ => East,
        }
    }

    pub fn south_player(player_id: usize, gd: &GameData) -> Player {
        gd.players
            .iter()
            .position(|x| x.is_some() && x.unwrap() == player_id)
            .unwrap()
    }
}

impl Display for TableLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TableLocation::North => f.write_str("North"),
            TableLocation::South => f.write_str("South"),
            TableLocation::East => f.write_str("East"),
            TableLocation::West => f.write_str("West"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum InGameState {
    Loading,
    NotFound,
    GameFull,
    UnknownError(String),
    Ok(GameData),
}

#[component]
pub fn InGame(cx: Scope, game_id: String) -> Element {
    hide_element("intro");

    let player_id = get_player_id(cx).unwrap();
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

    let player_id = get_player_id(cx).unwrap();
    let target = format!("{}/{}/{}", base_url(), SERVER, game_id);
    let _action_task = use_coroutine(cx, |mut rx: UnboundedReceiver<GameAction>| {
        let game_data = state.to_owned();

        async move {
            let client = reqwest::Client::new();

            while let Some(action) = rx.next().await {
                info!("sending actiond: {:?}", action);
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
        InGameState::Ok(gd)
            if matches!(
                gd.display_state,
                GameProcessingState::WaitingPlayerJoin { min_players: _ }
            ) =>
        {
            WaitingForOtherPlayers(cx)
        }

        InGameState::Ok(gd) if matches!(gd.display_state, GameProcessingState::GameOver) => {
            GameOver(cx, game_id.clone())
        }
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
                        RunningStats(cx, gd.computer_score, gd.human_score),
                        PlayerStats(cx, gd.players.clone())
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

fn GameOver<T>(cx: Scope<T>, game_id: String) -> Element {
    render!(
        div { class: "px-8 pt-8",
            div { class: "font-bold text-xl font-large text-black", "Thanks for playing!" }
            div {
                "If you completed this game as part of an event, please register your game to be eligible for any applicable prizes: "
                a {
                    class: "text-blue-600 visited:text-purple-600",
                    target: "_blank",
                    rel: "noopener",
                    href: "https://docs.google.com/forms/d/e/1FAIpQLSfoLDgRBwXoIHhI-MondqYO4agtvIhom1vHnacgv5brhSKJAA/viewform?usp=pp_url&entry.90030775={game_id}",
                    "game registration"
                }
            }
            button {
                class: "{ACTION_BUTTON_CLASS} font-medium px-2 mx-2 mt-8",
                onclick: move |_| {
                    let nav = use_navigator(cx);
                    nav.push(Route::Index {});
                },
                "Return home to start a new game"
            }
        }
    )
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
    render!(
        div { class: "max-w-xlg grid space-y-4 mx-4 my-4",
            p { "Encountered an unexpected error. Try going back and trying again." }
            p { "Error: {msg}" }
        }
    )
}

fn WaitingForOtherPlayers<T>(cx: Scope<T>) -> Element {
    render!(
        div { class: "px-8 pt-8",
            div { class: "font-bold text-xl font-large text-black", "Waiting for other players to join..." }
            div { "Send the other player the url of this page for them to join" }
        }
    )
}

fn GameData<T>(cx: Scope<T>, gs: String, south_player: usize) -> Element {
    let gs = EuchreGameState::from(gs.as_str());
    let trump_details = gs.trump();

    let dealer_seat = match south_player {
        0 => "East",
        1 => "North",
        2 => "West",
        3 => "South",
        _ => "Error finding dealer seat",
    };

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

    let face_up = gs.face_up();
    let face_up_str = if let Some(card) = face_up {
        format!("Face up card is: {}", card.icon())
    } else {
        "Face up card not yet dealt".to_string()
    };

    let south_trick_wins = gs.trick_score()[south_player % 2];
    let east_trick_wins = gs.trick_score()[(south_player + 1) % 2];

    render!(
        div {
            div { class: "pt-8 font-bold text-xl font-large text-black", "Game information" }
            div { "Dealer is {dealer_seat}" }
            div { face_up_str }
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
        return None;
    }

    let last_trick = gs.last_trick();
    if let Some((starter, mut trick)) = last_trick {
        trick.rotate_left(4 - starter);

        render!(CardIcon(cx, trick[player]))
    } else {
        None
    }
}

fn RunningStats<T>(cx: Scope<T>, machine_score: usize, human_score: usize) -> Element {
    render!(
        div {
            div { class: "pt-8 font-bold text-xl font-large text-black", "Running stats" }
            div { class: "grid grid-cols-2",
                div { "Humans" }
                div { "Machines" }
                div { "{human_score}" }
                div { "{machine_score}" }
            }
        }
    )
}

fn PlayerStats<T>(cx: Scope<T>, players: Vec<Option<usize>>) -> Element {
    let num_humans = players.iter().filter(|x| x.is_some()).count();
    if num_humans > 1 {
        render!(
            div { class: "pt-8 font-bold text-xl font-large text-black", "Player details" }
            div { "North: Human" }
            div { "South: Human" }
            div { "East: Computer" }
            div { "West: Computer" }
        )
    } else {
        render!(
            div { class: "pt-8 font-bold text-xl font-large text-black", "Player details" }
            div { "North: Computer" }
            div { "South: Human" }
            div { "East: Computer" }
            div { "West: Computer" }
        )
    }
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

    let show_bids = matches!(
        game_data.display_state,
        WaitingBidClear { ready_players: _ }
    ) || gs.phase() == EPhase::Pickup
        || gs.phase() == EPhase::ChooseTrump;
    use GameProcessingState::*;

    cx.render(rsx! {

        div { class: "grid grid-cols-5 content-between gap-2",
            // North area
            div { class: "col-start-2 col-span-3 grid",
                div { class: "justify-self-center", north_label }
                OpponentHand(cx, gs.get_hand(north_player).len())
            }

            // Middle area
            div { class: "row-start-2",
                div { class: "text-center", west_label }
                OpponentHand(cx, gs.get_hand(west_player).len())
            }

            div { class: "col-span-3 grid grid-cols-3 items-center justify-items-center space-y-4",
                div { class: "col-start-2",
                    PlayedCard(cx, gs.played_card(north_player)),
                    LastTrick(cx, game_data.clone(), north_player),
                    if show_bids  {
                        Bids(cx, gs.clone(), north_player)
                    }
                }
                div { class: "row-start-2",
                    PlayedCard(cx, gs.played_card(west_player)),
                    LastTrick(cx, game_data.clone(), west_player),
                    if show_bids {
                        Bids(cx, gs.clone(), west_player)
                    }
                }
                div { class: "row-start-2 col-start-2 grid justify-items-center",
                    FaceUpCard(cx, gs.displayed_face_up_card()),
                    if matches!(game_data.display_state, WaitingBidClear { ready_players: _ }) {
                        FaceUpCard(cx, Some(gs.face_up().expect("invalid faceup call")))
                    }
                    if !gs.is_terminal() && !gs.is_trick_over() {
                        TurnTracker(cx, gs.clone(), south_player)
                    }
                    ClearButton(cx, game_data.clone().display_state, game_data.clone())
                }

                div { class: "row-start-2 col-start-3",
                    PlayedCard(cx, gs.played_card(east_player)),
                    LastTrick(cx, game_data.clone(), east_player),
                    if show_bids {
                        Bids(cx, gs.clone(), east_player)
                    }
                }

                div { class: "row-start-3 col-start-2",
                    PlayedCard(cx, gs.played_card(south_player)),
                    LastTrick(cx, game_data.clone(), south_player),
                    if show_bids {
                        Bids(cx, gs.clone(), south_player)
                    }
                }
            }
            div { class: "",
                div { class: "text-center", east_label }
                OpponentHand(cx, gs.get_hand(east_player).len())
            }

            // bottom area
            div { class: "row-start-3 col-span-5 grid justify-items-center",
                div { class: "self-end", south_label }
                PlayerActions(cx, gs.clone(), south_player, game_data.display_state.clone())
            }
        }
    })
}

fn ClearButton<T>(cx: Scope<T>, display_state: GameProcessingState, gd: GameData) -> Element {
    let action_task = use_coroutine_handle::<GameAction>(cx).expect("error getting action task");
    let player_id = get_player_id(cx).unwrap();
    let gs = gd.to_state();

    match display_state {
        GameProcessingState::WaitingTrickClear { ready_players }
        | GameProcessingState::WaitingBidClear { ready_players }
            if ready_players.contains(&player_id) =>
        {
            render!( div { class: "text-center", "waiting on other players..." } )
        }
        GameProcessingState::WaitingTrickClear { ready_players: _ } if gs.is_terminal() => {
            let south_player = TableLocation::south_player(player_id, &gd);
            let south_wins = gs.trick_score()[south_player % 2];
            let east_wins = gs.trick_score()[(south_player + 1) % 2];

            render!(
                div { "Hand over" }
                div { "North/South tricks: {south_wins}" }
                div { "East/West tricks: {east_wins}" }
                button {
                    class: "bg-white outline outline-black hover:bg-slate-100 focus:outline-none focus:ring focus:bg-slate-100 active:bg-slate-200 px-5 py-2 text-sm leading-5 rounded-full font-semibold text-black",
                    onclick: move |_| { action_task.send(GameAction::ReadyTrickClear) },
                    "Next hand"
                }
            )
        }
        GameProcessingState::WaitingTrickClear { ready_players: _ } => {
            let winner = TableLocation::to_location(player_id, &gd, gs.cur_player());
            render!(
                div { "{winner} wins" }
                button {
                    class: "bg-white outline outline-black hover:bg-slate-100 focus:outline-none focus:ring focus:bg-slate-100 active:bg-slate-200 px-5 py-2 text-sm leading-5 rounded-full font-semibold text-black",
                    onclick: move |_| { action_task.send(GameAction::ReadyTrickClear) },
                    "Clear trick"
                }
            )
        }
        GameProcessingState::WaitingBidClear { ready_players: _ } => {
            render!(
                button {
                    class: "bg-white outline outline-black hover:bg-slate-100 focus:outline-none focus:ring focus:bg-slate-100 active:bg-slate-200 px-5 py-2 text-sm leading-5 rounded-full font-semibold text-black",
                    onclick: move |_| { action_task.send(GameAction::ReadyBidClear) },
                    "Continue game"
                }
            )
        }
        _ => render!({}),
    }
}

fn Bids<T>(cx: Scope<T>, gs: EuchreGameState, player: Player) -> Element {
    use EAction::*;
    let bids: Vec<Option<&str>> = gs
        .bids()
        .iter()
        .map(|x| {
            x.map(|a| match a {
                Pass => "Pass",
                Pickup => "Pickup",
                Clubs => "Clubs",
                Spades => "Spades",
                Hearts => "Hearts",
                Diamonds => "Diamonds",
                _ => "Invalid bid",
            })
        })
        .collect();

    if bids[player].is_some() && bids[player + 4].is_some() {
        render!(
            div { bids[player] }
            div { bids[player + 4] }
        )
    } else if bids[player].is_some() {
        render!(bids[player])
    } else {
        None
    }
}

fn OpponentHand<T>(cx: Scope<T>, num_cards: usize) -> Element {
    let mut s = String::new();
    for _ in 0..num_cards {
        s.push('🂠')
    }

    cx.render(rsx! {
        div { class: "text-3xl lg:text-6xl", style: "text-align:center", s.as_str() }
    })
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
    cx.render(rsx! { div { class: "text-4xl lg:text-6xl", "{arrow}" } })
}

fn FaceUpCard<T>(cx: Scope<T>, c: Option<Card>) -> Element {
    if let Some(c) = c {
        cx.render(rsx! {CardIcon(cx, c)})
    } else {
        render!({})
    }
}

fn CardIcon<T>(cx: Scope<T>, c: Card) -> Element {
    use games::gamestates::euchre::actions::Suit::*;
    let color = match c.suit() {
        Clubs | Spades => "black",
        Hearts | Diamonds => "red",
    };

    cx.render(rsx! {
        span { class: "text-7xl", color: color, c.icon() }
    })
}

fn PlayerActions<T>(
    cx: Scope<T>,
    gs: EuchreGameState,
    south_player: usize,
    display_state: GameProcessingState,
) -> Element {
    if gs.is_chance_node() {
        return render!({});
    }

    let actions: Vec<EAction> = actions!(gs).into_iter().map(EAction::from).collect();
    let action_task = use_coroutine_handle::<GameAction>(cx).expect("error getting action task");

    if gs.cur_player() != south_player
        || matches!(
            display_state,
            GameProcessingState::WaitingBidClear { ready_players: _ }
                | GameProcessingState::WaitingTrickClear { ready_players: _ }
        )
    {
        // if not our turn, just show our hand
        let hand = gs.get_hand(south_player);
        render!(
            div { class: "grid gap-y-4 justify-items-center",
                div { class: "flex gap-x-4",
                    for c in hand.into_iter() {
                        CardIcon(cx, c)
                    }
                }
            }
        )
    } else if actions.contains(&EAction::Pickup) {
        let pickup_text = if south_player == 3 {
            "Take card"
        } else {
            "Tell dealer to take card"
        };
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
                        "{pickup_text}"
                    }

                    button {
                        class: "basis-1/2 text-xl {ACTION_BUTTON_CLASS}",
                        onclick: move |_| { action_task.send(GameAction::TakeAction(EAction::Pass.into())) },
                        "Pass"
                    }
                }
            }
        )
    } else if actions.contains(&EAction::Clubs) || actions.contains(&EAction::Spades) {
        // special case for choosing suit, we test for two suits in case one of them was the face up card and is not
        // a valid suit selection action
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
            div { class: "flex flex-wrap space-x-4",
                for (c , a) in hand.into_iter() {
                    ActionButton(cx, c, a)
                }
            }
        )
    }
}

fn ActionButton<T>(cx: Scope<T>, card: Card, action: Option<EAction>) -> Element {
    use games::gamestates::euchre::actions::Suit::*;
    let color = match card.suit() {
        Clubs | Spades => "text-black",
        Hearts | Diamonds => "text-red-500",
    };
    let action_task = use_coroutine_handle::<GameAction>(cx).expect("error getting action task");

    if let Some(a) = action {
        render!(
            button {
                class: "text-7xl py-2 {ACTION_BUTTON_CLASS} {color}",
                onclick: move |_| { action_task.send(GameAction::TakeAction(a.into())) },
                card.icon()
            }
        )
    } else {
        render!(
            button { disabled: "true", class: "text-7xl py-2 {ACTION_BUTTON_CLASS} {color}", card.icon() }
        )
    }
}
