//! Boilerplate shared by the card-game web frontends (`euchre_server`,
//! `oh_hell_server`).
//!
//! Just the pieces that are literally identical between the two servers:
//! the cookie-based player-id helper, the page shell, two Maud form
//! builders, and the "waiting for players" view. Anything that depends
//! on a specific game's state or rendering stays in the game's own
//! crate.

use actix_web::{
    cookie::{time::Duration as CookieDuration, Cookie},
    HttpRequest, HttpResponse,
};
use maud::{html, Markup, DOCTYPE};
use rand::{rng, RngExt};
use uuid::Uuid;

/// Return the persisted player id from the named cookie, or mint a fresh
/// random one. The `Option<Cookie>` is `Some` only when a new id was
/// minted and the caller should attach it to the response.
pub fn get_or_set_player_id(
    req: &HttpRequest,
    cookie_name: &'static str,
) -> (usize, Option<Cookie<'static>>) {
    if let Some(c) = req.cookie(cookie_name) {
        if let Ok(id) = c.value().parse::<usize>() {
            return (id, None);
        }
    }
    let id: usize = rng().random_range(1..u32::MAX as usize);
    let cookie = Cookie::build(cookie_name, id.to_string())
        .path("/")
        .max_age(CookieDuration::days(30))
        .http_only(true)
        .finish();
    (id, Some(cookie))
}

/// Build the page shell: doctype, head with htmx + tailwind CDN scripts,
/// a centred body container, and the caller-supplied `body` markup.
pub fn layout(title: &str, body: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="UTF-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) }
                script src="https://unpkg.com/htmx.org@1.9.12" {}
                script src="https://cdn.tailwindcss.com" {}
            }
            body class="font-sans p-4 max-w-5xl mx-auto text-black" {
                (body)
            }
        }
    }
}

/// Wrap markup as a 200 HTML response, optionally setting a cookie. The
/// caller passes the cookie returned by [`get_or_set_player_id`].
pub fn html_response(markup: Markup, cookie: Option<Cookie<'static>>) -> HttpResponse {
    let mut resp = HttpResponse::Ok();
    resp.content_type("text/html; charset=utf-8");
    if let Some(c) = cookie {
        resp.cookie(c);
    }
    resp.body(markup.into_string())
}

/// Render the "share this url with a friend" page shown before all
/// expected human seats are filled.
pub fn render_waiting_players(game_id: &Uuid) -> Markup {
    html! {
        div class="p-4 grid gap-2" {
            h2 class="text-xl font-bold" { "Waiting for other players to join..." }
            p { "Send the other player the url of this page for them to join." }
            p class="text-sm text-gray-600" {
                "Game id: " code { (game_id) }
            }
        }
    }
}

/// htmx form that posts a `kind=<kind>` body to `/game/{id}/action` and
/// swaps the `#game` element with the response. Used for the
/// "Continue / Clear trick / Next hand" buttons.
pub fn clear_form(kind: &str, label: &str, game_id: &Uuid) -> Markup {
    html! {
        form
            hx-post={ "/game/" (game_id) "/action" }
            hx-target="#game"
            hx-swap="innerHTML"
            class="inline"
        {
            input type="hidden" name="kind" value=(kind);
            button
                type="submit"
                class="bg-white outline outline-black hover:bg-slate-100 focus:outline-none focus:ring focus:bg-slate-100 active:bg-slate-200 rounded-full px-5 py-2 text-sm leading-5 font-semibold"
            { (label) }
        }
    }
}

/// htmx form that posts a `kind=take` + `action=<raw>` body. Used for
/// per-card and per-bid buttons.
pub fn action_form_button(raw_action: u32, label: &str, classes: &str, game_id: &Uuid) -> Markup {
    html! {
        form
            hx-post={ "/game/" (game_id) "/action" }
            hx-target="#game"
            hx-swap="innerHTML"
            class="inline"
        {
            input type="hidden" name="kind" value="take";
            input type="hidden" name="action" value=(raw_action);
            button type="submit" class=(classes) { (label) }
        }
    }
}
