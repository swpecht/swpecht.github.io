//! htmx + Maud prototype frontend for the euchre server.
//!
//! This module demonstrates an alternative to the Dioxus WASM frontend:
//! the server renders HTML directly with Maud templates, and htmx handles
//! polling and form submission. The existing JSON API under `/api` is
//! untouched.
//!
//! Mount point: `/htmx`. Visit `/htmx` in a browser to try it.

use std::str::FromStr;

use actix_web::{
    cookie::{time::Duration as CookieDuration, Cookie},
    web, HttpRequest, HttpResponse, Responder,
};
use client_server_messages::{GameData, GameProcessingState};
use games::{
    actions,
    gamestates::euchre::{
        actions::{Card, EAction, Suit},
        EPhase, EuchreGameState,
    },
    GameState, Player,
};
use maud::{html, Markup, DOCTYPE};
use rand::{rng, RngExt};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    handle_ready_clear, handle_register_player, handle_take_action, new_game, progress_game,
    AppState,
};

const PLAYER_COOKIE: &str = "euchre_player_id";

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/htmx", web::get().to(index))
        .route("/htmx/new", web::post().to(new_game_handler))
        .route("/htmx/game/{id}", web::get().to(game_page))
        .route("/htmx/game/{id}/view", web::get().to(game_view))
        .route("/htmx/game/{id}/action", web::post().to(game_action));
}

// ---------- Player ID cookie ----------

fn get_or_set_player_id(req: &HttpRequest) -> (usize, Option<Cookie<'static>>) {
    if let Some(c) = req.cookie(PLAYER_COOKIE) {
        if let Ok(id) = c.value().parse::<usize>() {
            return (id, None);
        }
    }
    let id: usize = rng().random_range(1..u32::MAX as usize);
    let cookie = Cookie::build(PLAYER_COOKIE, id.to_string())
        .path("/")
        .max_age(CookieDuration::days(30))
        .http_only(true)
        .finish();
    (id, Some(cookie))
}

fn with_cookie(markup: Markup, cookie: Option<Cookie<'static>>) -> HttpResponse {
    let mut resp = HttpResponse::Ok();
    resp.content_type("text/html; charset=utf-8");
    if let Some(c) = cookie {
        resp.cookie(c);
    }
    resp.body(markup.into_string())
}

// ---------- Shared layout ----------

fn layout(title: &str, body: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html {
            head {
                meta charset="UTF-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) }
                script src="https://unpkg.com/htmx.org@1.9.12" {}
                script src="https://cdn.tailwindcss.com" {}
            }
            body class="font-sans p-4 max-w-4xl mx-auto" {
                (body)
            }
        }
    }
}

// ---------- Index ----------

async fn index(req: HttpRequest) -> impl Responder {
    let (_pid, cookie) = get_or_set_player_id(&req);
    let body = html! {
        h1 class="text-2xl font-bold mb-4" { "Euchre (htmx prototype)" }
        p class="mb-4" {
            "Play euchre against CFR/PIMCTS bots. This page is server-rendered "
            "HTML driven by htmx — no WASM, no client-side routing."
        }
        form method="post" action="/htmx/new" class="flex gap-2" {
            input type="hidden" name="min_players" value="1";
            button
                type="submit"
                class="bg-white outline outline-black hover:bg-slate-100 rounded-lg px-4 py-2"
            {
                "Play with AI partner"
            }
        }
    };
    with_cookie(layout("Euchre", body), cookie)
}

// ---------- New game ----------

#[derive(Deserialize)]
struct NewGameForm {
    min_players: usize,
}

async fn new_game_handler(
    req: HttpRequest,
    form: web::Form<NewGameForm>,
    data: web::Data<AppState>,
) -> impl Responder {
    let (player_id, cookie) = get_or_set_player_id(&req);
    let game_id = Uuid::new_v4();
    let gs = new_game();

    let mut game_data = GameData::new(gs, player_id, form.min_players);
    game_data.players.rotate_right(rng().random_range(0..4));
    progress_game(&mut game_data, &data.bot, &game_id);
    data.games.lock().unwrap().insert(game_id, game_data);

    let url = format!("/htmx/game/{}", game_id);
    let mut resp = HttpResponse::SeeOther();
    resp.insert_header(("Location", url));
    if let Some(c) = cookie {
        resp.cookie(c);
    }
    resp.finish()
}

// ---------- Full game page ----------

async fn game_page(
    req: HttpRequest,
    path: web::Path<String>,
    data: web::Data<AppState>,
) -> impl Responder {
    let (player_id, cookie) = get_or_set_player_id(&req);
    let game_id = match Uuid::from_str(&path.into_inner()) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().body("bad game id"),
    };

    {
        let mut games = data.games.lock().unwrap();
        let Some(gd) = games.get_mut(&game_id) else {
            return HttpResponse::NotFound().body("game not found");
        };
        // auto-register as second human if there's an open slot
        if !gd.players.contains(&Some(player_id))
            && gd.players.iter().filter(|x| x.is_some()).count() == 1
        {
            let _ = handle_register_player(gd, player_id);
            progress_game(gd, &data.bot, &game_id);
        }
    }

    let games = data.games.lock().unwrap();
    let Some(gd) = games.get(&game_id) else {
        return HttpResponse::NotFound().body("game not found");
    };

    let view = render_game_view(gd, player_id, &game_id);
    let body = html! {
        // The polling container. Every 3s it re-fetches the view fragment
        // and swaps its own innerHTML. One HTML attribute replaces the
        // entire Dioxus polling coroutine.
        div
            id="game"
            hx-get={ "/htmx/game/" (game_id) "/view" }
            hx-trigger="every 3s"
            hx-swap="innerHTML"
        {
            (view)
        }
    };
    with_cookie(layout("Euchre", body), cookie)
}

// ---------- Polling fragment ----------

async fn game_view(
    req: HttpRequest,
    path: web::Path<String>,
    data: web::Data<AppState>,
) -> impl Responder {
    let (player_id, _) = get_or_set_player_id(&req);
    let game_id = match Uuid::from_str(&path.into_inner()) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().body("bad game id"),
    };
    let games = data.games.lock().unwrap();
    let Some(gd) = games.get(&game_id) else {
        return HttpResponse::NotFound().body("game not found");
    };
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(render_game_view(gd, player_id, &game_id).into_string())
}

// ---------- Action handler ----------

#[derive(Deserialize)]
struct ActionForm {
    kind: String,
    // For TakeAction: the EAction discriminant as a u32 string
    action: Option<u32>,
}

async fn game_action(
    req: HttpRequest,
    path: web::Path<String>,
    form: web::Form<ActionForm>,
    data: web::Data<AppState>,
) -> impl Responder {
    let (player_id, _) = get_or_set_player_id(&req);
    let game_id = match Uuid::from_str(&path.into_inner()) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().body("bad game id"),
    };

    let mut games = data.games.lock().unwrap();
    let Some(gd) = games.get_mut(&game_id) else {
        return HttpResponse::NotFound().body("game not found");
    };

    let result: Result<(), HttpResponse> = match form.kind.as_str() {
        "take" => {
            let Some(raw) = form.action else {
                return HttpResponse::BadRequest().body("missing action");
            };
            let eaction: EAction = raw.into();
            handle_take_action(gd, eaction.into(), player_id)
        }
        "ready_trick" | "ready_bid" => handle_ready_clear(gd, player_id),
        other => Err(HttpResponse::BadRequest().body(format!("unknown kind: {other}"))),
    };

    if let Err(e) = result {
        return e;
    }

    progress_game(gd, &data.bot, &game_id);
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(render_game_view(gd, player_id, &game_id).into_string())
}

// ---------- Rendering ----------

fn render_game_view(gd: &GameData, player_id: usize, game_id: &Uuid) -> Markup {
    use GameProcessingState::*;

    match &gd.display_state {
        WaitingPlayerJoin { .. } => html! {
            div class="p-4" {
                h2 class="text-xl font-bold" { "Waiting for players to join..." }
                p { "Share this URL: " code { "/htmx/game/" (game_id) } }
            }
        },
        GameOver => html! {
            div class="p-4" {
                h2 class="text-xl font-bold" { "Game over" }
                p { "Humans: " (gd.human_score) " — Machines: " (gd.computer_score) }
                a href="/htmx" class="text-blue-600 underline" { "Play again" }
            }
        },
        _ => render_active_game(gd, player_id, game_id),
    }
}

fn card_color(suit: Suit) -> &'static str {
    match suit {
        Suit::Clubs | Suit::Spades => "text-black",
        Suit::Hearts | Suit::Diamonds => "text-red-500",
    }
}

fn seat_name(player: Player, south: Player) -> &'static str {
    match (player + 4 - south) % 4 {
        0 => "South",
        1 => "West",
        2 => "North",
        _ => "East",
    }
}

fn render_active_game(gd: &GameData, player_id: usize, game_id: &Uuid) -> Markup {
    let gs = EuchreGameState::from(gd.gs.as_str());
    let south_player = gd
        .players
        .iter()
        .position(|x| *x == Some(player_id))
        .unwrap_or(0);

    let trump_str = match gs.trump() {
        Some((suit, caller)) => format!(
            "Trump: {} (called by {})",
            suit.icon(),
            seat_name(caller, south_player)
        ),
        None => "Trump not yet called".to_string(),
    };
    let face_up_str = gs
        .face_up()
        .map(|c| format!("Face up: {}", c.icon()))
        .unwrap_or_else(|| "Face up not yet dealt".to_string());
    let south_tricks = gs.trick_score()[south_player % 2];
    let opp_tricks = gs.trick_score()[(south_player + 1) % 2];

    html! {
        div class="grid grid-cols-1 md:grid-cols-3 gap-4" {
            // Left: play area
            div class="md:col-span-2 border p-4 rounded" {
                h2 class="text-lg font-bold mb-2" { "Table" }
                (render_opponents(&gs, south_player))
                (render_middle(&gs, gd, game_id))
                (render_hand_and_actions(&gs, gd, south_player, game_id))
            }
            // Right: info sidebar
            div class="border p-4 rounded" {
                h2 class="text-lg font-bold" { "Game" }
                p { (face_up_str) }
                p { (trump_str) }
                p class="font-bold mt-2" { "Tricks" }
                p { "You/Partner: " (south_tricks) }
                p { "Opponents: " (opp_tricks) }
                h2 class="text-lg font-bold mt-4" { "Score" }
                p { "Humans: " (gd.human_score) }
                p { "Machines: " (gd.computer_score) }
            }
        }
    }
}

fn render_opponents(gs: &EuchreGameState, south: Player) -> Markup {
    let north = (south + 2) % 4;
    let west = (south + 1) % 4;
    let east = (south + 3) % 4;
    let back = |n: usize| "🂠".repeat(n);
    html! {
        div class="text-center mb-2" {
            div { "North" }
            div class="text-3xl" { (back(gs.get_hand(north).len())) }
            div class="text-xs text-gray-500" {
                @if let Some(c) = gs.played_card(north) { "played: " (c.icon()) }
            }
        }
        div class="flex justify-between items-center my-2" {
            div class="text-center" {
                div { "West" }
                div class="text-3xl" { (back(gs.get_hand(west).len())) }
                div class="text-xs text-gray-500" {
                    @if let Some(c) = gs.played_card(west) { "played: " (c.icon()) }
                }
            }
            div class="text-center" {
                div { "East" }
                div class="text-3xl" { (back(gs.get_hand(east).len())) }
                div class="text-xs text-gray-500" {
                    @if let Some(c) = gs.played_card(east) { "played: " (c.icon()) }
                }
            }
        }
    }
}

fn render_middle(gs: &EuchreGameState, gd: &GameData, game_id: &Uuid) -> Markup {
    use GameProcessingState::*;
    html! {
        div class="text-center my-4" {
            @if let Some(c) = gs.displayed_face_up_card() {
                div class="text-5xl" { (c.icon()) }
            }
            @match &gd.display_state {
                WaitingTrickClear { .. } => (clear_button("ready_trick", "Clear trick", game_id)),
                WaitingBidClear { .. } => (clear_button("ready_bid", "Continue", game_id)),
                _ => {}
            }
        }
    }
}

fn clear_button(kind: &str, label: &str, game_id: &Uuid) -> Markup {
    html! {
        form
            hx-post={ "/htmx/game/" (game_id) "/action" }
            hx-target="#game"
            hx-swap="innerHTML"
            class="inline"
        {
            input type="hidden" name="kind" value=(kind);
            button
                type="submit"
                class="bg-white outline outline-black hover:bg-slate-100 rounded-lg px-4 py-2"
            { (label) }
        }
    }
}

fn render_hand_and_actions(
    gs: &EuchreGameState,
    gd: &GameData,
    south: Player,
    game_id: &Uuid,
) -> Markup {
    if gs.is_chance_node() {
        return html! {};
    }

    let hand = gs.get_hand(south);
    let is_waiting_clear = matches!(
        gd.display_state,
        GameProcessingState::WaitingTrickClear { .. } | GameProcessingState::WaitingBidClear { .. }
    );
    let our_turn = gs.cur_player() == south
        && matches!(gd.display_state, GameProcessingState::WaitingHumanMove);
    let legal: Vec<EAction> = actions!(gs).into_iter().map(EAction::from).collect();

    html! {
        div class="mt-4" {
            h3 class="font-bold" { "Your hand" }
            div class="flex gap-2 flex-wrap mt-2" {
                @for c in &hand {
                    @let is_playable = our_turn
                        && !is_waiting_clear
                        && legal.iter().any(|a| a.card() == *c);
                    (card_button(c, is_playable, game_id))
                }
            }
            @if our_turn {
                (render_bid_options(&legal, game_id))
            }
        }
    }
}

fn card_button(c: &Card, playable: bool, game_id: &Uuid) -> Markup {
    let color = card_color(c.suit());
    let classes = format!(
        "text-5xl px-2 py-1 rounded {color} {}",
        if playable {
            "outline outline-black hover:bg-slate-100"
        } else {
            "opacity-60"
        }
    );
    if playable {
        let eaction: u32 = EAction::from(*c) as u32;
        html! {
            form
                hx-post={ "/htmx/game/" (game_id) "/action" }
                hx-target="#game"
                hx-swap="innerHTML"
                class="inline"
            {
                input type="hidden" name="kind" value="take";
                input type="hidden" name="action" value=(eaction);
                button type="submit" class=(classes) { (c.icon()) }
            }
        }
    } else {
        html! { span class=(classes) { (c.icon()) } }
    }
}

fn render_bid_options(legal: &[EAction], game_id: &Uuid) -> Markup {
    use EAction::*;
    // Only show bid/suit choice buttons when they're legal in the current phase.
    let bid_actions: Vec<(EAction, &str, &str)> = [
        (Pickup, "Pickup", ""),
        (Pass, "Pass", ""),
        (Spades, "♠", "text-black"),
        (Clubs, "♣", "text-black"),
        (Hearts, "♥", "text-red-500"),
        (Diamonds, "♦", "text-red-500"),
    ]
    .into_iter()
    .filter(|(a, _, _)| legal.contains(a))
    .collect();

    if bid_actions.is_empty() {
        return html! {};
    }

    html! {
        div class="mt-4 flex gap-2 flex-wrap" {
            @for (a, label, color) in bid_actions {
                @let raw: u32 = a as u32;
                form
                    hx-post={ "/htmx/game/" (game_id) "/action" }
                    hx-target="#game"
                    hx-swap="innerHTML"
                    class="inline"
                {
                    input type="hidden" name="kind" value="take";
                    input type="hidden" name="action" value=(raw);
                    button
                        type="submit"
                        class={ "outline outline-black bg-white hover:bg-slate-100 rounded-lg px-3 py-2 text-xl " (color) }
                    { (label) }
                }
            }
        }
    }
}

// Silence unused-import warning for EPhase; it's available for future rendering needs.
#[allow(dead_code)]
fn _phase_hint(gs: &EuchreGameState) -> EPhase {
    gs.phase()
}
