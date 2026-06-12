//! Server-rendered HTML frontend for the Catan server. Built with Maud +
//! htmx (no client-side framework), mirroring oh_hell_server/html.rs.
//!
//! The board is an inline SVG. When it's the viewer's turn, legal board
//! placements (settlements, cities, roads, robber hexes) render as
//! clickable translucent overlays that post the raw action id — htmx
//! works on SVG nodes via `hx-post` + `hx-vals`. Everything else (end
//! turn, trades, dev cards, discards) is plain buttons under the board.

use std::str::FromStr;

use actix_web::{web, HttpRequest, HttpResponse, Responder};
use games::{
    actions,
    gamestates::catan::{
        actions::CatanAction,
        board::{
            geometry, Port, Resource, Terrain, NUM_EDGES, NUM_HEXES, NUM_VERTICES, RESOURCES,
        },
        Catan, CatanGameState, CatanPhase, MAX_ROLLS,
    },
    Action, GameState, Player,
};
use maud::{html, Markup, PreEscaped};
use serde::Deserialize;
use uuid::Uuid;
use web_common::{
    action_form_button, get_or_set_player_id as web_get_or_set_player_id, html_response, layout,
    render_waiting_players,
};

use crate::{
    handle_register_player, handle_take_action, progress_game, AppState, GameData,
    GameProcessingState, DEFAULT_PLAYERS, MAX_HUMANS, SUPPORTED_PLAYERS,
};

const PLAYER_COOKIE: &str = "catan_player_id";

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
            h1 class="text-2xl font-bold" { "Play Settlers of Catan against ai bots" }
            p {
                "Settle the island: roll for resources, build roads, settlements "
                "and cities, and be the first to 10 victory points. For the full "
                "rules, see "
                a
                    class="text-blue-600 visited:text-purple-600 underline"
                    href="https://en.wikipedia.org/wiki/Catan"
                    target="_blank"
                    rel="noopener"
                { "Catan" }
                "."
            }
            p {
                "This server plays the base game on the fixed beginner board for "
                span class="font-bold" { "2-4 players" }
                ", with a few simplifications: trades go through the bank and "
                "ports only (no player-to-player trading), development cards are "
                "playable only after your dice roll, and games are capped at "
                (MAX_ROLLS) " dice rolls (highest score wins at the cap)."
            }
            p {
                span class="font-bold" { "Optionally play with friends. " }
                "Share the game url after creating a game to seat up to "
                (MAX_HUMANS - 1) " human friends. The remaining seats are filled "
                "by ai bots (PIMCTS with random rollouts)."
            }
        }
        div class="grid justify-items-center gap-2" {
            form method="post" action="/new" class="grid gap-2 justify-items-center" {
                div class="flex gap-4 items-center" {
                    label class="text-sm font-medium" for="num_players" { "Players at the table" }
                    select
                        name="num_players"
                        id="num_players"
                        class="bg-white outline outline-black rounded-lg px-2 py-1"
                    {
                        @for np in SUPPORTED_PLAYERS {
                            option value=(np) selected[np == DEFAULT_PLAYERS] { (np) }
                        }
                    }
                }
                div class="flex gap-4 items-center" {
                    label class="text-sm font-medium" for="num_humans" { "Human players" }
                    select
                        name="num_humans"
                        id="num_humans"
                        class="bg-white outline outline-black rounded-lg px-2 py-1"
                    {
                        @for h in 1..=MAX_HUMANS {
                            option value=(h) { (h) }
                        }
                    }
                }
                button
                    type="submit"
                    class="bg-white outline outline-black hover:bg-slate-100 rounded-lg px-4 py-2 font-medium"
                { "Create game" }
            }
        }
    };
    html_response(layout("Catan", body), cookie)
}

// ---------- New game ----------

#[derive(Deserialize)]
struct NewGameForm {
    num_humans: usize,
    #[serde(default)]
    num_players: Option<usize>,
}

async fn new_game_handler(
    req: HttpRequest,
    form: web::Form<NewGameForm>,
    data: web::Data<AppState>,
) -> impl Responder {
    let (player_id, cookie) = get_or_set_player_id(&req);
    let num_players = form
        .num_players
        .filter(|np| SUPPORTED_PLAYERS.contains(np))
        .unwrap_or(DEFAULT_PLAYERS);
    let num_humans = form.num_humans.clamp(1, MAX_HUMANS.min(num_players));
    let game_id = Uuid::new_v4();

    let mut gd = GameData::new(
        Catan::new_state(num_players),
        player_id,
        num_humans,
        num_players,
    );
    progress_game(&mut gd, &data.bot, &game_id);
    data.games.lock().unwrap().insert(game_id, gd);

    log::info!(
        "new catan game created: {game_id} (players: {num_players}, humans: {num_humans})"
    );

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
        // Auto-register a visitor who arrived via shared link if the game
        // still has room for another human.
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
    html_response(layout("Catan", body), cookie)
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
    /// For `kind=take`: the raw action discriminant.
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
        GameOver => render_game_over(gd, player_id),
        _ => render_active_game(gd, player_id, game_id),
    }
}

fn render_game_over(gd: &GameData, player_id: usize) -> Markup {
    let gs = &gd.gs;
    let np = gs.num_players();
    let south = viewer_seat(gd, player_id);
    let winner = (0..np)
        .max_by_key(|&p| gs.victory_points(p))
        .unwrap_or(0);
    html! {
        div class="px-8 pt-8 grid gap-4" {
            div class="font-bold text-xl" { "Thanks for playing!" }
            div { (seat_label(winner, south, gd)) " wins with " (gs.victory_points(winner)) " victory points." }
            div class="grid grid-cols-2 gap-x-4 w-fit" {
                div class="font-semibold" { "Seat" }
                div class="font-semibold" { "Victory points" }
                @for p in 0..np {
                    div { (seat_label(p, south, gd)) }
                    div { (gs.victory_points(p)) }
                }
            }
            a
                href="/"
                class="bg-white outline outline-black hover:bg-slate-100 rounded-lg px-4 py-2 mt-4 font-medium w-fit"
            { "Return home to start a new game" }
        }
    }
}

/// Seat index the viewer occupies; seat 0 for spectators.
fn viewer_seat(gd: &GameData, player_id: usize) -> Player {
    gd.players
        .iter()
        .position(|x| *x == Some(player_id))
        .unwrap_or(0)
}

fn render_active_game(gd: &GameData, player_id: usize, game_id: &Uuid) -> Markup {
    let gs = &gd.gs;
    let south = viewer_seat(gd, player_id);
    let seated = gd.players[south] == Some(player_id);
    // The viewer may interact only when it is their seat's move.
    let interactive = seated
        && matches!(gd.display_state, GameProcessingState::WaitingHumanMove)
        && gs.cur_player() == south;

    // Partition legal actions: board placements render as SVG overlays,
    // the rest as buttons in the action panel.
    let mut board_actions = Vec::new();
    let mut panel_actions = Vec::new();
    if interactive {
        for a in actions!(gs) {
            match CatanAction::from_action(a) {
                CatanAction::BuildSettlement(_)
                | CatanAction::BuildCity(_)
                | CatanAction::BuildRoad(_)
                | CatanAction::MoveRobber(_) => board_actions.push(a),
                _ => panel_actions.push(a),
            }
        }
    }

    html! {
        div class="grid lg:flex lg:flex-row gap-4" {
            div class="lg:basis-2/3 grid gap-2" {
                (render_board_svg(gs, &board_actions, game_id))
                (render_action_panel(gs, gd, south, interactive, &panel_actions, game_id))
            }
            div class="lg:basis-1/3 grid gap-4 content-start" {
                (render_turn_info(gs, gd, south))
                (render_player_table(gs, gd, south))
                @if seated { (render_own_panel(gs, south)) }
                (render_bank_panel(gs))
                (render_legend())
            }
        }
    }
}

// ---------- Board geometry → pixels ----------

const HEX_SIZE: f64 = 52.0;

fn hex_center(q: i8, r: i8) -> (f64, f64) {
    (
        HEX_SIZE * 3f64.sqrt() * (q as f64 + r as f64 / 2.0),
        HEX_SIZE * 1.5 * r as f64,
    )
}

/// Pixel offset of a hex corner, in the N, NE, SE, S, SW, NW order used
/// by `BoardGeometry::hex_vertices`.
fn corner_offset(k: usize) -> (f64, f64) {
    let angle: f64 = [-90.0f64, -30.0, 30.0, 90.0, 150.0, 210.0][k].to_radians();
    (HEX_SIZE * angle.cos(), HEX_SIZE * angle.sin())
}

fn vertex_positions() -> [(f64, f64); NUM_VERTICES] {
    let geo = geometry();
    let mut pos = [(0.0, 0.0); NUM_VERTICES];
    for h in 0..NUM_HEXES {
        let (q, r) = geo.hex_coords[h];
        let c = hex_center(q, r);
        for k in 0..6 {
            let o = corner_offset(k);
            pos[geo.hex_vertices[h][k] as usize] = (c.0 + o.0, c.1 + o.1);
        }
    }
    pos
}

fn fmt2(x: f64) -> String {
    format!("{x:.1}")
}

// ---------- Board rendering ----------

fn terrain_fill(t: Terrain) -> &'static str {
    match t {
        Terrain::Producing(Resource::Brick) => "#c1693c",
        Terrain::Producing(Resource::Lumber) => "#2e7d3a",
        Terrain::Producing(Resource::Ore) => "#8d99ae",
        Terrain::Producing(Resource::Grain) => "#e8c34e",
        Terrain::Producing(Resource::Wool) => "#a6d75b",
        Terrain::Desert => "#e3d6a7",
    }
}

fn seat_fill(p: Player) -> &'static str {
    ["#dc2626", "#2563eb", "#15803d", "#ea580c"][p]
}

pub(crate) fn seat_name(p: Player) -> &'static str {
    ["Red", "Blue", "Green", "Orange"][p]
}

fn res_emoji(r: Resource) -> &'static str {
    match r {
        Resource::Brick => "🧱",
        Resource::Lumber => "🌲",
        Resource::Ore => "⛰️",
        Resource::Grain => "🌾",
        Resource::Wool => "🐑",
    }
}

fn res_name(r: Resource) -> &'static str {
    match r {
        Resource::Brick => "Brick",
        Resource::Lumber => "Lumber",
        Resource::Ore => "Ore",
        Resource::Grain => "Grain",
        Resource::Wool => "Wool",
    }
}

/// Wrap SVG content in a clickable group that posts the action via htmx.
fn svg_action_group(raw: u8, inner: Markup, game_id: &Uuid) -> Markup {
    let vals = format!(r#"{{"kind":"take","action":{raw}}}"#);
    html! {
        g
            hx-post={ "/game/" (game_id) "/action" }
            hx-vals=(vals)
            hx-target="#game"
            hx-swap="innerHTML"
            style="cursor:pointer"
        {
            (inner)
        }
    }
}

fn render_board_svg(gs: &CatanGameState, board_actions: &[Action], game_id: &Uuid) -> Markup {
    let geo = geometry();
    let vpos = vertex_positions();

    html! {
        svg
            viewBox="-285 -260 570 520"
            xmlns="http://www.w3.org/2000/svg"
            class="w-full bg-blue-100 rounded-xl"
        {
            // Hexes with number tokens.
            @for h in 0..NUM_HEXES {
                @let (q, r) = geo.hex_coords[h];
                @let (cx, cy) = hex_center(q, r);
                @let points: String = (0..6)
                    .map(|k| {
                        let o = corner_offset(k);
                        format!("{},{}", fmt2(cx + o.0), fmt2(cy + o.1))
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                polygon points=(points) fill=(terrain_fill(geo.hex_terrain[h])) stroke="#f5edd5" stroke-width="3" {}
                @if geo.hex_number[h] > 0 {
                    @let hot = geo.hex_number[h] == 6 || geo.hex_number[h] == 8;
                    circle cx=(fmt2(cx)) cy=(fmt2(cy)) r="14" fill="#f7f1dd" stroke="#7a6a4f" {}
                    text
                        x=(fmt2(cx)) y=(fmt2(cy + 5.0))
                        text-anchor="middle"
                        font-size="14"
                        font-weight="bold"
                        fill=(if hot { "#b91c1c" } else { "#1f2937" })
                    { (geo.hex_number[h]) }
                }
                // Robber.
                @if gs.robber_hex() as usize == h {
                    circle cx=(fmt2(cx - 20.0)) cy=(fmt2(cy - 16.0)) r="10" fill="#1f2937" stroke="#f9fafb" stroke-width="2" {}
                }
            }
            // Port labels, offset outward from the board center.
            @for v in 0..NUM_VERTICES {
                @if let Some(port) = geo.vertex_port[v] {
                    @let (x, y) = vpos[v];
                    @let label = match port {
                        Port::ThreeToOne => "3:1".to_string(),
                        Port::TwoToOne(r) => format!("2:1{}", res_emoji(r)),
                    };
                    text
                        x=(fmt2(x * 1.17)) y=(fmt2(y * 1.17 + 3.0))
                        text-anchor="middle"
                        font-size="11"
                        fill="#374151"
                    { (label) }
                }
            }
            // Roads.
            @for e in 0..NUM_EDGES {
                @if let Some(p) = gs.road_at(e) {
                    @let (a, b) = geo.edge_vertices[e];
                    @let (x1, y1) = vpos[a as usize];
                    @let (x2, y2) = vpos[b as usize];
                    line
                        x1=(fmt2(x1)) y1=(fmt2(y1)) x2=(fmt2(x2)) y2=(fmt2(y2))
                        stroke="#111827" stroke-width="9" stroke-linecap="round" {}
                    line
                        x1=(fmt2(x1)) y1=(fmt2(y1)) x2=(fmt2(x2)) y2=(fmt2(y2))
                        stroke=(seat_fill(p)) stroke-width="6" stroke-linecap="round" {}
                }
            }
            // Buildings: settlements are circles, cities are squares.
            @for v in 0..NUM_VERTICES {
                @if let Some((p, is_city)) = gs.building_at(v) {
                    @let (x, y) = vpos[v];
                    @if is_city {
                        rect
                            x=(fmt2(x - 9.0)) y=(fmt2(y - 9.0)) width="18" height="18"
                            fill=(seat_fill(p)) stroke="#111827" stroke-width="2" {}
                    } @else {
                        circle
                            cx=(fmt2(x)) cy=(fmt2(y)) r="8"
                            fill=(seat_fill(p)) stroke="#111827" stroke-width="2" {}
                    }
                }
            }
            // Clickable overlays for the viewer's legal board placements.
            @for &a in board_actions {
                (board_overlay(a, &vpos, game_id))
            }
        }
    }
}

fn board_overlay(a: Action, vpos: &[(f64, f64); NUM_VERTICES], game_id: &Uuid) -> Markup {
    let geo = geometry();
    let inner = match CatanAction::from_action(a) {
        CatanAction::BuildSettlement(v) => {
            let (x, y) = vpos[v as usize];
            html! {
                circle
                    cx=(fmt2(x)) cy=(fmt2(y)) r="11"
                    fill="rgba(34,197,94,0.5)" stroke="#15803d" stroke-width="2" stroke-dasharray="4 2" {}
            }
        }
        CatanAction::BuildCity(v) => {
            let (x, y) = vpos[v as usize];
            html! {
                rect
                    x=(fmt2(x - 11.0)) y=(fmt2(y - 11.0)) width="22" height="22"
                    fill="rgba(168,85,247,0.45)" stroke="#7e22ce" stroke-width="2" stroke-dasharray="4 2" {}
            }
        }
        CatanAction::BuildRoad(e) => {
            let (va, vb) = geo.edge_vertices[e as usize];
            let (x1, y1) = vpos[va as usize];
            let (x2, y2) = vpos[vb as usize];
            html! {
                line
                    x1=(fmt2(x1)) y1=(fmt2(y1)) x2=(fmt2(x2)) y2=(fmt2(y2))
                    stroke="rgba(34,197,94,0.55)" stroke-width="10" stroke-linecap="round" {}
            }
        }
        CatanAction::MoveRobber(h) => {
            let (q, r) = geo.hex_coords[h as usize];
            let (cx, cy) = hex_center(q, r);
            html! {
                circle
                    cx=(fmt2(cx)) cy=(fmt2(cy)) r="22"
                    fill="rgba(31,41,55,0.30)" stroke="#1f2937" stroke-width="2" stroke-dasharray="5 3" {}
            }
        }
        other => panic!("not a board action: {other:?}"),
    };
    svg_action_group(a.0, inner, game_id)
}

// ---------- Info panels ----------

fn render_turn_info(gs: &CatanGameState, gd: &GameData, south: Player) -> Markup {
    let (d1, d2) = gs.dice();
    let phase_line = match gs.phase() {
        CatanPhase::SetupSettlement | CatanPhase::SetupRoad => "Initial placement".to_string(),
        CatanPhase::Roll1 | CatanPhase::Roll2 => "Rolling the dice".to_string(),
        CatanPhase::Discard => "Discarding (a 7 was rolled)".to_string(),
        CatanPhase::MoveRobber => "Moving the robber".to_string(),
        CatanPhase::StealChoice => "Choosing who to rob".to_string(),
        CatanPhase::StealCard => "Robbing a card".to_string(),
        CatanPhase::DevDraw => "Drawing a development card".to_string(),
        CatanPhase::Main => "Build / trade".to_string(),
        CatanPhase::FreeRoad => "Placing free roads (Road Building)".to_string(),
        CatanPhase::PickYearOfPlenty => "Taking resources (Year of Plenty)".to_string(),
        CatanPhase::PickMonopoly => "Declaring a monopoly".to_string(),
    };
    let cur = gs.cur_player();
    let turn_line = if matches!(gd.display_state, GameProcessingState::WaitingHumanMove)
        && cur == south
    {
        "Your move".to_string()
    } else {
        format!("Waiting on {}", seat_name(cur))
    };
    html! {
        div {
            div class="font-bold text-xl" { "Turn" }
            div { (seat_name(gs.turn())) "'s turn · roll " (gs.num_rolls()) " of " (MAX_ROLLS) }
            div { "Phase: " (phase_line) }
            @if d1 > 0 && d2 > 0 {
                div { "Last roll: 🎲 " (d1) " + " (d2) " = " (d1 + d2) }
            }
            div class="italic" { (turn_line) }
        }
    }
}

fn render_player_table(gs: &CatanGameState, gd: &GameData, south: Player) -> Markup {
    let np = gs.num_players();
    html! {
        div {
            div class="font-bold text-xl" { "Players" }
            div class="grid grid-cols-5 gap-x-3 text-sm" {
                div class="font-semibold" { "Seat" }
                div class="font-semibold" { "VP" }
                div class="font-semibold" { "Cards" }
                div class="font-semibold" { "Devs" }
                div class="font-semibold" { "Army" }
                @for p in 0..np {
                    div { (seat_label(p, south, gd)) }
                    div {
                        // Hidden VP dev cards stay hidden for other seats.
                        @if p == south && gd.players[p].is_some() {
                            (gs.victory_points(p))
                        } @else {
                            (gs.public_victory_points(p))
                        }
                        @if gs.longest_road_holder() == Some(p) { " 🛣" }
                        @if gs.largest_army_holder() == Some(p) { " ⚔" }
                    }
                    div { (gs.hand_size(p)) }
                    div { (gs.dev_count(p)) }
                    div { (gs.knights_played(p)) }
                }
            }
            div class="text-xs text-gray-500 pt-1" {
                "🛣 longest road · ⚔ largest army · first to 10 VP wins"
            }
        }
    }
}

fn render_own_panel(gs: &CatanGameState, south: Player) -> Markup {
    let res = gs.resources(south);
    let dev = gs.dev_playable(south);
    let dev_new = gs.dev_bought_this_turn(south);
    let dev_names = ["Knight", "Road Building", "Year of Plenty", "Monopoly", "Victory Point"];
    let (roads, settlements, cities) = gs.pieces_left(south);
    html! {
        div {
            div class="font-bold text-xl" { "Your hand" }
            div class="flex flex-wrap gap-x-3" {
                @for r in RESOURCES {
                    span {
                        (res_emoji(r)) (res[r as usize])
                        span class="text-xs text-gray-500" { " (" (gs.bank_trade_rate(south, r)) ":1)" }
                    }
                }
            }
            @if dev.iter().sum::<u8>() + dev_new.iter().sum::<u8>() > 0 {
                div class="pt-1" { "Development cards:" }
                @for (i, name) in dev_names.iter().enumerate() {
                    @if dev[i] > 0 || dev_new[i] > 0 {
                        div class="text-sm" {
                            (name) ": " (dev[i])
                            @if dev_new[i] > 0 {
                                span class="text-gray-500" { " (+" (dev_new[i]) " new)" }
                            }
                        }
                    }
                }
            }
            div class="text-xs text-gray-500 pt-1" {
                "Pieces left: " (roads) " roads, " (settlements) " settlements, " (cities) " cities"
            }
        }
    }
}

fn render_bank_panel(gs: &CatanGameState) -> Markup {
    let bank = gs.bank();
    html! {
        div {
            div class="font-bold text-xl" { "Bank" }
            div class="flex flex-wrap gap-x-3" {
                @for r in RESOURCES {
                    span { (res_emoji(r)) (bank[r as usize]) }
                }
                span { "🂠" (gs.dev_deck_len()) }
            }
        }
    }
}

fn render_legend() -> Markup {
    html! {
        div class="text-xs text-gray-500" {
            div class="font-semibold text-sm text-gray-700" { "Costs" }
            div { "Road: 🧱🌲 · Settlement: 🧱🌲🌾🐑 · City: ⛰️⛰️⛰️🌾🌾 · Dev card: ⛰️🌾🐑" }
            div { "On the board: circles are settlements (1 VP), squares are cities (2 VP)." }
        }
    }
}

// ---------- Action panel ----------

fn render_action_panel(
    gs: &CatanGameState,
    gd: &GameData,
    south: Player,
    interactive: bool,
    panel_actions: &[Action],
    game_id: &Uuid,
) -> Markup {
    if !interactive {
        let cur = gs.cur_player();
        let line = match &gd.display_state {
            GameProcessingState::WaitingHumanMove if gd.players[cur].is_some() => {
                format!("Waiting on {}...", seat_name(cur))
            }
            _ => "Bots are playing...".to_string(),
        };
        return html! { div class="text-center italic py-2" { (line) } };
    }

    let hint = match gs.phase() {
        CatanPhase::SetupSettlement => Some("Place a settlement: click a highlighted spot."),
        CatanPhase::SetupRoad => Some("Place a road next to your new settlement."),
        CatanPhase::MoveRobber => Some("Move the robber: click a highlighted hex."),
        CatanPhase::FreeRoad => Some("Place your free road(s): click a highlighted edge."),
        CatanPhase::Discard => Some("Discard down to half your hand: pick cards to give up."),
        CatanPhase::StealChoice => Some("Choose a player to rob."),
        CatanPhase::PickYearOfPlenty => Some("Take a resource from the bank."),
        CatanPhase::PickMonopoly => {
            Some("Declare a resource: every player hands you all of theirs.")
        }
        CatanPhase::Main => None,
        _ => None,
    };

    html! {
        div class="grid gap-2 justify-items-center py-2" {
            @if let Some(h) = hint {
                div class="text-sm font-medium" { (h) }
            }
            @if gs.phase() == CatanPhase::Discard {
                div class="text-sm" { "Cards still to discard: " (gs.pending_discard(south)) }
            }
            div class="flex flex-wrap gap-2 justify-center" {
                @for &a in panel_actions {
                    (panel_button(a, gs, south, game_id))
                }
            }
        }
    }
}

fn panel_button(a: Action, gs: &CatanGameState, south: Player, game_id: &Uuid) -> Markup {
    let base =
        "bg-white outline outline-black hover:bg-slate-100 rounded-lg px-3 py-2 text-sm font-medium";
    let label: String = match CatanAction::from_action(a) {
        CatanAction::EndTurn => "End turn".to_string(),
        CatanAction::BuyDev => "Buy dev card ⛰️🌾🐑".to_string(),
        CatanAction::PlayKnight => "Play Knight ⚔".to_string(),
        CatanAction::PlayRoadBuilding => "Play Road Building".to_string(),
        CatanAction::PlayYearOfPlenty => "Play Year of Plenty".to_string(),
        CatanAction::PlayMonopoly => "Play Monopoly".to_string(),
        CatanAction::PickResource(r) => format!("{} {}", res_emoji(r), res_name(r)),
        CatanAction::BankTrade { give, get } => format!(
            "{}{} → 1{}",
            gs.bank_trade_rate(south, give),
            res_emoji(give),
            res_emoji(get)
        ),
        CatanAction::StealFrom(p) => format!("Steal from {}", seat_name(p)),
        other => panic!("not a panel action: {other:?}"),
    };
    action_form_button(a.0 as u32, &label, base, game_id)
}

// ---------- Seat labels ----------

fn seat_label(player: Player, south: Player, gd: &GameData) -> Markup {
    let mut s = seat_name(player).to_string();
    if player == south && gd.players[player].is_some() {
        s.push_str(" (you)");
    }
    if gd.players[player].is_none() {
        s.push_str(" [bot]");
    }
    let color = seat_fill(player);
    html! {
        span {
            span style={ "color:" (color) } { (PreEscaped("&#9632;")) }
            " " (s)
        }
    }
}
