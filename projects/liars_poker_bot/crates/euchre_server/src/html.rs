//! Server-rendered HTML frontend for the euchre server, using Maud
//! templates and htmx for polling and form submission.
//!
//! All routes live at the root path. There is no JavaScript framework
//! in the browser — htmx handles the polling coroutine and state swaps
//! that were previously implemented as Dioxus `use_coroutine` handles.

use std::str::FromStr;

use actix_web::{web, HttpRequest, HttpResponse, Responder};
use games::{
    actions,
    gamestates::euchre::{
        actions::{Card, EAction, Suit},
        EuchreGameState,
    },
    GameState, Player,
};
use maud::{html, Markup};
use rand::{rng, RngExt};
use serde::Deserialize;
use uuid::Uuid;
use web_common::{
    action_form_button, clear_form, get_or_set_player_id as web_get_or_set_player_id,
    html_response, layout, render_waiting_players,
};

use crate::{
    handle_ready_clear, handle_register_player, handle_take_action, new_game, progress_game,
    AppState, GameData, GameProcessingState,
};

const PLAYER_COOKIE: &str = "euchre_player_id";

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

// ---------- Index / landing page ----------

async fn index(req: HttpRequest) -> impl Responder {
    let (_pid, cookie) = get_or_set_player_id(&req);
    let body = html! {
        div class="max-w-2xl mx-auto grid gap-4 mb-8" {
            h1 class="text-2xl font-bold" { "Play euchre against ai bots" }
            p {
                "Euchre is a card game where you and a partner try to take more tricks "
                "than the opponent team. The game is two phases. In the first, trump is "
                "decided. In the second, cards are played to take tricks."
            }
            p {
                "For an overview of the rules, see Wikipedia: "
                a
                    class="text-blue-600 visited:text-purple-600 underline"
                    href="https://en.wikipedia.org/wiki/Euchre"
                    target="_blank"
                    rel="noopener"
                { "Euchre" }
            }
            p {
                span class="font-bold" { "Optionally play with a friend. " }
                "You can play with a friend against the ai bots by sharing the url after "
                "you create a game. If you play alone, you'll get an ai agent as a teammate."
            }
            p {
                span class="font-bold" {
                    "Bots use counter factual regret minimization (CFR) and perfect "
                    "information monte carlo tree search (PIMCT). "
                }
                "Using counter factual regret minimization (CFR) alone would result in a "
                "stronger bot. But CFR cannot be naively applied to euchre — the game is too large."
            }
            p {
                "Instead, I use CFR for the first phase where trump is chosen and PIMCTS "
                "for the second phase where cards are played."
            }
            p {
                "More detail on the approach can be found on my blog: "
                a
                    class="text-blue-600 visited:text-purple-600 underline"
                    href="https://fewworddotrick.com/project-log/2023/07/30/cfr-for-euchre.html"
                    target="_blank"
                    rel="noopener"
                { "CFR for euchre" }
            }
        }
        div class="grid justify-items-center gap-2" {
            form method="post" action="/new" class="inline" {
                input type="hidden" name="min_players" value="1";
                button
                    type="submit"
                    class="bg-white outline outline-black hover:bg-slate-100 rounded-lg px-4 py-2 font-medium"
                { "Play with ai partner" }
            }
            form method="post" action="/new" class="inline" {
                input type="hidden" name="min_players" value="2";
                button
                    type="submit"
                    class="bg-white outline outline-black hover:bg-slate-100 rounded-lg px-4 py-2 font-medium"
                { "Play with human partner" }
            }
        }
    };
    html_response(layout("Euchre", body), cookie)
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

    log::info!("new game created: {game_id}");

    let url = format!("/game/{}", game_id);
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
        // Auto-register this visitor as the second human if there's a
        // free slot — matches the old Dioxus frontend's behavior.
        let mut games = data.games.lock().unwrap();
        let Some(gd) = games.get_mut(&game_id) else {
            return HttpResponse::NotFound().body("game not found");
        };
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
        // Polling container. Every 2s it re-fetches the view fragment
        // and replaces its innerHTML. One HTML attribute replaces the
        // Dioxus polling coroutine.
        div
            id="game"
            hx-get={ "/game/" (game_id) "/view" }
            hx-trigger="every 2s"
            hx-swap="innerHTML"
        {
            (view)
        }
    };
    html_response(layout("Euchre", body), cookie)
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
    /// For `kind=take`: the raw EAction discriminant as a u32.
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

pub(crate) fn render_game_view(gd: &GameData, player_id: usize, game_id: &Uuid) -> Markup {
    use GameProcessingState::*;
    match &gd.display_state {
        WaitingPlayerJoin { .. } => render_waiting_players(game_id),
        GameOver => render_game_over(gd, game_id),
        _ => render_active_game(gd, player_id, game_id),
    }
}

fn render_game_over(gd: &GameData, game_id: &Uuid) -> Markup {
    html! {
        div class="px-8 pt-8 grid gap-4" {
            div class="font-bold text-xl" { "Thanks for playing!" }
            div {
                "Final score — Humans: " (gd.human_score)
                " · Machines: " (gd.computer_score)
            }
            div {
                "If you completed this game as part of an event, please register your "
                "game to be eligible for any applicable prizes: "
                a
                    class="text-blue-600 visited:text-purple-600 underline"
                    target="_blank"
                    rel="noopener"
                    href={
                        "https://docs.google.com/forms/d/e/1FAIpQLSfoLDgRBwXoIHhI-MondqYO4agtvIhom1vHnacgv5brhSKJAA/viewform?usp=pp_url&entry.90030775="
                        (game_id)
                    }
                { "game registration" }
            }
            a
                href="/"
                class="bg-white outline outline-black hover:bg-slate-100 rounded-lg px-4 py-2 mt-4 font-medium w-fit"
            { "Return home to start a new game" }
        }
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
    let gs = gd.to_state();
    let south_player = gd
        .players
        .iter()
        .position(|x| *x == Some(player_id))
        .unwrap_or(0);

    html! {
        div class="grid sm:flex sm:flex-row gap-4" {
            div class="sm:basis-3/4" {
                (render_play_area(&gs, gd, south_player, game_id))
            }
            div class="sm:basis-1/4 grid gap-4" {
                (render_game_info(&gs, south_player))
                (render_running_stats(gd))
                (render_player_stats(&gd.players))
            }
        }
    }
}

fn render_game_info(gs: &EuchreGameState, south: Player) -> Markup {
    // Find which seat holds the dealer (player 3 in internal indexing).
    let dealer_seat = seat_name(3, south);
    let trump_line = match gs.trump() {
        Some((suit, caller)) => format!(
            "Trump is {}. Called by {}",
            suit.icon(),
            seat_name(caller, south)
        ),
        None => "Trump has not been called".to_string(),
    };
    let face_up_line = match gs.face_up() {
        Some(c) => format!("Face up card is: {}", c.icon()),
        None => "Face up card not yet dealt".to_string(),
    };
    // Going-alone status: only the trump caller's seat plays; their partner
    // sits out. Without this line, the seat that "doesn't play" looks like a
    // bug to the user.
    let alone_line = if gs.going_alone() {
        gs.trump().map(|(_, caller)| {
            format!("{} is going alone", seat_name(caller, south))
        })
    } else {
        None
    };
    let south_tricks = gs.trick_score()[south % 2];
    let opp_tricks = gs.trick_score()[(south + 1) % 2];

    html! {
        div {
            div class="pt-2 font-bold text-xl" { "Game information" }
            div { "Dealer is " (dealer_seat) }
            div { (face_up_line) }
            div { (trump_line) }
            @if let Some(line) = alone_line {
                div class="font-bold text-amber-700" { (line) }
            }
            div class="font-bold pt-2" { "Tricks taken:" }
            div class="grid grid-cols-2" {
                div { "North/South" }
                div { "East/West" }
                div { (south_tricks) }
                div { (opp_tricks) }
            }
        }
    }
}

fn render_running_stats(gd: &GameData) -> Markup {
    html! {
        div {
            div class="font-bold text-xl" { "Running stats" }
            div class="grid grid-cols-2" {
                div { "Humans" }
                div { "Machines" }
                div { (gd.human_score) }
                div { (gd.computer_score) }
            }
        }
    }
}

fn render_player_stats(players: &[Option<usize>]) -> Markup {
    let multi_human = players.iter().filter(|x| x.is_some()).count() > 1;
    html! {
        div {
            div class="font-bold text-xl" { "Player details" }
            @if multi_human {
                div { "North: Human" }
                div { "South: Human" }
                div { "East: Computer" }
                div { "West: Computer" }
            } @else {
                div { "North: Computer" }
                div { "South: Human" }
                div { "East: Computer" }
                div { "West: Computer" }
            }
        }
    }
}

fn render_play_area(
    gs: &EuchreGameState,
    gd: &GameData,
    south: Player,
    game_id: &Uuid,
) -> Markup {
    let west = (south + 1) % 4;
    let north = (south + 2) % 4;
    let east = (south + 3) % 4;

    let label = |player: Player, base: &'static str| -> String {
        if player == 3 {
            format!("{} (Dealer)", base)
        } else {
            base.to_string()
        }
    };

    let show_bids = matches!(gd.display_state, GameProcessingState::WaitingBidClear { .. })
        || matches!(
            gs.phase(),
            games::gamestates::euchre::EPhase::Pickup
                | games::gamestates::euchre::EPhase::ChooseTrump
        );

    html! {
        div class="grid grid-cols-5 content-between gap-2" {
            // North area
            div class="col-start-2 col-span-3 grid" {
                div class="justify-self-center" { (label(north, "North")) }
                (opponent_hand(gs.get_hand(north).len()))
            }

            // Middle row: west, center play area, east
            div class="row-start-2" {
                div class="text-center" { (label(west, "West")) }
                (opponent_hand(gs.get_hand(west).len()))
            }

            div class="col-span-3 grid grid-cols-3 items-center justify-items-center space-y-4" {
                // North played card / last trick / bid
                div class="col-start-2" {
                    (played_slot(gs.played_card(north)))
                    (last_trick_card(gs, gd, north))
                    @if show_bids { (bid_for(gs, north)) }
                }
                // West played card / last trick / bid
                div class="row-start-2" {
                    (played_slot(gs.played_card(west)))
                    (last_trick_card(gs, gd, west))
                    @if show_bids { (bid_for(gs, west)) }
                }
                // Center: face-up card, turn arrow, clear button
                div class="row-start-2 col-start-2 grid justify-items-center" {
                    (face_up_slot(gs.displayed_face_up_card()))
                    @if matches!(gd.display_state, GameProcessingState::WaitingBidClear { .. }) {
                        @if let Some(c) = gs.face_up() { (card_icon(c)) }
                    }
                    @if !gs.is_terminal() && !gs.is_trick_over() {
                        (turn_tracker(gs, south))
                    }
                    (clear_button_for(gd, gs, south, game_id))
                }
                // East played card / last trick / bid
                div class="row-start-2 col-start-3" {
                    (played_slot(gs.played_card(east)))
                    (last_trick_card(gs, gd, east))
                    @if show_bids { (bid_for(gs, east)) }
                }
                // South played card / last trick / bid
                div class="row-start-3 col-start-2" {
                    (played_slot(gs.played_card(south)))
                    (last_trick_card(gs, gd, south))
                    @if show_bids { (bid_for(gs, south)) }
                }
            }
            div class="" {
                div class="text-center" { (label(east, "East")) }
                (opponent_hand(gs.get_hand(east).len()))
            }

            // Bottom area: south label + player's hand / actions
            div class="row-start-3 col-span-5 grid justify-items-center" {
                div class="self-end" { (label(south, "South")) }
                (render_hand_and_actions(gs, gd, south, game_id))
            }
        }
    }
}

fn opponent_hand(n: usize) -> Markup {
    html! {
        div class="text-3xl lg:text-6xl text-center" {
            @for _ in 0..n { "🂠" }
        }
    }
}

fn played_slot(c: Option<Card>) -> Markup {
    match c {
        Some(c) => card_icon(c),
        None => html! { div class="text-6xl" {} },
    }
}

fn face_up_slot(c: Option<Card>) -> Markup {
    match c {
        Some(c) => card_icon(c),
        None => html! {},
    }
}

fn card_icon(c: Card) -> Markup {
    let color = card_color(c.suit());
    html! {
        span class={ "text-7xl " (color) } { (c.icon()) }
    }
}

fn turn_tracker(gs: &EuchreGameState, south: Player) -> Markup {
    let arrow = match gs.cur_player() {
        x if x == (south + 1) % 4 => "←",
        x if x == (south + 2) % 4 => "↑",
        x if x == (south + 3) % 4 => "→",
        _ => "↓",
    };
    html! { div class="text-4xl lg:text-6xl" { (arrow) } }
}

fn last_trick_card(gs: &EuchreGameState, gd: &GameData, player: Player) -> Markup {
    if !matches!(gd.display_state, GameProcessingState::WaitingTrickClear { .. }) {
        return html! {};
    }
    let Some((starter, mut trick)) = gs.last_trick() else {
        return html! {};
    };
    trick.rotate_left((4 - starter) % 4);
    match trick[player] {
        Some(card) => card_icon(card),
        None => html! {},
    }
}

fn bid_for(gs: &EuchreGameState, player: Player) -> Markup {
    use EAction::*;
    let bids = gs.bids();
    let label = |a: Option<EAction>| -> Option<&'static str> {
        a.and_then(|a| match a {
            Pass => Some("Pass"),
            Pickup => Some("Pickup"),
            Clubs => Some("Clubs"),
            Spades => Some("Spades"),
            Hearts => Some("Hearts"),
            Diamonds => Some("Diamonds"),
            _ => None,
        })
    };
    let first = label(bids[player]);
    let second = label(bids[player + 4]);
    html! {
        @if let Some(f) = first { div { (f) } }
        @if let Some(s) = second { div { (s) } }
    }
}

fn clear_button_for(
    gd: &GameData,
    gs: &EuchreGameState,
    south: Player,
    game_id: &Uuid,
) -> Markup {
    // Pull out the player id for the south seat so we can tell if this
    // player has already readied.
    let south_player_id = gd.players[south];
    let already_ready = |ready: &[usize]| -> bool {
        south_player_id.map(|p| ready.contains(&p)).unwrap_or(false)
    };

    match &gd.display_state {
        GameProcessingState::WaitingTrickClear { ready_players }
        | GameProcessingState::WaitingBidClear { ready_players }
            if already_ready(ready_players) =>
        {
            html! { div class="text-center" { "waiting on other players..." } }
        }
        GameProcessingState::WaitingTrickClear { .. } if gs.is_terminal() => {
            let south_wins = gs.trick_score()[south % 2];
            let east_wins = gs.trick_score()[(south + 1) % 2];
            html! {
                div { "Hand over" }
                div { "North/South tricks: " (south_wins) }
                div { "East/West tricks: " (east_wins) }
                (clear_form("ready_trick", "Next hand", game_id))
            }
        }
        GameProcessingState::WaitingTrickClear { .. } => {
            let winner_seat = seat_name(gs.cur_player(), south);
            html! {
                div { (winner_seat) " wins" }
                (clear_form("ready_trick", "Clear trick", game_id))
            }
        }
        GameProcessingState::WaitingBidClear { .. } => {
            clear_form("ready_bid", "Continue game", game_id)
        }
        _ => html! {},
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

    let legal: Vec<EAction> = actions!(gs).into_iter().map(EAction::from).collect();
    let is_our_turn = gs.cur_player() == south
        && matches!(gd.display_state, GameProcessingState::WaitingHumanMove);
    let hand = gs.get_hand(south);

    // If it's not our turn (or we're in a clear state), just render hand
    // as non-interactive card icons.
    if !is_our_turn {
        return html! {
            div class="grid gap-y-4 justify-items-center" {
                div class="flex gap-x-4" {
                    @for c in &hand { (card_icon(*c)) }
                }
            }
        };
    }

    // Pickup / pass phase
    if legal.contains(&EAction::Pickup) {
        let pickup_text = if south == 3 {
            "Take card"
        } else {
            "Tell dealer to take card"
        };
        return html! {
            div class="grid gap-y-4 justify-items-center" {
                div class="flex gap-x-4" {
                    @for c in &hand { (card_icon(*c)) }
                }
                div class="flex gap-x-4" {
                    (action_form_button(
                        EAction::Pickup as u32,
                        pickup_text,
                        "basis-1/2 text-xl bg-white outline outline-black hover:bg-slate-100 rounded-lg px-3 py-2",
                        game_id,
                    ))
                    (action_form_button(
                        EAction::Pass as u32,
                        "Pass",
                        "basis-1/2 text-xl bg-white outline outline-black hover:bg-slate-100 rounded-lg px-3 py-2",
                        game_id,
                    ))
                }
            }
        };
    }

    // Go-alone phase — the maker (or dealer told to pick up) chooses
    // whether to play without their partner. Legal actions are Alone and
    // Pass. Must be handled before the "Regular play" fallthrough; that
    // branch assumes every legal action is a card and calls .card() on
    // each, which panics for non-card variants like Alone/Pass and
    // poisons the shared mutex.
    if legal.contains(&EAction::Alone) {
        return html! {
            div class="grid gap-y-4 justify-items-center" {
                div class="flex gap-x-4" {
                    @for c in &hand { (card_icon(*c)) }
                }
                div class="flex gap-x-4" {
                    (action_form_button(
                        EAction::Alone as u32,
                        "Go alone",
                        "text-xl bg-white outline outline-black hover:bg-slate-100 rounded-lg px-3 py-2",
                        game_id,
                    ))
                    (action_form_button(
                        EAction::Pass as u32,
                        "With partner",
                        "text-xl bg-white outline outline-black hover:bg-slate-100 rounded-lg px-3 py-2",
                        game_id,
                    ))
                }
            }
        };
    }

    // Suit-choice phase
    if legal.contains(&EAction::Clubs) || legal.contains(&EAction::Spades) {
        return html! {
            div class="grid gap-y-4" {
                div class="flex gap-x-4" {
                    @for c in &hand { (card_icon(*c)) }
                }
                div class="flex gap-x-4" {
                    (action_form_button(
                        EAction::Spades as u32,
                        Suit::Spades.icon(),
                        "text-xl text-black bg-white outline outline-black hover:bg-slate-100 rounded-lg px-3 py-2",
                        game_id,
                    ))
                    (action_form_button(
                        EAction::Clubs as u32,
                        Suit::Clubs.icon(),
                        "text-xl text-black bg-white outline outline-black hover:bg-slate-100 rounded-lg px-3 py-2",
                        game_id,
                    ))
                    (action_form_button(
                        EAction::Hearts as u32,
                        Suit::Hearts.icon(),
                        "text-xl text-red-500 bg-white outline outline-black hover:bg-slate-100 rounded-lg px-3 py-2",
                        game_id,
                    ))
                    (action_form_button(
                        EAction::Diamonds as u32,
                        Suit::Diamonds.icon(),
                        "text-xl text-red-500 bg-white outline outline-black hover:bg-slate-100 rounded-lg px-3 py-2",
                        game_id,
                    ))
                    (action_form_button(
                        EAction::Pass as u32,
                        "Pass",
                        "text-xl bg-white outline outline-black hover:bg-slate-100 rounded-lg px-3 py-2",
                        game_id,
                    ))
                }
            }
        };
    }

    // Regular play phase — each card either playable or disabled.
    html! {
        div class="flex flex-wrap gap-x-4" {
            @for c in &hand {
                @let legal_action = legal.iter().find(|a| a.card() == *c).copied();
                (play_card_button(*c, legal_action, game_id))
            }
        }
    }
}

fn play_card_button(card: Card, action: Option<EAction>, game_id: &Uuid) -> Markup {
    let color = card_color(card.suit());
    let classes = format!(
        "text-7xl py-2 px-2 rounded-lg bg-white outline outline-black hover:bg-slate-100 disabled:outline-white {color}"
    );
    match action {
        Some(a) => html! {
            form
                hx-post={ "/game/" (game_id) "/action" }
                hx-target="#game"
                hx-swap="innerHTML"
                class="inline"
            {
                input type="hidden" name="kind" value="take";
                input type="hidden" name="action" value=(a as u32);
                button type="submit" class=(classes) { (card.icon()) }
            }
        },
        None => html! {
            button disabled="true" class=(classes) { (card.icon()) }
        },
    }
}

