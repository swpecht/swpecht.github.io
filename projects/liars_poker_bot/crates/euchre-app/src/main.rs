#![allow(non_snake_case)]
use core::num;
use std::fmt::format;

use card_platypus::{
    actions,
    algorithms::{open_hand_solver::OpenHandSolver, pimcts::PIMCTSBot},
    game::{
        euchre::{
            actions::{Card, EAction},
            EuchreGameState,
        },
        GameState,
    },
};
// import the prelude to get access to the `rsx!` macro and the `Scope` and `Element` types
use dioxus::{
    html::{table, tr},
    prelude::*,
};
use rand::{rngs::StdRng, thread_rng, SeedableRng};

fn main() {
    // launch the web app
    dioxus_web::launch(App);
}

fn App(cx: Scope) -> Element {
    let mut count = use_state(cx, || 0);

    let mut agent = PIMCTSBot::new(
        20,
        OpenHandSolver::new_euchre(),
        StdRng::from_rng(thread_rng()).unwrap(),
    );

    let gs = use_state(cx, || {
        EuchreGameState::from("Qc9sTs9dAd|9cKsThQhTd|KcAsJhKhQd|AcJs9hAhJd|Qs")
    });

    let controlling_player = use_state(cx, || 0);

    let west_player = (controlling_player + 1) % 4;
    let north_player = (controlling_player + 2) % 4;
    let east_player = (controlling_player + 3) % 4;

    cx.render(rsx! {
        h1 { "High-Five counter: {controlling_player}" }
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
                    TurnTracker(cx, gs.get().clone(), *controlling_player.get())
                }
                td { style: "text-align:center", PlayedCard(cx, gs.played_card(east_player)) }
                td { OpponentHand(cx, gs.get_hand(east_player).len()) }
            }
            tr {
                td {}
                td {}
                td { style: "text-align:center", PlayedCard(cx, gs.played_card(**controlling_player)) }
            }
            tr {
                td {}
                td {}
                td { style: "text-align:center", PlayerHand(cx, gs.get_hand(**controlling_player)) }
            }
        }
        button {
            onclick: move |_| {
                gs.make_mut().apply_action(actions!(gs)[0]);
            },
            "go to next player: {controlling_player}"
        }
        GameLog(cx, gs.get().clone())
    })
}

fn OpponentHand(cx: Scope, num_cards: usize) -> Element {
    let mut s = String::new();
    for _ in 0..num_cards {
        s.push('🂠')
    }

    cx.render(rsx! {
        div { font_size: "75px", s.as_str() }
    })
}

fn PlayerHand(cx: Scope, hand: Vec<Card>) -> Element {
    cx.render(rsx! {
        if !hand.is_empty() {CardButton(cx, hand[0]) }
        if hand.len() > 1 {CardButton(cx, hand[1]) }
        if hand.len() > 2 {CardButton(cx, hand[2]) }
        if hand.len() > 3 {CardButton(cx, hand[3]) }
        if hand.len() > 4 {CardButton(cx, hand[4]) }
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

fn PlayedCard(cx: Scope, c: Option<Card>) -> Element {
    if let Some(c) = c {
        cx.render(rsx! {CardIcon(cx, c)})
    } else {
        cx.render(rsx! { div {} })
    }
}

fn TurnTracker(cx: Scope, gs: EuchreGameState, controlling_player: usize) -> Element {
    let arrow = match gs.cur_player() {
        x if x == (controlling_player + 1) % 4 => "←",
        x if x == (controlling_player + 2) % 4 => "↑",
        x if x == (controlling_player + 3) % 4 => "→",
        _ => "↓",
    };
    cx.render(rsx! { div { font_size: "60px", "{arrow}" } })
}

fn FaceUpCard(cx: Scope, c: Option<Card>) -> Element {
    if let Some(c) = c {
        cx.render(rsx! {CardIcon(cx, c)})
    } else {
        cx.render(rsx! { div {} })
    }
}

fn CardIcon(cx: Scope, c: Card) -> Element {
    use card_platypus::game::euchre::actions::Suit::*;
    let color = match c.suit() {
        Clubs | Spades => "black",
        Hearts | Diamonds => "red",
    };

    cx.render(rsx! {
        div { color: color, font_size: "75px", c.icon() }
    })
}

fn GameLog(cx: Scope, gs: EuchreGameState) -> Element {
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

fn PlayerAction(cx: Scope, gs: EuchreGameState) -> Element {
    todo!()
}
