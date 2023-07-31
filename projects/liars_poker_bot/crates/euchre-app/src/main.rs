#![allow(non_snake_case)]
use core::num;

use card_platypus::game::euchre::actions::Card;
// import the prelude to get access to the `rsx!` macro and the `Scope` and `Element` types
use dioxus::{
    html::{table, tr},
    prelude::*,
};

fn main() {
    // launch the web app
    dioxus_web::launch(App);
}

fn App(cx: Scope) -> Element {
    let mut count = use_state(cx, || 0);

    cx.render(rsx! {
        h1 { "High-Five counter: {count}" }
        table {
            tr {
                td {}
                td {}
                td { OpponentHand(cx, 4) }
            }
            tr {
                td {}
                td {}
                td { style: "text-align:center", PlayedCard(cx) }
            }
            tr {
                td { OpponentHand(cx, 4) }
                td { style: "text-align:center", PlayedCard(cx) }
                td {}
                td { style: "text-align:center", PlayedCard(cx) }
                td { OpponentHand(cx, 4) }
            }
            tr {
                td {}
                td {}
                td { style: "text-align:center",
                    button { color: "red", font_size: "75px", onclick: move |_| count += 1, Card::AC.icon() }
                    button { color: "red", font_size: "75px", onclick: move |_| count += 1, Card::NS.icon() }
                    CardButton(cx, Card::AS),
                    button { color: "red", font_size: "75px", onclick: move |_| count += 1, "🂡" }
                    button { color: "red", font_size: "75px", onclick: move |_| count += 1, "🂡" }
                }
            }
        }
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

fn PlayedCard(cx: Scope) -> Element {
    cx.render(rsx! { div { font_size: "75px", "🃙" } })
}
