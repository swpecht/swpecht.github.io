#![allow(non_snake_case)]
use core::num;

// import the prelude to get access to the `rsx!` macro and the `Scope` and `Element` types
use dioxus::{
    html::{table, tr},
    prelude::*,
};

fn main() {
    // launch the web app
    dioxus_web::launch(App);
}

// create a component that renders a div with the text "Hello, world!"
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
                    button { color: "red", font_size: "75px", onclick: move |_| count += 1, "🂡" }
                    button { color: "red", font_size: "75px", onclick: move |_| count += 1, "🂡" }
                    button { color: "red", font_size: "75px", onclick: move |_| count += 1, "🂡" }
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

fn PlayedCard(cx: Scope) -> Element {
    cx.render(rsx! { div { font_size: "75px", "🃙" } })
}
