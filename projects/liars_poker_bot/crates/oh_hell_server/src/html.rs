//! Server-rendered HTML frontend for the Oh Hell server. Built with
//! Maud + htmx (no client-side framework). Mirrors euchre_server/html.rs
//! but adapted for Oh Hell's bidding-by-integer phase and the
//! variable-seat layout.

use std::str::FromStr;

use actix_web::{web, HttpRequest, HttpResponse, Responder};
use games::{
    actions,
    gamestates::oh_hell::{
        actions::{OHAction, OHCard, OHSuit, BID_BASE},
        OHPhase, OhHellGameState,
    },
    Action, GameState, Player,
};
use maud::{html, Markup};
use serde::Deserialize;
use uuid::Uuid;
use web_common::{
    action_form_button, clear_form, get_or_set_player_id as web_get_or_set_player_id,
    html_response, layout, render_waiting_players,
};

use crate::{
    default_hand_sequence, handle_ready_clear, handle_register_player, handle_take_action,
    new_hand, progress_game, strategy_for_hand_size, AppState, GameData, GameProcessingState,
    NUM_PLAYERS,
};

const PLAYER_COOKIE: &str = "oh_hell_player_id";

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/", web::get().to(index))
        .route("/new", web::post().to(new_game_handler))
        .route("/game/{id}", web::get().to(game_page))
        .route("/game/{id}/view", web::get().to(game_view))
        .route("/game/{id}/action", web::post().to(game_action));
}

fn get_or_set_player_id(
    req: &HttpRequest,
) -> (usize, Option<actix_web::cookie::Cookie<'static>>) {
    web_get_or_set_player_id(req, PLAYER_COOKIE)
}

// ---------- Landing page ----------

async fn index(req: HttpRequest) -> impl Responder {
    let (_pid, cookie) = get_or_set_player_id(&req);
    let body = html! {
        div class="max-w-2xl mx-auto grid gap-4 mb-8" {
            h1 class="text-2xl font-bold" { "Play Oh Hell against ai bots" }
            p {
                "Oh Hell (also called Oh Pshaw, Bust, Blackout) is a trick-taking "
                "card game. Each hand, players are dealt the same number of cards, "
                "a card is flipped for trump, then players bid the exact number of "
                "tricks they think they'll take. If you take exactly your bid, you "
                "score 10 + bid; otherwise you score 0."
            }
            p {
                "For an overview of the rules, see Wikipedia: "
                a
                    class="text-blue-600 visited:text-purple-600 underline"
                    href="https://en.wikipedia.org/wiki/Oh_hell"
                    target="_blank"
                    rel="noopener"
                { "Oh Hell" }
            }
            p {
                "This server runs a "
                span class="font-bold" { (NUM_PLAYERS) "-player" }
                " variant on the canonical Wikipedia schedule: deal 10 cards "
                "to each player, then 9, all the way down to 1, then back up to 10. "
                "The hand with the highest cumulative score after all 19 hands wins."
            }
            p {
                span class="font-bold" { "Common scoring. " }
                "Each player gets 1 point per trick taken, plus a 10-point bonus "
                "if their bid matched the number of tricks taken exactly. "
                "The dealer's bid is constrained so the total of all bids in a "
                "hand can never equal the number of tricks (\"the hook\")."
            }
            p {
                span class="font-bold" { "Optionally play with a friend. " }
                "You can play with a friend against the ai bots by sharing the url "
                "after you create a game. The remaining seats are filled by ai bots."
            }
            (strategy_table())
        }
        div class="grid justify-items-center gap-2" {
            form method="post" action="/new" class="inline" {
                input type="hidden" name="num_humans" value="1";
                button
                    type="submit"
                    class="bg-white outline outline-black hover:bg-slate-100 rounded-lg px-4 py-2 font-medium"
                { "Play solo vs bots" }
            }
            form method="post" action="/new" class="inline" {
                input type="hidden" name="num_humans" value="2";
                button
                    type="submit"
                    class="bg-white outline outline-black hover:bg-slate-100 rounded-lg px-4 py-2 font-medium"
                { "Play with a human friend" }
            }
        }
    };
    html_response(layout("Oh Hell", body), cookie)
}

// ---------- New game ----------

#[derive(Deserialize)]
struct NewGameForm {
    num_humans: usize,
}

async fn new_game_handler(
    req: HttpRequest,
    form: web::Form<NewGameForm>,
    data: web::Data<AppState>,
) -> impl Responder {
    let (player_id, cookie) = get_or_set_player_id(&req);
    let num_humans = form.num_humans.clamp(1, 2);
    let game_id = Uuid::new_v4();

    let sequence = default_hand_sequence();
    let first_size = sequence[0];
    let mut gd = GameData::new(
        new_hand(first_size),
        player_id,
        num_humans,
        NUM_PLAYERS,
        sequence,
    );
    progress_game(&mut gd, &data.bot, &game_id);
    data.games.lock().unwrap().insert(game_id, gd);

    log::info!("new oh hell game created: {game_id} (humans: {num_humans})");

    let url = format!("/game/{}", game_id);
    let mut resp = HttpResponse::SeeOther();
    resp.insert_header(("Location", url));
    if let Some(c) = cookie {
        resp.cookie(c);
    }
    resp.finish()
}

// ---------- Game page (with htmx polling) ----------

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
        // Auto-register a visitor who arrived via shared link if the
        // game still has room for another human.
        if !gd.players.contains(&Some(player_id))
            && gd.players.iter().filter(|x| x.is_some()).count() < gd.num_humans
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
        div
            id="game"
            hx-get={ "/game/" (game_id) "/view" }
            hx-trigger="every 2s"
            hx-swap="innerHTML"
        {
            (view)
        }
    };
    html_response(layout("Oh Hell", body), cookie)
}

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
    /// For `kind=take`: the raw action discriminant as a u32.
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
            let a = Action(raw as u8);
            handle_take_action(gd, a, player_id)
        }
        "ready_trick" | "ready_bid" | "ready_hand" => handle_ready_clear(gd, player_id),
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

// ---------- Strategy table (landing page) ----------

/// Render the bot strategy per hand-size as a small table. Driven off
/// `strategy_for_hand_size` so as the bot mix evolves the landing page
/// stays accurate. The row set comes from the canonical hand schedule
/// deduped (10..1 covers everything).
fn strategy_table() -> Markup {
    let mut sizes: Vec<usize> = default_hand_sequence();
    sizes.sort();
    sizes.dedup();
    sizes.reverse(); // 10 down to 1
    html! {
        div class="grid gap-1" {
            div class="font-bold" { "Bot strategy per hand size" }
            div class="grid grid-cols-2 gap-x-4 text-sm" {
                div class="font-semibold" { "Hand size (tricks)" }
                div class="font-semibold" { "Strategy" }
                @for n in sizes {
                    div { (n) }
                    div { (strategy_for_hand_size(n)) }
                }
            }
        }
    }
}

// ---------- Rendering ----------

pub(crate) fn render_game_view(gd: &GameData, player_id: usize, game_id: &Uuid) -> Markup {
    use GameProcessingState::*;
    match &gd.display_state {
        WaitingPlayerJoin { .. } => render_waiting_players(game_id),
        GameOver => render_game_over(gd, player_id, game_id),
        _ => render_active_game(gd, player_id, game_id),
    }
}

fn render_game_over(gd: &GameData, player_id: usize, _game_id: &Uuid) -> Markup {
    let winner = gd
        .scores
        .iter()
        .enumerate()
        .max_by_key(|(_, s)| **s)
        .map(|(i, _)| i)
        .unwrap_or(0);
    let south = gd
        .players
        .iter()
        .position(|x| *x == Some(player_id))
        .unwrap_or(0);
    let np = gd.players.len();
    html! {
        div class="px-8 pt-8 grid gap-4" {
            div class="font-bold text-xl" { "Thanks for playing!" }
            div { (seat_label(winner, south, gd, np)) " wins." }
            div {
                "Final scores:"
                @for (i, s) in gd.scores.iter().enumerate() {
                    span class="ml-2" {
                        (seat_label(i, south, gd, np)) ": " (s)
                    }
                }
            }
            a
                href="/"
                class="bg-white outline outline-black hover:bg-slate-100 rounded-lg px-4 py-2 mt-4 font-medium w-fit"
            { "Return home to start a new game" }
        }
    }
}

// ---------- Active game ----------

fn render_active_game(gd: &GameData, player_id: usize, game_id: &Uuid) -> Markup {
    let gs = &gd.gs;
    // Pick the viewer's seat (south). If the viewer isn't in the game,
    // default to seat 0 so they get a sensible spectator view.
    let south = gd
        .players
        .iter()
        .position(|x| *x == Some(player_id))
        .unwrap_or(0);

    html! {
        div class="grid sm:flex sm:flex-row gap-4" {
            div class="sm:basis-3/4" {
                (render_play_area(gs, gd, south, game_id))
            }
            div class="sm:basis-1/4 grid gap-4" {
                (render_game_info(gs, gd, south))
                (render_score_table(gd, south))
                (render_seat_table(gd, south))
            }
        }
    }
}

fn render_game_info(gs: &OhHellGameState, gd: &GameData, south: Player) -> Markup {
    let trump_line = match gs.trump_suit() {
        Some(s) => format!("Trump suit: {}", suit_icon(s)),
        None => "Trump not yet revealed".to_string(),
    };
    let face_up_line = match gs.face_up() {
        Some(c) => format!("Face up card: {}", card_text(c)),
        None => "Face up card not yet dealt".to_string(),
    };
    let phase_line = match gs.phase() {
        OHPhase::DealHands | OHPhase::DealFaceUp => "Dealing".to_string(),
        OHPhase::Bidding => "Bidding".to_string(),
        OHPhase::Play => "Playing".to_string(),
        OHPhase::Terminal => "Hand complete".to_string(),
    };
    let np = gs.num_players();
    html! {
        div {
            div class="pt-2 font-bold text-xl" { "Hand info" }
            div { "Phase: " (phase_line) }
            div { (face_up_line) }
            div { (trump_line) }
            div class="font-bold pt-2" { "Bids / tricks won this hand:" }
            div class="grid grid-cols-3 gap-x-2" {
                div class="font-semibold" { "Seat" }
                div class="font-semibold" { "Bid" }
                div class="font-semibold" { "Won" }
                @for p in 0..np {
                    div { (seat_label(p, south, gd, np)) }
                    div { (display_bid(gs.bids()[p])) }
                    div { (gs.tricks_won()[p]) }
                }
            }
        }
    }
}

fn render_score_table(gd: &GameData, south: Player) -> Markup {
    let total = gd.hand_sequence.len();
    let hand_num = (gd.hand_idx + 1).min(total);
    let np = gd.players.len();
    html! {
        div {
            div class="font-bold text-xl" { "Cumulative score" }
            div class="grid grid-cols-2 gap-x-2" {
                @for (i, s) in gd.scores.iter().enumerate() {
                    div { (seat_label(i, south, gd, np)) }
                    div { (s) }
                }
            }
            div class="text-xs text-gray-500 pt-1" {
                "Hand " (hand_num) " of " (total)
                " · this hand: " (gd.gs.n_tricks()) " tricks"
            }
        }
    }
}

fn render_seat_table(gd: &GameData, south: Player) -> Markup {
    let np = gd.players.len();
    html! {
        div {
            div class="font-bold text-xl" { "Seat details" }
            @for p in 0..np {
                div {
                    (seat_label(p, south, gd, np))
                    ": "
                    @if gd.players[p].is_some() {
                        "Human"
                    } @else {
                        "Computer"
                    }
                }
            }
        }
    }
}

fn render_play_area(
    gs: &OhHellGameState,
    gd: &GameData,
    south: Player,
    game_id: &Uuid,
) -> Markup {
    // Order seats around the south viewer: south then clockwise.
    let np = gs.num_players();
    html! {
        div class="grid gap-4" {
            // Top: other seats with their played cards
            div class={ "grid gap-2 grid-cols-" (np - 1) } {
                @for offset in 1..np {
                    @let p = (south + offset) % np;
                    div class="flex flex-col items-center gap-1" {
                        div class="text-sm font-semibold" { (seat_label(p, south, gd, np)) }
                        div class="text-sm text-gray-600" {
                            "Bid: " (display_bid(gs.bids()[p]))
                            " · Won: " (gs.tricks_won()[p])
                        }
                        (opponent_hand(gs.get_hand(p).len()))
                        (played_slot(gs.played_card(p)))
                        (last_trick_card(gs, gd, p))
                    }
                }
            }
            // Center: face-up card, turn arrow, clear button
            div class="grid justify-items-center gap-2 border-t border-b py-3" {
                (face_up_display(gs))
                @if !gs.is_terminal() && matches!(gd.display_state, GameProcessingState::WaitingHumanMove | GameProcessingState::WaitingMachineMoves) {
                    (turn_tracker(gs, south))
                }
                (clear_button_for(gd, south, game_id))
            }
            // Bottom: south seat (this viewer)
            div class="grid justify-items-center gap-2" {
                div class="text-sm font-semibold" { (seat_label(south, south, gd, np)) }
                div class="text-sm text-gray-600" {
                    "Bid: " (display_bid(gs.bids()[south]))
                    " · Won: " (gs.tricks_won()[south])
                }
                (played_slot(gs.played_card(south)))
                (last_trick_card(gs, gd, south))
                (render_hand_and_actions(gs, gd, south, game_id))
            }
        }
    }
}

fn face_up_display(gs: &OhHellGameState) -> Markup {
    match gs.face_up() {
        Some(c) => html! {
            div class="text-center" {
                div class="text-xs text-gray-500" { "Face up (trump)" }
                (card_icon(c))
            }
        },
        None => html! {},
    }
}

fn opponent_hand(n: usize) -> Markup {
    html! {
        div class="text-2xl lg:text-3xl text-center" {
            @for _ in 0..n { "🂠" }
        }
    }
}

fn played_slot(c: Option<OHCard>) -> Markup {
    match c {
        Some(c) => card_icon(c),
        None => html! { div class="text-5xl text-gray-300" { "·" } },
    }
}

fn last_trick_card(gs: &OhHellGameState, gd: &GameData, player: Player) -> Markup {
    // Only show when we're paused on a completed trick.
    if !matches!(
        gd.display_state,
        GameProcessingState::WaitingTrickClear { .. }
    ) {
        return html! {};
    }
    let Some((starter, trick)) = gs.last_trick() else {
        return html! {};
    };
    let np = gs.num_players();
    let pos = (player + np - starter) % np;
    if pos < trick.len() {
        card_icon(trick[pos])
    } else {
        html! {}
    }
}

fn turn_tracker(gs: &OhHellGameState, south: Player) -> Markup {
    let np = gs.num_players();
    let cur = gs.cur_player();
    let label = if cur == south {
        "Your turn".to_string()
    } else {
        format!("Waiting on {}", seat_label_plain(cur, south, np))
    };
    html! { div class="text-sm italic" { (label) } }
}

fn clear_button_for(gd: &GameData, south: Player, game_id: &Uuid) -> Markup {
    let south_pid = gd.players[south];
    let already_ready = |ready: &[usize]| -> bool {
        south_pid.map(|p| ready.contains(&p)).unwrap_or(false)
    };

    match &gd.display_state {
        GameProcessingState::WaitingBidClear { ready_players }
        | GameProcessingState::WaitingTrickClear { ready_players }
        | GameProcessingState::WaitingHandClear { ready_players }
            if already_ready(ready_players) =>
        {
            html! { div class="text-center" { "waiting on other players..." } }
        }
        GameProcessingState::WaitingTrickClear { .. } => {
            let winner_label = match gd.gs.last_trick() {
                Some((starter, trick)) => {
                    let np = gd.gs.num_players();
                    let trump = gd.gs.trump_suit().expect("trump set in play");
                    let lead = trick[0].suit();
                    let mut best_pos = 0;
                    let mut best = trick[0];
                    for (i, c) in trick.iter().enumerate().skip(1) {
                        if oh_beats(*c, best, lead, trump) {
                            best = *c;
                            best_pos = i;
                        }
                    }
                    let winner = (starter + best_pos) % np;
                    seat_label_plain(winner, south, np)
                }
                None => "?".to_string(),
            };
            html! {
                div { (winner_label) " wins the trick" }
                (clear_form("ready_trick", "Clear trick", game_id))
            }
        }
        GameProcessingState::WaitingBidClear { .. } => html! {
            div { "Bidding complete. Time to play!" }
            (clear_form("ready_bid", "Begin play", game_id))
        },
        GameProcessingState::WaitingHandClear { .. } => {
            let np = gd.gs.num_players();
            html! {
                div class="font-bold" { "Hand over" }
                div class="grid grid-cols-3 gap-x-2" {
                    div class="font-semibold" { "Seat" }
                    div class="font-semibold" { "Bid" }
                    div class="font-semibold" { "Won" }
                    @for p in 0..np {
                        div { (seat_label_plain(p, south, np)) }
                        div { (display_bid(gd.gs.bids()[p])) }
                        div { (gd.gs.tricks_won()[p]) }
                    }
                }
                (clear_form("ready_hand", "Next hand", game_id))
            }
        }
        _ => html! {},
    }
}

fn render_hand_and_actions(
    gs: &OhHellGameState,
    gd: &GameData,
    south: Player,
    game_id: &Uuid,
) -> Markup {
    if gs.is_chance_node() {
        return html! {};
    }
    let hand = gs.get_hand(south);
    let is_our_turn = gs.cur_player() == south
        && matches!(gd.display_state, GameProcessingState::WaitingHumanMove);

    if !is_our_turn {
        return html! {
            div class="grid gap-y-2 justify-items-center" {
                div class="flex flex-wrap gap-x-2" {
                    @for c in &hand { (card_icon(*c)) }
                }
            }
        };
    }

    match gs.phase() {
        OHPhase::Bidding => render_bid_choices(gs, &hand, game_id),
        OHPhase::Play => render_card_choices(gs, &hand, game_id),
        _ => html! {},
    }
}

fn render_bid_choices(gs: &OhHellGameState, hand: &[OHCard], game_id: &Uuid) -> Markup {
    // Only legal bid values are rendered as buttons — this is what
    // surfaces the dealer's hook constraint to a human player without
    // having to re-explain it in prose.
    let legal_bids: Vec<u8> = actions!(gs)
        .into_iter()
        .filter_map(|a| match OHAction::from(a) {
            OHAction::Bid(n) => Some(n),
            _ => None,
        })
        .collect();
    html! {
        div class="grid gap-y-2 justify-items-center" {
            div class="flex flex-wrap gap-x-2" {
                @for c in hand { (card_icon(*c)) }
            }
            div class="text-sm" { "Bid the exact number of tricks you'll take this hand:" }
            div class="flex flex-wrap gap-x-2 gap-y-2 justify-center" {
                @for n in &legal_bids {
                    @let raw = (BID_BASE + *n) as u32;
                    (action_form_button(
                        raw,
                        &n.to_string(),
                        "text-xl bg-white outline outline-black hover:bg-slate-100 rounded-lg px-4 py-2 min-w-[3rem]",
                        game_id,
                    ))
                }
            }
            div class="text-xs text-gray-500" {
                "Score per hand: 1 point per trick, +10 bonus for exact bid. "
                "Face up trump: "
                @if let Some(c) = gs.face_up() { (card_text(c)) }
            }
        }
    }
}

fn render_card_choices(gs: &OhHellGameState, hand: &[OHCard], game_id: &Uuid) -> Markup {
    let legal: Vec<OHCard> = actions!(gs)
        .into_iter()
        .filter_map(|a| match OHAction::from(a) {
            OHAction::Card(c) => Some(c),
            _ => None,
        })
        .collect();
    html! {
        div class="flex flex-wrap gap-x-2 justify-center" {
            @for c in hand {
                @let is_legal = legal.contains(c);
                (play_card_button(*c, is_legal, game_id))
            }
        }
    }
}

fn play_card_button(card: OHCard, is_legal: bool, game_id: &Uuid) -> Markup {
    let color = card_color(card.suit());
    let classes = format!(
        "text-6xl py-2 px-2 rounded-lg bg-white outline outline-black hover:bg-slate-100 disabled:outline-gray-300 disabled:text-gray-400 {color}"
    );
    if is_legal {
        let raw = card as u32;
        html! {
            form
                hx-post={ "/game/" (game_id) "/action" }
                hx-target="#game"
                hx-swap="innerHTML"
                class="inline"
            {
                input type="hidden" name="kind" value="take";
                input type="hidden" name="action" value=(raw);
                button type="submit" class=(classes) { (card_glyph(card)) }
            }
        }
    } else {
        html! { button disabled="true" class=(classes) { (card_glyph(card)) } }
    }
}

// ---------- Card / suit display helpers ----------

fn card_color(suit: OHSuit) -> &'static str {
    match suit {
        OHSuit::Clubs | OHSuit::Spades => "text-black",
        OHSuit::Hearts | OHSuit::Diamonds => "text-red-500",
    }
}

fn suit_icon(suit: OHSuit) -> &'static str {
    match suit {
        OHSuit::Clubs => "♣",
        OHSuit::Spades => "♠",
        OHSuit::Hearts => "♥",
        OHSuit::Diamonds => "♦",
    }
}

fn rank_glyph(r: u8) -> &'static str {
    match r {
        2 => "2",
        3 => "3",
        4 => "4",
        5 => "5",
        6 => "6",
        7 => "7",
        8 => "8",
        9 => "9",
        10 => "10",
        11 => "J",
        12 => "Q",
        13 => "K",
        14 => "A",
        _ => "?",
    }
}

/// Plain text form of a card, e.g. "9♣".
fn card_text(c: OHCard) -> String {
    format!("{}{}", rank_glyph(c.rank()), suit_icon(c.suit()))
}

/// Display glyph for a card in a button: rank stacked over suit so the
/// full 52-card deck renders consistently. Unicode playing card
/// codepoints exist but cover only a subset of ranks we care about.
fn card_glyph(c: OHCard) -> Markup {
    html! {
        span class="flex flex-col items-center leading-none" {
            span class="text-xl font-bold" { (rank_glyph(c.rank())) }
            span class="text-3xl" { (suit_icon(c.suit())) }
        }
    }
}

fn card_icon(c: OHCard) -> Markup {
    let color = card_color(c.suit());
    html! {
        span class={ "inline-block px-2 py-1 rounded border border-gray-300 " (color) } {
            (card_glyph(c))
        }
    }
}

fn display_bid(b: Option<u8>) -> String {
    match b {
        Some(n) => n.to_string(),
        None => "—".to_string(),
    }
}

/// Markup label for a seat, including a "(you)" suffix on the viewer's
/// seat and a "(dealer)" suffix when applicable. Dealer in this Oh Hell
/// is the last seat to bid — bidding starts at seat 0, so dealer is
/// seat num_players - 1. (The implementation doesn't have an explicit
/// dealer concept; treat the last seat in bid order as dealer for UI
/// clarity.)
fn seat_label(player: Player, south: Player, gd: &GameData, np: usize) -> Markup {
    let mut s = seat_label_plain(player, south, np);
    if player == south {
        s.push_str(" (you)");
    }
    if gd.players[player].is_none() {
        s.push_str(" [bot]");
    }
    html! { (s) }
}

/// Plain seat label that depends only on viewer geometry, no game-data
/// context. Used inside captions that just need a position name.
fn seat_label_plain(player: Player, south: Player, np: usize) -> String {
    if player == south {
        return "South".to_string();
    }
    let offset = (player + np - south) % np;
    match (offset, np) {
        (_, 2) => "North".to_string(),
        (1, 3) => "West".to_string(),
        (2, 3) => "East".to_string(),
        (1, 4) => "West".to_string(),
        (2, 4) => "North".to_string(),
        (3, 4) => "East".to_string(),
        _ => format!("Seat {}", player),
    }
}

/// Local copy of the trick-winner rule so the UI can label trick
/// winners without poking inside the game module. Matches `beats` in
/// `games::gamestates::oh_hell`.
fn oh_beats(candidate: OHCard, current_best: OHCard, lead: OHSuit, trump: OHSuit) -> bool {
    let c_trump = candidate.suit() == trump;
    let b_trump = current_best.suit() == trump;
    match (c_trump, b_trump) {
        (true, false) => true,
        (false, true) => false,
        (true, true) => candidate.rank() > current_best.rank(),
        (false, false) => {
            let c_lead = candidate.suit() == lead;
            let b_lead = current_best.suit() == lead;
            match (c_lead, b_lead) {
                (true, false) => true,
                (false, true) => false,
                (false, false) => false,
                (true, true) => candidate.rank() > current_best.rank(),
            }
        }
    }
}
