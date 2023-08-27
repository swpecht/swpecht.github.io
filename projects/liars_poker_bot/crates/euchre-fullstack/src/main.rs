#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_fullstack::prelude::*;
use dioxus_router::prelude::*;
use euchre_fullstack::PlayerId;

use euchre_fullstack::ACTION_BUTTON_CLASS;

#[derive(Routable, Clone, PartialEq)]
enum Route {
    // if the current location is "/home", render the Home component
    #[route("/")]
    Index {},

    #[route("/game")]
    NewGame {},

    #[route("/game/:game_id")]
    InGame { game_id: String },

    #[route("/:..route")]
    NotFound { route: Vec<String> },
}

fn main() {
    let config = LaunchBuilder::new(App);
    #[cfg(feature = "ssr")]
    let config = config.incremental(
        IncrementalRendererConfig::default().invalidate_after(std::time::Duration::from_secs(120)),
    );

    config.launch();
}

fn App(cx: Scope) -> Element {
    #[cfg(feature = "web")]
    {
        const PLAYER_ID_KEY: &str = "PLAYER_ID";
        let local_storage = web_sys::window().unwrap().local_storage().unwrap().unwrap();

        let stored_id = local_storage.get_item(PLAYER_ID_KEY);

        if let Ok(Some(player_id)) = stored_id {
            use_shared_state_provider(cx, || PlayerId {
                id: player_id
                    .parse()
                    .expect("error parsing previously saved player id"),
            });
        } else {
            use rand::Rng;
            let player_id: usize = rand::thread_rng().gen();
            local_storage
                .set_item(PLAYER_ID_KEY, player_id.to_string().as_str())
                .expect("error storing player id");
            use_shared_state_provider(cx, || PlayerId { id: player_id });
        }
    }

    render! { Router::<Route> {} }
}

#[server]
async fn poll_state() -> Result<(), ServerFnError> {
    Ok(())
}

#[server]
async fn request_new_game(player_id: usize) -> Result<String, ServerFnError> {
    Ok("Todo".to_string())
}

#[inline_props]
fn NotFound(cx: Scope, route: Vec<String>) -> Element {
    render! {
        div { format!("Error: page not found: {:?}", route) }
    }
}

#[inline_props]
fn Index(cx: Scope) -> Element {
    render!(
        div { class: "max-w-xlg grid space-y-4 mx-4 my-4",
            p {
                p { class: "font-bold", "Play euchre against ai bots" }
                p {
                    "Euchre is a card game where you and a partner try to take more tricks than the opponent team.
                    The game is two phases. In the first, trump is decided. In the second, cards are played to take tricks"
                }
            }

            p {
                "For an overview of the rules, see Wikipedia: "
                a {
                    href: "https://en.wikipedia.org/wiki/Euchre",
                    class: "text-blue-600 visited:text-purple-600",
                    "Euchre"
                }
            }

            p {
                p { class: "font-bold", "Optionally play with a friend" }
                "You can play with a friend against the ai bots by sharing the url after you create a game. If you play alone,
                you'll get an ai agent as a teammate."
            }

            p {
                p { class: "font-bold",
                    "Agents use counter factual regret minimization (CFR) and perfect information monte carlo tree search (PIMCT)"
                }
                p {
                    "Using counter factual regret minimization (CFR) alone would result in a stronger agent.
                        But CFR cannot be naively applied to euchre -- the game is too large."
                }
            }
            p {
                "Instead, I use CFR for the first phase where trump is chosen and PIMCTS for the second phase where cards are played."
            }
            p {
                "More detail on the approach can be found on by blog: "
                a {
                    href: "https://fewworddotrick.com/project-log/2023/07/30/cfr-for-euchre.html",
                    class: "text-blue-600 visited:text-purple-600",
                    "CFR for euchre"
                }
            }

            div { class: "grid justify-items-center",
                button {
                    class: "{ACTION_BUTTON_CLASS} font-medium px-2",
                    onclick: move |_| {
                        let nav = use_navigator(cx);
                        nav.push("/game");
                    },
                    "New game"
                }
            }
        }
    )
}

#[inline_props]
fn NewGame(cx: Scope) -> Element {
    let player_id = use_shared_state::<PlayerId>(cx).unwrap().read().id;

    let new_game_response =
        use_future(cx, (), |_| async move { request_new_game(player_id).await });

    let nav = use_navigator(cx);
    match new_game_response.value() {
        Some(Ok(game_id)) => {
            // use replace here since we want to return to the index page
            // not the game page on back
            nav.replace(format!("/game/{}", game_id));
            render!({})
        }
        Some(Err(e)) => render!(
            div { format!("Error getting new game: {:?}", e) }
        ),
        None => render!( div { class: "text-xl", "Loading new game..." } ),
    }
}
