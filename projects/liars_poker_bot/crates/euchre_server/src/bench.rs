//! Benchmark API — lets external challenger agents play Euchre against
//! a choice of bench bots and records results on a leaderboard.
//!
//! Anti-cheat design: the challenger controls both seats of one team
//! (seats 0 and 2) while the bench agent plays seats 1 and 3. To prevent
//! the challenger from correlating seat-0 and seat-2 state within a single
//! game, the server runs N games concurrently and serves turns in shuffled
//! order with no game IDs exposed. Each challenger may only have one
//! active session at a time, and sessions cannot be abandoned.

use std::{
    collections::HashMap,
    env,
    path::PathBuf,
    sync::Mutex,
};

use actix_web::{web, HttpResponse, Responder};
use card_platypus::{
    agents::Agent,
    algorithms::{cfres::CFRES, open_hand_solver::OpenHandSolver, pimcts::PIMCTSBot},
};
use games::{
    actions,
    gamestates::euchre::EuchreGameState,
    Action, GameState,
};
use log::info;
use rand::{
    rng, rngs::StdRng, seq::IndexedRandom, RngExt, SeedableRng,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{new_game, AppState};

// ─── Types ───────────────────────────────────────────────────────────────────

pub(crate) type BenchAgent = Box<dyn Agent<EuchreGameState> + Send>;

/// Per-session shared state. Kept in AppState.
#[derive(Default)]
pub(crate) struct BenchState {
    pub(crate) sessions: Mutex<HashMap<Uuid, BenchSession>>,
    pub(crate) results: Mutex<HashMap<String, ChallengerStats>>,
    /// challenger_id -> active session_id. Enforces one-session-at-a-time.
    pub(crate) active_challengers: Mutex<HashMap<String, Uuid>>,
    /// Loaded bench agents; read-only after startup so the outer map needs
    /// no Mutex — the inner Mutex handles concurrent inference per agent.
    pub(crate) agents: HashMap<String, Mutex<BenchAgent>>,
}

/// State of a single Euchre match within a benchmark session.
pub(crate) struct BenchGameData {
    gs: String,
    /// Cumulative points scored by challenger team (seats 0, 2) this match
    challenger_score: usize,
    /// Cumulative points scored by bench agent team (seats 1, 3) this match
    agent_score: usize,
    hands_played: usize,
    complete: bool,
}

/// A benchmark session: N concurrent games played in shuffled order.
pub(crate) struct BenchSession {
    challenger_id: String,
    agent_name: String,
    games: HashMap<Uuid, BenchGameData>,
    /// (game_id, seat) pairs waiting for a challenger action. Drawn at
    /// random so the challenger can't deduce game pairings.
    pending: Vec<(Uuid, usize)>,
    /// The (game_id, seat) currently waiting for a response.
    in_flight: Option<(Uuid, usize)>,
    games_done: usize,
    total_games: usize,
    total_challenger_score: usize,
    total_agent_score: usize,
    total_challenger_match_wins: usize,
    total_agent_match_wins: usize,
    total_hands: usize,
    complete: bool,
}

#[derive(Serialize, Clone, Default)]
pub(crate) struct AgentRecord {
    // "games" here is sessions, kept for backwards compatibility with the
    // /bench/results consumers; matches_won + matches_lost is the per-match
    // tally added when match-level scoring was introduced.
    games: usize,
    matches_won: usize,
    matches_lost: usize,
    points: usize,
    opp_points: usize,
}

#[derive(Serialize, Clone, Default)]
pub(crate) struct ChallengerStats {
    games_completed: usize,
    hands_played: usize,
    matches_won: usize,
    matches_lost: usize,
    total_points: usize,
    total_opp_points: usize,
    per_agent: HashMap<String, AgentRecord>,
}

// ─── Request / response types ────────────────────────────────────────────────

#[derive(Deserialize)]
struct StartSessionRequest {
    challenger_id: String,
    agent_name: String,
    num_games: usize,
}

#[derive(Serialize)]
struct StartSessionResponse {
    session_id: String,
    num_games: usize,
}

#[derive(Deserialize)]
struct BenchMoveRequest {
    challenger_id: String,
    /// None on the first call; the u8 action value on subsequent calls.
    action: Option<u8>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum BenchMoveResponse {
    Turn {
        istate: String,
        legal_actions: Vec<u8>,
        games_done: usize,
        games_total: usize,
    },
    Complete {
        complete: bool,
        challenger_score: usize,
        agent_score: usize,
        challenger_match_wins: usize,
        agent_match_wins: usize,
        hands_played: usize,
    },
}

// ─── Random agent (Send-compatible) ──────────────────────────────────────────

/// A simple random agent that uses an `StdRng` so it satisfies `Send`.
/// The stdlib `RandomAgent` uses `ThreadRng`, which is `!Send`.
pub(crate) struct StdRandomAgent {
    rng: StdRng,
}

impl Agent<EuchreGameState> for StdRandomAgent {
    fn step(&mut self, s: &EuchreGameState) -> Action {
        let mut actions = Vec::new();
        s.legal_actions(&mut actions);
        *actions.choose(&mut self.rng).unwrap()
    }
    fn get_name(&self) -> String {
        "random".to_string()
    }
}

// ─── Agent loading ───────────────────────────────────────────────────────────

const MEDIUM_WEIGHT_PATH: &str = "/home/steven/card_platypus/infostate.baseline";
const HARD_WEIGHT_PATH: &str = "/home/steven/card_platypus/infostate.three_card_played_f32";
const MEDIUM_WEIGHT_PATH_ENV: &str = "EUCHRE_MEDIUM_WEIGHTS_PATH";
const HARD_WEIGHT_PATH_ENV: &str = "EUCHRE_HARD_WEIGHTS_PATH";

/// Build the set of bench agents at server startup. Missing weight files
/// are skipped with a log message so development works without training.
pub(crate) fn load_bench_agents() -> HashMap<String, Mutex<BenchAgent>> {
    let mut agents: HashMap<String, Mutex<BenchAgent>> = HashMap::new();

    // random — uniform over legal actions
    agents.insert(
        "random".to_string(),
        Mutex::new(Box::new(StdRandomAgent {
            rng: StdRng::from_rng(&mut rng()),
        })),
    );
    info!("bench agent 'random' loaded");

    // easy — PIMCTS with open-hand solver, no trained weights
    let pimcts: PIMCTSBot<EuchreGameState, OpenHandSolver<EuchreGameState>> = PIMCTSBot::new(
        50,
        OpenHandSolver::new_euchre(),
        StdRng::from_rng(&mut rng()),
    );
    agents.insert("easy".to_string(), Mutex::new(Box::new(pimcts)));
    info!("bench agent 'easy' loaded (PIMCTS, 50 rollouts)");

    // medium — CFR trained on bidding only (max_cards_played = 0)
    let medium_path: PathBuf = env::var(MEDIUM_WEIGHT_PATH_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(MEDIUM_WEIGHT_PATH));
    if medium_path.exists() {
        let agent = CFRES::new_euchre(
            StdRng::from_rng(&mut rng()),
            0,
            Some(medium_path.as_path()),
        );
        info!(
            "bench agent 'medium' loaded from {} ({} infostates)",
            medium_path.display(),
            agent.num_info_states()
        );
        agents.insert("medium".to_string(), Mutex::new(Box::new(agent)));
    } else {
        info!(
            "bench agent 'medium' skipped: weights not found at {}",
            medium_path.display()
        );
    }

    // hard — CFR trained through 3 cards played
    let hard_path: PathBuf = env::var(HARD_WEIGHT_PATH_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(HARD_WEIGHT_PATH));
    if hard_path.exists() {
        let agent = CFRES::new_euchre(
            StdRng::from_rng(&mut rng()),
            3,
            Some(hard_path.as_path()),
        );
        info!(
            "bench agent 'hard' loaded from {} ({} infostates)",
            hard_path.display(),
            agent.num_info_states()
        );
        agents.insert("hard".to_string(), Mutex::new(Box::new(agent)));
    } else {
        info!(
            "bench agent 'hard' skipped: weights not found at {}",
            hard_path.display()
        );
    }

    agents
}

// ─── Game advancement ────────────────────────────────────────────────────────

enum AdvanceResult {
    /// Seat 0 or 2 needs to act next.
    NeedsChallenger(usize),
    /// This match reached WIN_SCORE points.
    MatchComplete {
        challenger_pts: usize,
        agent_pts: usize,
        hands: usize,
    },
}

/// Runs the bench agent (seats 1, 3) until either the challenger (seats 0, 2)
/// needs to act, or the match ends.
fn advance_game(game: &mut BenchGameData, agent_mutex: &Mutex<BenchAgent>) -> AdvanceResult {
    loop {
        let mut gs = EuchreGameState::from(game.gs.as_str());

        if gs.is_terminal() {
            // evaluate(0) is positive when team (0, 2) wins, negative otherwise.
            let score = gs.evaluate(0);
            game.challenger_score += score.max(0.0) as usize;
            game.agent_score += (-score).max(0.0) as usize;
            game.hands_played += 1;

            if game.challenger_score >= crate::WIN_SCORE
                || game.agent_score >= crate::WIN_SCORE
            {
                game.complete = true;
                return AdvanceResult::MatchComplete {
                    challenger_pts: game.challenger_score,
                    agent_pts: game.agent_score,
                    hands: game.hands_played,
                };
            }

            // Start the next hand
            gs = new_game();
            game.gs = gs.to_string();
            continue;
        }

        let seat = gs.cur_player();
        if seat == 0 || seat == 2 {
            game.gs = gs.to_string();
            return AdvanceResult::NeedsChallenger(seat);
        }

        // Bench agent's turn (seat 1 or 3)
        let a = {
            let mut agent = agent_mutex.lock().unwrap();
            agent.step(&gs)
        };
        gs.apply_action(a);
        game.gs = gs.to_string();
    }
}

// ─── Handlers ────────────────────────────────────────────────────────────────

async fn list_agents(data: web::Data<AppState>) -> impl Responder {
    let names: Vec<&String> = data.bench.agents.keys().collect();
    HttpResponse::Ok().json(names)
}

async fn start_session(
    body: web::Json<StartSessionRequest>,
    data: web::Data<AppState>,
) -> impl Responder {
    let req = body.into_inner();

    if req.num_games == 0 || req.num_games > 1000 {
        return HttpResponse::BadRequest()
            .body("num_games must be between 1 and 1000");
    }

    let agent_mutex = match data.bench.agents.get(&req.agent_name) {
        Some(m) => m,
        None => {
            return HttpResponse::BadRequest()
                .body(format!("unknown agent: {}", req.agent_name));
        }
    };

    // First check — before expensive work — under active_challengers lock.
    {
        let active = data.bench.active_challengers.lock().unwrap();
        if let Some(existing_id) = active.get(&req.challenger_id) {
            let sessions = data.bench.sessions.lock().unwrap();
            if let Some(existing) = sessions.get(existing_id) {
                if !existing.complete {
                    return HttpResponse::Conflict()
                        .body("you already have an active session; finish it first");
                }
            }
        }
    }

    // Build the session outside of any lock — this may advance many games
    // through the bench agent before the challenger's first turn.
    let session_id = Uuid::new_v4();
    let mut session = BenchSession {
        challenger_id: req.challenger_id.clone(),
        agent_name: req.agent_name.clone(),
        games: HashMap::new(),
        pending: Vec::new(),
        in_flight: None,
        games_done: 0,
        total_games: req.num_games,
        total_challenger_score: 0,
        total_agent_score: 0,
        total_challenger_match_wins: 0,
        total_agent_match_wins: 0,
        total_hands: 0,
        complete: false,
    };

    for _ in 0..req.num_games {
        let game_id = Uuid::new_v4();
        let mut game = BenchGameData {
            gs: new_game().to_string(),
            challenger_score: 0,
            agent_score: 0,
            hands_played: 0,
            complete: false,
        };

        match advance_game(&mut game, agent_mutex) {
            AdvanceResult::NeedsChallenger(seat) => {
                session.pending.push((game_id, seat));
            }
            AdvanceResult::MatchComplete {
                challenger_pts,
                agent_pts,
                hands,
            } => {
                session.total_challenger_score += challenger_pts;
                session.total_agent_score += agent_pts;
                session.total_hands += hands;
                session.games_done += 1;
                // Each match goes to WIN_SCORE; only one side can reach it
                // in a given hand, so the winner is unambiguous.
                if challenger_pts >= crate::WIN_SCORE {
                    session.total_challenger_match_wins += 1;
                } else {
                    session.total_agent_match_wins += 1;
                }
            }
        }
        session.games.insert(game_id, game);
    }

    // Shuffle pending so early picks are already random.
    let mut shuffle_rng = rng();
    for i in (1..session.pending.len()).rev() {
        let j = shuffle_rng.random_range(0..=i);
        session.pending.swap(i, j);
    }

    if session.games_done == session.total_games {
        session.complete = true;
    }

    // Insert under lock with TOCTOU re-check.
    {
        let mut active = data.bench.active_challengers.lock().unwrap();
        let mut sessions = data.bench.sessions.lock().unwrap();

        if let Some(existing_id) = active.get(&req.challenger_id) {
            if let Some(existing) = sessions.get(existing_id) {
                if !existing.complete {
                    return HttpResponse::Conflict()
                        .body("you already have an active session; finish it first");
                }
            }
        }

        if !session.complete {
            active.insert(req.challenger_id.clone(), session_id);
        }
        sessions.insert(session_id, session);
    }

    info!(
        "bench session created|challenger={}|agent={}|num_games={}|session_id={}",
        req.challenger_id, req.agent_name, req.num_games, session_id
    );

    HttpResponse::Ok().json(StartSessionResponse {
        session_id: session_id.to_string(),
        num_games: req.num_games,
    })
}

async fn make_move(
    path: web::Path<String>,
    body: web::Json<BenchMoveRequest>,
    data: web::Data<AppState>,
) -> impl Responder {
    let session_id = match Uuid::parse_str(path.into_inner().as_str()) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().body("invalid session id"),
    };
    let req = body.into_inner();

    // Collected for post-lock results recording.
    // (challenger_id, agent_name, c_pts, a_pts, c_match_wins, a_match_wins, hands)
    let mut completion: Option<(String, String, usize, usize, usize, usize, usize)> = None;

    let response = {
        let mut sessions = data.bench.sessions.lock().unwrap();
        let session = match sessions.get_mut(&session_id) {
            Some(s) => s,
            None => return HttpResponse::NotFound().finish(),
        };

        if session.challenger_id != req.challenger_id {
            return HttpResponse::Forbidden()
                .body("challenger_id does not match this session");
        }

        if session.complete {
            return HttpResponse::Ok().json(BenchMoveResponse::Complete {
                complete: true,
                challenger_score: session.total_challenger_score,
                agent_score: session.total_agent_score,
                challenger_match_wins: session.total_challenger_match_wins,
                agent_match_wins: session.total_agent_match_wins,
                hands_played: session.total_hands,
            });
        }

        // Step 1: apply the previous action (if a state is in-flight).
        if let Some((game_id, seat)) = session.in_flight.take() {
            let Some(action_byte) = req.action else {
                session.in_flight = Some((game_id, seat));
                return HttpResponse::BadRequest()
                    .body("action is required when a game state is awaiting a response");
            };

            let agent_mutex = data
                .bench
                .agents
                .get(&session.agent_name)
                .expect("session references an unknown agent");

            let advance_result = {
                let game = session.games.get_mut(&game_id).unwrap();
                let mut gs = EuchreGameState::from(game.gs.as_str());
                let action = Action(action_byte);
                let legal = actions!(gs);

                if !legal.contains(&action) {
                    session.in_flight = Some((game_id, seat));
                    return HttpResponse::BadRequest().body(format!(
                        "illegal action {}; legal: {:?}",
                        action_byte,
                        legal.iter().map(|a| a.0).collect::<Vec<_>>()
                    ));
                }
                if gs.cur_player() != seat {
                    session.in_flight = Some((game_id, seat));
                    return HttpResponse::BadRequest().body(format!(
                        "wrong seat: expected {}, got current player {}",
                        seat,
                        gs.cur_player()
                    ));
                }

                gs.apply_action(action);
                game.gs = gs.to_string();
                advance_game(game, agent_mutex)
            };

            match advance_result {
                AdvanceResult::NeedsChallenger(next_seat) => {
                    session.pending.push((game_id, next_seat));
                }
                AdvanceResult::MatchComplete {
                    challenger_pts,
                    agent_pts,
                    hands,
                } => {
                    session.total_challenger_score += challenger_pts;
                    session.total_agent_score += agent_pts;
                    session.total_hands += hands;
                    session.games_done += 1;
                    if challenger_pts >= crate::WIN_SCORE {
                        session.total_challenger_match_wins += 1;
                    } else {
                        session.total_agent_match_wins += 1;
                    }
                }
            }
        }

        // Step 2: is the whole session done?
        if session.games_done == session.total_games && session.pending.is_empty() {
            session.complete = true;
            completion = Some((
                session.challenger_id.clone(),
                session.agent_name.clone(),
                session.total_challenger_score,
                session.total_agent_score,
                session.total_challenger_match_wins,
                session.total_agent_match_wins,
                session.total_hands,
            ));
            BenchMoveResponse::Complete {
                complete: true,
                challenger_score: session.total_challenger_score,
                agent_score: session.total_agent_score,
                challenger_match_wins: session.total_challenger_match_wins,
                agent_match_wins: session.total_agent_match_wins,
                hands_played: session.total_hands,
            }
        } else {
            // Step 3: pick the next pending (game_id, seat) at random.
            let idx = rng().random_range(0..session.pending.len());
            let (next_game_id, next_seat) = session.pending.swap_remove(idx);
            session.in_flight = Some((next_game_id, next_seat));

            let gs = EuchreGameState::from(session.games[&next_game_id].gs.as_str());
            let istate = gs.istate_string(next_seat);
            let legal_actions: Vec<u8> = actions!(gs).iter().map(|a| a.0).collect();

            BenchMoveResponse::Turn {
                istate,
                legal_actions,
                games_done: session.games_done,
                games_total: session.total_games,
            }
        }
    }; // sessions lock released

    // Record results and unblock challenger once the match is over.
    if let Some((challenger_id, agent_name, c_pts, a_pts, c_wins, a_wins, hands)) = completion {
        info!(
            "bench session complete|challenger={}|agent={}|challenger_score={}|agent_score={}|c_match_wins={}|a_match_wins={}|hands={}",
            challenger_id, agent_name, c_pts, a_pts, c_wins, a_wins, hands
        );
        {
            let mut active = data.bench.active_challengers.lock().unwrap();
            active.remove(&challenger_id);
        }
        {
            let mut results = data.bench.results.lock().unwrap();
            let stats = results.entry(challenger_id).or_default();
            stats.games_completed += 1;
            stats.hands_played += hands;
            stats.matches_won += c_wins;
            stats.matches_lost += a_wins;
            stats.total_points += c_pts;
            stats.total_opp_points += a_pts;
            let rec = stats.per_agent.entry(agent_name).or_default();
            rec.games += 1;
            rec.matches_won += c_wins;
            rec.matches_lost += a_wins;
            rec.points += c_pts;
            rec.opp_points += a_pts;
        }
    }

    HttpResponse::Ok().json(response)
}

async fn get_results(data: web::Data<AppState>) -> impl Responder {
    let results = data.bench.results.lock().unwrap();
    HttpResponse::Ok().json(&*results)
}

async fn help_docs(data: web::Data<AppState>) -> impl Responder {
    let mut agents: Vec<&String> = data.bench.agents.keys().collect();
    agents.sort();
    let agent_list = if agents.is_empty() {
        "(none loaded)".to_string()
    } else {
        agents
            .iter()
            .map(|s| format!("  - {s}"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let body = format!(
        r#"# Euchre Benchmark API

You are a Euchre-playing agent. Your goal is to play as many hands as
possible against a trained CFR bot and accumulate points for your team.

## How Euchre works (essentials)

- 4 players in 2 fixed teams: seats (0, 2) vs seats (1, 3).
- 24-card deck: 9, T, J, Q, K, A in each of 4 suits (s=spades, c=clubs,
  h=hearts, d=diamonds). Each player is dealt 5 cards; one card is turned
  face up after the deal.
- Game flow:
    1. **Pickup phase**: each player (starting left of dealer) chooses to
       have the dealer pick up the face-up card as trump (`T`) or pass (`P`).
       If picked up, the dealer must discard one card.
    2. **Trump-call phase**: if all 4 passed, each player may declare a
       different suit as trump or pass. Can't call the rejected suit.
    3. **Alone phase**: the trump caller decides whether to go alone (`L`)
       — the partner sits out — or play with the partner (`P`).
    4. **Play phase**: 5 tricks. Must follow suit if possible. Highest
       trump wins; otherwise highest card of the led suit wins.
- Trump ranking: Right Bower (J of trump), Left Bower (J of same color),
  A, K, Q, T, 9.
- Scoring per hand:
    - Caller takes 3-4 tricks: +1
    - Caller takes 5 (march): +2
    - Going alone and taking 5: +4
    - Caller euchred (takes <= 2): defending team +2
- Match ends when one team reaches 10 points.

## Your role

You control **both seats of one team** (seats 0 and 2). The bench agent
controls seats 1 and 3. To prevent cheating, you only see one player's
information at a time and you don't know which game/seat you're in —
the server interleaves N concurrent games and serves your requests in
random order.

## Workflow

1. List agents:
       GET /bench/agents
   Returns: ["random", "easy", "medium", "hard"]

   Difficulty tiers:
     - random: picks uniformly from legal actions
     - easy:   PIMCTS (open-hand Monte Carlo), no training
     - medium: CFR trained on bidding phase only
     - hard:   CFR trained through 3 cards played

2. Start a session:
       POST /bench/sessions
       {{"challenger_id": "your_unique_name",
        "agent_name": "easy",
        "num_games": 200}}
   Returns: {{"session_id": "...", "num_games": 200}}

   - num_games must be in 1..=1000
   - Only ONE active session per challenger_id at a time.
   - You cannot abandon a session — finish it before starting another.

3. Play loop. Repeat until you receive a Complete response:
       POST /bench/sessions/{{session_id}}/move
       First call:  {{"challenger_id": "your_unique_name", "action": null}}
       Subsequent:  {{"challenger_id": "your_unique_name", "action": <u8>}}

   Response shapes (untagged JSON):
       Turn:     {{"istate": "<info-state string>",
                  "legal_actions": [<u8>, ...],
                  "games_done": <int>,
                  "games_total": <int>}}
       Complete: {{"complete": true,
                  "challenger_score": <int>,
                  "agent_score": <int>,
                  "challenger_match_wins": <int>,
                  "agent_match_wins": <int>,
                  "hands_played": <int>}}

   *_score is total Euchre points (1, 2, or 4 per hand). *_match_wins is the
   count of to-10 matches won by each team within the session. With
   num_games=N, challenger_match_wins + agent_match_wins = N.

   Each submitted `action` must appear in the previous response's
   `legal_actions` list.

4. Inspect the leaderboard:
       GET /bench/results    (JSON)
       GET /bench            (HTML)

## Information state format

Pipe-delimited segments. Cards are `<rank><suit-letter>`, e.g. `Js` =
Jack of spades, `Td` = Ten of diamonds.

Example istate (player 0, post-bidding):
    "9cTcJcQcKc|Js|T|0S|9cAcKsJs|"

Segments:
    1. Your 5 cards (sorted), e.g. "9cTcJcQcKc"
    2. Face-up card, e.g. "Js"
    3. Pickup phase actions: 'T'=Pickup, 'P'=Pass (one char per player)
    4. Trump caller + trump: e.g. "0S" = player 0 called Spades
    5. (Dealer only) Discarded card after pickup
    6. Cards played in tricks so far, in play order

## Action encoding

Each action is a `u8` (the `Action.0` field). Always choose from the
`legal_actions` list returned by the server. You don't need to know the
exact integer mapping — parse the istate to understand the position.

## Available agents
{agents}

## Errors

- 400 Bad Request: invalid action, missing action, bad num_games,
  unknown agent_name, malformed UUID.
- 403 Forbidden: challenger_id does not match the session.
- 404 Not Found: session_id does not exist.
- 409 Conflict: you already have an active session.

## Example client (pseudocode)

    resp = POST /bench/sessions
           body = {{"challenger_id": "mybot",
                   "agent_name": "easy",
                   "num_games": 50}}
    sid = resp["session_id"]

    action = None
    while True:
        r = POST /bench/sessions/{{sid}}/move
            body = {{"challenger_id": "mybot", "action": action}}
        if r.get("complete"):
            print("done", r["challenger_score"], "-", r["agent_score"])
            break
        action = pick_action(r["istate"], r["legal_actions"])
"#,
        agents = agent_list
    );

    HttpResponse::Ok()
        .content_type("text/plain; charset=utf-8")
        .body(body)
}

async fn ui_page(data: web::Data<AppState>) -> impl Responder {
    let results = data.bench.results.lock().unwrap();
    let mut agents: Vec<&String> = data.bench.agents.keys().collect();
    agents.sort();

    fn win_pct(points_for: usize, points_against: usize) -> f64 {
        let total = points_for + points_against;
        if total == 0 {
            0.0
        } else {
            100.0 * points_for as f64 / total as f64
        }
    }
    // Match win rate: fraction of to-WIN_SCORE matches won. Sharper signal
    // than point win rate — a team that consistently wins close matches will
    // have a high match win rate but a mediocre point win rate, and vice
    // versa.
    fn match_win_pct(won: usize, lost: usize) -> f64 {
        let total = won + lost;
        if total == 0 {
            0.0
        } else {
            100.0 * won as f64 / total as f64
        }
    }

    let mut html = String::from(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Euchre Benchmark Leaderboard</title>
<style>
  body { font-family: monospace; max-width: 960px; margin: 2rem auto; padding: 0 1rem; background: #fafafa; }
  h1 { border-bottom: 2px solid #333; }
  h2 { border-bottom: 1px solid #ccc; margin-top: 2rem; }
  h3 { margin-top: 1.5rem; color: #444; }
  table { border-collapse: collapse; width: 100%; margin-bottom: 1.5rem; }
  th, td { border: 1px solid #ccc; padding: 0.4rem 0.8rem; text-align: left; }
  th { background: #f0f0f0; }
  .win { color: #1a7a1a; font-weight: bold; }
  .loss { color: #b00; }
  pre { background: #f4f4f4; padding: 1rem; border: 1px solid #ddd; overflow-x: auto; }
  ul { line-height: 1.8; }
</style>
</head>
<body>
<h1>Euchre Benchmark Leaderboard</h1>
"#,
    );

    html.push_str("<h2>Overall</h2>\n");
    if results.is_empty() {
        html.push_str("<p><em>No completed sessions yet.</em></p>\n");
    } else {
        html.push_str(
            "<table>\n<tr><th>Challenger</th><th>Sessions</th><th>Hands</th>\
             <th>Matches W–L</th><th>Match Win%</th>\
             <th>Points For</th><th>Points Against</th><th>Point Win%</th></tr>\n",
        );

        // Sort by match win rate primarily — it's the headline metric now —
        // with point win rate as the tiebreak for challengers who haven't
        // finished any matches yet.
        let mut rows: Vec<(&String, &ChallengerStats)> = results.iter().collect();
        rows.sort_by(|a, b| {
            let mpct_a = match_win_pct(a.1.matches_won, a.1.matches_lost);
            let mpct_b = match_win_pct(b.1.matches_won, b.1.matches_lost);
            mpct_b
                .partial_cmp(&mpct_a)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    let ppct_a = win_pct(a.1.total_points, a.1.total_opp_points);
                    let ppct_b = win_pct(b.1.total_points, b.1.total_opp_points);
                    ppct_b
                        .partial_cmp(&ppct_a)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });

        for (id, stats) in &rows {
            let mpct = match_win_pct(stats.matches_won, stats.matches_lost);
            let ppct = win_pct(stats.total_points, stats.total_opp_points);
            let mcls = if mpct >= 50.0 { "win" } else { "loss" };
            let pcls = if ppct >= 50.0 { "win" } else { "loss" };
            html.push_str(&format!(
                "<tr><td>{id}</td><td>{}</td><td>{}</td>\
                 <td>{}–{}</td><td class=\"{mcls}\">{mpct:.1}%</td>\
                 <td>{}</td><td>{}</td>\
                 <td class=\"{pcls}\">{ppct:.1}%</td></tr>\n",
                stats.games_completed,
                stats.hands_played,
                stats.matches_won,
                stats.matches_lost,
                stats.total_points,
                stats.total_opp_points,
            ));
        }
        html.push_str("</table>\n");
    }

    html.push_str("<h2>Per-Agent Breakdown</h2>\n");
    for (challenger_id, stats) in results.iter() {
        html.push_str(&format!("<h3>{challenger_id}</h3>\n<table>\n"));
        html.push_str(
            "<tr><th>Agent</th><th>Sessions</th>\
             <th>Matches W–L</th><th>Match Win%</th>\
             <th>Points For</th><th>Points Against</th><th>Point Win%</th></tr>\n",
        );
        let mut agent_rows: Vec<(&String, &AgentRecord)> = stats.per_agent.iter().collect();
        agent_rows.sort_by_key(|(name, _)| *name);
        for (agent_name, rec) in &agent_rows {
            let mpct = match_win_pct(rec.matches_won, rec.matches_lost);
            let ppct = win_pct(rec.points, rec.opp_points);
            let mcls = if mpct >= 50.0 { "win" } else { "loss" };
            let pcls = if ppct >= 50.0 { "win" } else { "loss" };
            html.push_str(&format!(
                "<tr><td>{agent_name}</td><td>{}</td>\
                 <td>{}–{}</td><td class=\"{mcls}\">{mpct:.1}%</td>\
                 <td>{}</td><td>{}</td>\
                 <td class=\"{pcls}\">{ppct:.1}%</td></tr>\n",
                rec.games,
                rec.matches_won, rec.matches_lost,
                rec.points, rec.opp_points,
            ));
        }
        html.push_str("</table>\n");
    }

    html.push_str("<h2>Available Agents</h2>\n<ul>\n");
    for name in &agents {
        html.push_str(&format!("<li>{name}</li>\n"));
    }
    html.push_str("</ul>\n");

    html.push_str("<h2>API Reference</h2>\n");
    html.push_str("<p>Full LLM-friendly docs: <a href=\"/bench/help\">/bench/help</a></p>\n");
    html.push_str("<pre>");
    html.push_str(
        r#"GET  /bench                     — this leaderboard page
GET  /bench/help                — full LLM-friendly API reference
GET  /bench/agents              — JSON list of available agent names
GET  /bench/results             — JSON leaderboard data

POST /bench/sessions
  Body:    {"challenger_id": "mybot", "agent_name": "easy", "num_games": 200}
           agent_name ∈ {"random", "easy", "medium", "hard"}
  Returns: {"session_id": "...", "num_games": 200}

POST /bench/sessions/{session_id}/move
  First call:  {"challenger_id": "mybot", "action": null}
  Subsequent:  {"challenger_id": "mybot", "action": 42}
  Returns (turn):     {"istate": "...", "legal_actions": [3,7,12], "games_done": 5, "games_total": 200}
  Returns (complete): {"complete": true, "challenger_score": 142, "agent_score": 93, "hands_played": 340}

Notes:
- Challenger controls BOTH seats of one team (seats 0 and 2).
- Bench agent controls seats 1 and 3.
- Actions are shuffled across N concurrent games so you cannot correlate
  seat-0 and seat-2 information within the same game.
- Only one active session per challenger at a time.
- Sessions cannot be abandoned — finish what you started.
"#,
    );
    html.push_str("</pre>\n</body>\n</html>\n");

    drop(results);

    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(html)
}

// ─── Route registration ──────────────────────────────────────────────────────

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/bench", web::get().to(ui_page))
        .route("/bench/help", web::get().to(help_docs))
        .route("/bench/agents", web::get().to(list_agents))
        .route("/bench/results", web::get().to(get_results))
        .route("/bench/sessions", web::post().to(start_session))
        .route("/bench/sessions/{id}/move", web::post().to(make_move));
}
