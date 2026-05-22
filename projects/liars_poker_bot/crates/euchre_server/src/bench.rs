//! Benchmark API — lets external challenger agents play Euchre against
//! a choice of bench bots and records results on a leaderboard.
//!
//! Anti-cheat design: the challenger controls both seats of one team
//! (seats 0 and 2) while the bench agent plays seats 1 and 3. To prevent
//! the challenger from correlating seat-0 and seat-2 state within a single
//! game, the server runs N games concurrently and serves turns in shuffled
//! order with no game IDs exposed. A challenger may only have one active
//! session at a time; starting another while one is active returns the
//! in-progress session_id so the agent can resume.

use std::{
    collections::HashMap,
    env,
    path::PathBuf,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
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
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{db, new_game, AppState};

// ─── Types ───────────────────────────────────────────────────────────────────

pub(crate) type BenchAgent = Box<dyn Agent<EuchreGameState> + Send>;

/// Per-session shared state. Kept in AppState.
///
/// In-memory: only *active* sessions and the active-challenger lookup. The
/// `db` holds the historical record of every completed session — that's the
/// source of truth for `/bench`, `/bench/results`, and per-pair charts.
pub(crate) struct BenchState {
    pub(crate) sessions: Mutex<HashMap<Uuid, BenchSession>>,
    /// challenger_id -> active session_id. Enforces one-session-at-a-time
    /// and powers resume-on-conflict for start_session.
    pub(crate) active_challengers: Mutex<HashMap<String, Uuid>>,
    /// Loaded bench agents; read-only after startup so the outer map needs
    /// no Mutex — the inner Mutex handles concurrent inference per agent.
    pub(crate) agents: HashMap<String, Mutex<BenchAgent>>,
    /// SQLite connection for completed-session history.
    pub(crate) db: Mutex<Connection>,
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
    started_at: i64,
    complete: bool,
}

fn now_epoch_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
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
    agent_name: String,
}

/// Body returned alongside 409 Conflict when the challenger already has an
/// active session. Carries the in-progress session_id so the agent can
/// resume by POSTing /bench/sessions/{session_id}/move (with action=null to
/// learn the in-flight istate).
#[derive(Serialize)]
struct ActiveSessionConflict {
    error: &'static str,
    session_id: String,
    agent_name: String,
    num_games: usize,
}

#[derive(Deserialize)]
struct ResultsQuery {
    challenger_id: Option<String>,
    agent_name: Option<String>,
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

    // Resume-on-conflict path — before any expensive work, check whether
    // this challenger already has an active session. If so, return 409 with
    // the in-progress session_id in the body so an agent that crashed
    // mid-session can pick up where it left off rather than getting stuck.
    {
        let active = data.bench.active_challengers.lock().unwrap();
        if let Some(existing_id) = active.get(&req.challenger_id) {
            let sessions = data.bench.sessions.lock().unwrap();
            if let Some(existing) = sessions.get(existing_id) {
                if !existing.complete {
                    info!(
                        "bench session conflict|challenger={}|session_id={}",
                        req.challenger_id, existing_id
                    );
                    return HttpResponse::Conflict().json(ActiveSessionConflict {
                        error: "you already have an active session; resume it",
                        session_id: existing_id.to_string(),
                        agent_name: existing.agent_name.clone(),
                        num_games: existing.total_games,
                    });
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
        started_at: now_epoch_secs(),
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

    // The session may have completed during the initial bench-agent passes
    // (e.g. an agent that never gives the challenger a turn). Capture the
    // record now so we can persist it after releasing the locks.
    let immediate_completion_record = if session.complete {
        Some(session_to_record(&session, session_id))
    } else {
        None
    };

    // Insert under lock with TOCTOU re-check.
    {
        let mut active = data.bench.active_challengers.lock().unwrap();
        let mut sessions = data.bench.sessions.lock().unwrap();

        if let Some(existing_id) = active.get(&req.challenger_id) {
            if let Some(existing) = sessions.get(existing_id) {
                if !existing.complete {
                    info!(
                        "bench session conflict|challenger={}|session_id={}",
                        req.challenger_id, existing_id
                    );
                    return HttpResponse::Conflict().json(ActiveSessionConflict {
                        error: "you already have an active session; resume it",
                        session_id: existing_id.to_string(),
                        agent_name: existing.agent_name.clone(),
                        num_games: existing.total_games,
                    });
                }
            }
        }

        if !session.complete {
            active.insert(req.challenger_id.clone(), session_id);
        }
        sessions.insert(session_id, session);
    }

    if let Some(rec) = immediate_completion_record {
        if let Err(e) = db::insert_session(&data.bench.db, &rec) {
            log::error!("failed to persist completed session {session_id}: {e}");
        }
    }

    info!(
        "bench session created|challenger={}|agent={}|num_games={}|session_id={}",
        req.challenger_id, req.agent_name, req.num_games, session_id
    );

    HttpResponse::Ok().json(StartSessionResponse {
        session_id: session_id.to_string(),
        num_games: req.num_games,
        agent_name: req.agent_name,
    })
}

/// Snapshot a (completed) session into a `db::SessionRecord` for persistence.
fn session_to_record(session: &BenchSession, session_id: Uuid) -> db::SessionRecord {
    db::SessionRecord {
        session_id: session_id.to_string(),
        challenger_id: session.challenger_id.clone(),
        agent_name: session.agent_name.clone(),
        num_games: session.total_games as i64,
        started_at: session.started_at,
        completed_at: now_epoch_secs(),
        challenger_score: session.total_challenger_score as i64,
        agent_score: session.total_agent_score as i64,
        challenger_match_wins: session.total_challenger_match_wins as i64,
        agent_match_wins: session.total_agent_match_wins as i64,
        hands_played: session.total_hands as i64,
    }
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

    // Collected for post-lock results recording: snapshot of the completed
    // session ready to insert into the DB.
    let mut completion_record: Option<db::SessionRecord> = None;

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
        if let Some((game_id, seat)) = session.in_flight {
            let Some(action_byte) = req.action else {
                // No action submitted — treat as a "what's the current
                // turn?" probe. Used by an agent that resumed via
                // start_session and wants to recover the in-flight istate
                // it didn't get back originally. Does not advance state.
                let game = session.games.get(&game_id).unwrap();
                let gs = EuchreGameState::from(game.gs.as_str());
                let istate = gs.istate_string(seat);
                let legal_actions: Vec<u8> = actions!(gs).iter().map(|a| a.0).collect();
                return HttpResponse::Ok().json(BenchMoveResponse::Turn {
                    istate,
                    legal_actions,
                    games_done: session.games_done,
                    games_total: session.total_games,
                });
            };
            session.in_flight = None;

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
            completion_record = Some(session_to_record(session, session_id));
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
    if let Some(rec) = completion_record {
        info!(
            "bench session complete|challenger={}|agent={}|challenger_score={}|agent_score={}|c_match_wins={}|a_match_wins={}|hands={}",
            rec.challenger_id, rec.agent_name, rec.challenger_score, rec.agent_score,
            rec.challenger_match_wins, rec.agent_match_wins, rec.hands_played
        );
        {
            let mut active = data.bench.active_challengers.lock().unwrap();
            active.remove(&rec.challenger_id);
        }
        if let Err(e) = db::insert_session(&data.bench.db, &rec) {
            log::error!("failed to persist completed session {session_id}: {e}");
        }
    }

    HttpResponse::Ok().json(response)
}

async fn get_results(
    query: web::Query<ResultsQuery>,
    data: web::Data<AppState>,
) -> impl Responder {
    let q = query.into_inner();
    match db::list_sessions(
        &data.bench.db,
        q.challenger_id.as_deref(),
        q.agent_name.as_deref(),
    ) {
        Ok(rows) => HttpResponse::Ok().json(rows),
        Err(e) => {
            log::error!("get_results db error: {e}");
            HttpResponse::InternalServerError().body("db error")
        }
    }
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
   Returns 200: {{"session_id": "...", "num_games": 200,
                  "agent_name": "easy"}}

   - num_games must be in 1..=1000.
   - Only ONE active session per challenger_id at a time. If one is
     already active, the server returns:
         409 Conflict
         {{"error": "you already have an active session; resume it",
          "session_id": "<existing>",
          "agent_name": "<existing-agent>",
          "num_games": <existing-num>}}
     Use that session_id to resume — the existing agent_name and
     num_games are authoritative; the values you posted are ignored.
   - To recover the in-flight istate after resuming, call
     POST /bench/sessions/{{session_id}}/move with action=null. The
     server returns the current Turn response without advancing state.

3. Play loop. Repeat until you receive a Complete response:
       POST /bench/sessions/{{session_id}}/move
       First call:  {{"challenger_id": "your_unique_name", "action": null}}
       Subsequent:  {{"challenger_id": "your_unique_name", "action": <u8>}}

   Sending action=null on any call is a no-op probe: it returns the
   current Turn (or Complete) without applying anything. Use it after
   resuming to learn the in-flight istate.

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
       GET /bench                                       (HTML)
       GET /bench/results                               (JSON, every session, newest first)
       GET /bench/results?challenger_id=X               (filter)
       GET /bench/results?agent_name=Y                  (filter)
       GET /bench/results?challenger_id=X&agent_name=Y  (both filters)
       GET /bench/history/{{challenger_id}}/{{agent_name}} (HTML chart over time)

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

- 400 Bad Request: invalid action, bad num_games, unknown agent_name,
  malformed UUID.
- 403 Forbidden: challenger_id does not match the session.
- 404 Not Found: session_id does not exist.
- 409 Conflict: you already have an active session. The response body
  carries its session_id, agent_name, and num_games — resume that
  session instead of starting a new one.

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

// Win-rate helpers — used by both the leaderboard and the history chart.
fn win_pct(points_for: i64, points_against: i64) -> f64 {
    let total = points_for + points_against;
    if total == 0 {
        0.0
    } else {
        100.0 * points_for as f64 / total as f64
    }
}

// Match win rate: fraction of to-WIN_SCORE matches won. Sharper signal than
// point win rate — a team that consistently wins close matches will have a
// high match win rate but a mediocre point win rate, and vice versa.
fn match_win_pct(won: i64, lost: i64) -> f64 {
    let total = won + lost;
    if total == 0 {
        0.0
    } else {
        100.0 * won as f64 / total as f64
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

const BENCH_PAGE_CSS: &str = r#"<style>
  body { font-family: monospace; max-width: 960px; margin: 2rem auto; padding: 0 1rem; background: #fafafa; }
  h1 { border-bottom: 2px solid #333; }
  h2 { border-bottom: 1px solid #ccc; margin-top: 2rem; }
  h3 { margin-top: 1.5rem; color: #444; }
  table { border-collapse: collapse; width: 100%; margin-bottom: 1.5rem; }
  th, td { border: 1px solid #ccc; padding: 0.4rem 0.8rem; text-align: left; }
  th { background: #f0f0f0; }
  tr.clickable { cursor: pointer; }
  tr.clickable:hover { background: #eef; }
  a { color: #1452a3; text-decoration: none; }
  a:hover { text-decoration: underline; }
  .win { color: #1a7a1a; font-weight: bold; }
  .loss { color: #b00; }
  pre { background: #f4f4f4; padding: 1rem; border: 1px solid #ddd; overflow-x: auto; }
  ul { line-height: 1.8; }
  svg { background: #fff; border: 1px solid #ddd; }
</style>"#;

async fn ui_page(data: web::Data<AppState>) -> impl Responder {
    let mut agents: Vec<&String> = data.bench.agents.keys().collect();
    agents.sort();

    let latest = match db::latest_per_pair(&data.bench.db) {
        Ok(v) => v,
        Err(e) => {
            log::error!("ui_page latest_per_pair: {e}");
            return HttpResponse::InternalServerError().body("db error");
        }
    };
    let counts = match db::session_counts(&data.bench.db) {
        Ok(v) => v,
        Err(e) => {
            log::error!("ui_page session_counts: {e}");
            return HttpResponse::InternalServerError().body("db error");
        }
    };

    let mut html = String::from(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Euchre Benchmark Leaderboard</title>
"#,
    );
    html.push_str(BENCH_PAGE_CSS);
    html.push_str(
        "\n</head>\n<body>\n<h1>Euchre Benchmark Leaderboard</h1>\n\
         <p>Rates below are from each challenger's most recent completed \
         session against that agent. Click a row for a chart over time.</p>\n",
    );

    if latest.is_empty() {
        html.push_str("<p><em>No completed sessions yet.</em></p>\n");
    } else {
        // Group rows by challenger so each gets its own sub-table, matching
        // the old layout. Sort sub-tables by best match win % desc.
        let mut by_challenger: HashMap<String, Vec<&db::SessionRecord>> = HashMap::new();
        for rec in &latest {
            by_challenger
                .entry(rec.challenger_id.clone())
                .or_default()
                .push(rec);
        }
        let mut challenger_rows: Vec<(String, Vec<&db::SessionRecord>)> =
            by_challenger.into_iter().collect();
        challenger_rows.sort_by_key(|(c, _)| c.clone());

        // Fixed difficulty order so the table reads left-to-right by
        // increasing difficulty. Any agent outside the canonical set goes
        // after the known ones, alphabetically.
        const AGENT_ORDER: &[&str] = &["random", "easy", "medium", "hard"];
        let agent_rank = |name: &str| -> (usize, String) {
            match AGENT_ORDER.iter().position(|n| *n == name) {
                Some(i) => (i, String::new()),
                None => (AGENT_ORDER.len(), name.to_string()),
            }
        };
        for (challenger_id, mut rows) in challenger_rows {
            rows.sort_by(|a, b| agent_rank(&a.agent_name).cmp(&agent_rank(&b.agent_name)));
            html.push_str(&format!(
                "<h3>{}</h3>\n<table>\n\
                 <tr><th>Agent</th><th>Sessions</th><th>Latest Hands</th>\
                 <th>Latest Matches W–L</th><th>Latest Match Win%</th>\
                 <th>Latest Points For</th><th>Latest Points Against</th>\
                 <th>Latest Point Win%</th></tr>\n",
                html_escape(&challenger_id)
            ));
            for rec in rows {
                let mpct = match_win_pct(rec.challenger_match_wins, rec.agent_match_wins);
                let ppct = win_pct(rec.challenger_score, rec.agent_score);
                let mcls = if mpct >= 50.0 { "win" } else { "loss" };
                let pcls = if ppct >= 50.0 { "win" } else { "loss" };
                let sessions_count = counts
                    .get(&(rec.challenger_id.clone(), rec.agent_name.clone()))
                    .copied()
                    .unwrap_or(0);
                let url = format!(
                    "/bench/history/{}/{}",
                    urlencode(&rec.challenger_id),
                    urlencode(&rec.agent_name),
                );
                html.push_str(&format!(
                    "<tr class=\"clickable\" onclick=\"window.location='{url}'\">\
                     <td><a href=\"{url}\">{agent}</a></td>\
                     <td>{sessions}</td><td>{hands}</td>\
                     <td>{cw}–{aw}</td><td class=\"{mcls}\">{mpct:.1}%</td>\
                     <td>{cs}</td><td>{as_}</td>\
                     <td class=\"{pcls}\">{ppct:.1}%</td></tr>\n",
                    agent = html_escape(&rec.agent_name),
                    sessions = sessions_count,
                    hands = rec.hands_played,
                    cw = rec.challenger_match_wins,
                    aw = rec.agent_match_wins,
                    cs = rec.challenger_score,
                    as_ = rec.agent_score,
                ));
            }
            html.push_str("</table>\n");
        }
    }

    html.push_str("<h2>Available Agents</h2>\n<ul>\n");
    for name in &agents {
        html.push_str(&format!("<li>{}</li>\n", html_escape(name)));
    }
    html.push_str("</ul>\n");

    html.push_str("<h2>API Reference</h2>\n");
    html.push_str(
        "<p>Full LLM-friendly docs: <a href=\"/bench/help\">/bench/help</a></p>\n",
    );
    html.push_str("<pre>");
    html.push_str(
        r#"GET  /bench                     — this leaderboard page
GET  /bench/help                — full LLM-friendly API reference
GET  /bench/agents              — JSON list of available agent names
GET  /bench/results             — JSON list of every session, newest first
                                  filter: ?challenger_id=X&amp;agent_name=Y
GET  /bench/history/{c}/{a}     — HTML chart for one (challenger, agent) pair

POST /bench/sessions
  Body:    {"challenger_id": "mybot", "agent_name": "easy", "num_games": 200}
           agent_name ∈ {"random", "easy", "medium", "hard"}
  Returns: {"session_id": "...", "num_games": 200, "agent_name": "easy"}

  409 Conflict body when a session is already active:
    {"error": "...", "session_id": "...",
     "agent_name": "easy", "num_games": 200}
  Use the returned session_id to resume.

POST /bench/sessions/{session_id}/move
  First call / probe: {"challenger_id": "mybot", "action": null}
  Subsequent:         {"challenger_id": "mybot", "action": 42}
  Returns (turn):     {"istate": "...", "legal_actions": [3,7,12], "games_done": 5, "games_total": 200}
  Returns (complete): {"complete": true, "challenger_score": 142, "agent_score": 93, "hands_played": 340}

  Sending action=null after resume returns the current in-flight istate
  without advancing state.

Notes:
- Challenger controls BOTH seats of one team (seats 0 and 2).
- Bench agent controls seats 1 and 3.
- Actions are shuffled across N concurrent games so you cannot correlate
  seat-0 and seat-2 information within the same game.
- Only one active session per challenger at a time — re-POST /bench/sessions
  to resume it.
"#,
    );
    html.push_str("</pre>\n</body>\n</html>\n");

    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(html)
}

/// Minimal percent-encoding for URL path segments. Only encodes the few
/// characters that would actually break a URL; everything else is left as
/// the literal byte. Sufficient for challenger_id / agent_name which are
/// agent-chosen identifiers.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

// History page — per (challenger_id, agent_name), shows two line charts
// (match win rate and point win rate) across all sessions for that pair.
async fn history_page(
    path: web::Path<(String, String)>,
    data: web::Data<AppState>,
) -> impl Responder {
    let (challenger_id, agent_name) = path.into_inner();
    let rows = match db::pair_history(&data.bench.db, &challenger_id, &agent_name) {
        Ok(v) => v,
        Err(e) => {
            log::error!("history_page db error: {e}");
            return HttpResponse::InternalServerError().body("db error");
        }
    };

    let mut html = String::from(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Euchre Bench History</title>
"#,
    );
    html.push_str(BENCH_PAGE_CSS);
    html.push_str("\n</head>\n<body>\n");
    html.push_str("<p><a href=\"/bench\">&larr; back to leaderboard</a></p>\n");
    html.push_str(&format!(
        "<h1>{} vs {}</h1>\n",
        html_escape(&challenger_id),
        html_escape(&agent_name)
    ));

    if rows.is_empty() {
        html.push_str("<p><em>No completed sessions for this pair yet.</em></p>\n");
    } else {
        // Build (match%, point%) per session for the chart.
        let series: Vec<(f64, f64)> = rows
            .iter()
            .map(|r| {
                (
                    match_win_pct(r.challenger_match_wins, r.agent_match_wins),
                    win_pct(r.challenger_score, r.agent_score),
                )
            })
            .collect();

        html.push_str("<h2>Win rate over time</h2>\n");
        html.push_str(&render_winrate_svg(&series));
        html.push_str("<p><span style=\"color:#1452a3\">■</span> match win % &nbsp; \
            <span style=\"color:#1a7a1a\">■</span> point win %</p>\n");

        html.push_str("<h2>Sessions</h2>\n");
        html.push_str(
            "<table>\n<tr><th>#</th><th>Completed</th>\
             <th>Matches W–L</th><th>Match Win%</th>\
             <th>Points For</th><th>Points Against</th><th>Point Win%</th>\
             <th>Hands</th><th>Games</th></tr>\n",
        );
        // Display in reverse (newest first) so the most recent session is
        // visible without scrolling, but the chart x-axis still flows
        // oldest → newest.
        for (idx, rec) in rows.iter().enumerate().rev() {
            let mpct = match_win_pct(rec.challenger_match_wins, rec.agent_match_wins);
            let ppct = win_pct(rec.challenger_score, rec.agent_score);
            let mcls = if mpct >= 50.0 { "win" } else { "loss" };
            let pcls = if ppct >= 50.0 { "win" } else { "loss" };
            html.push_str(&format!(
                "<tr><td>{n}</td><td>{when}</td>\
                 <td>{cw}–{aw}</td><td class=\"{mcls}\">{mpct:.1}%</td>\
                 <td>{cs}</td><td>{as_}</td>\
                 <td class=\"{pcls}\">{ppct:.1}%</td>\
                 <td>{hands}</td><td>{games}</td></tr>\n",
                n = idx + 1,
                when = format_epoch_utc(rec.completed_at),
                cw = rec.challenger_match_wins,
                aw = rec.agent_match_wins,
                cs = rec.challenger_score,
                as_ = rec.agent_score,
                hands = rec.hands_played,
                games = rec.num_games,
            ));
        }
        html.push_str("</table>\n");
    }

    html.push_str("</body>\n</html>\n");
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(html)
}

/// Render a tiny inline SVG line chart of two series, each in 0..=100.
/// X = session index, Y = percent. No JS, no external chart lib.
fn render_winrate_svg(series: &[(f64, f64)]) -> String {
    const W: f64 = 700.0;
    const H: f64 = 260.0;
    const PAD_L: f64 = 40.0;
    const PAD_R: f64 = 12.0;
    const PAD_T: f64 = 12.0;
    const PAD_B: f64 = 30.0;
    let plot_w = W - PAD_L - PAD_R;
    let plot_h = H - PAD_T - PAD_B;

    // One point — render a dot at the lone x position so the chart still
    // says something. The path string would otherwise be empty.
    let n = series.len();
    let x_for = |i: usize| -> f64 {
        if n <= 1 {
            PAD_L + plot_w / 2.0
        } else {
            PAD_L + plot_w * (i as f64) / ((n - 1) as f64)
        }
    };
    let y_for = |v: f64| -> f64 {
        // Clamp; v should already be in [0, 100].
        let v = v.clamp(0.0, 100.0);
        PAD_T + plot_h * (1.0 - v / 100.0)
    };

    let build_path = |idx: usize| -> String {
        let mut d = String::new();
        for (i, pair) in series.iter().enumerate() {
            let v = if idx == 0 { pair.0 } else { pair.1 };
            let cmd = if i == 0 { 'M' } else { 'L' };
            d.push_str(&format!("{cmd}{:.1},{:.1} ", x_for(i), y_for(v)));
        }
        d
    };

    let mut s = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">"#
    );
    // Y gridlines at 0/25/50/75/100
    for pct in [0, 25, 50, 75, 100] {
        let y = y_for(pct as f64);
        s.push_str(&format!(
            r##"<line x1="{PAD_L}" y1="{y:.1}" x2="{x2:.1}" y2="{y:.1}" stroke="#eee" />"##,
            x2 = W - PAD_R
        ));
        s.push_str(&format!(
            r##"<text x="{x:.1}" y="{y:.1}" font-size="10" text-anchor="end" dominant-baseline="central" fill="#666">{pct}%</text>"##,
            x = PAD_L - 4.0
        ));
    }
    // 50% reference line, slightly darker.
    let y50 = y_for(50.0);
    s.push_str(&format!(
        r##"<line x1="{PAD_L}" y1="{y50:.1}" x2="{x2:.1}" y2="{y50:.1}" stroke="#bbb" stroke-dasharray="4,3" />"##,
        x2 = W - PAD_R
    ));

    // X axis label hints
    s.push_str(&format!(
        r##"<text x="{x:.1}" y="{y:.1}" font-size="10" fill="#666">session 1</text>"##,
        x = PAD_L,
        y = H - 10.0
    ));
    s.push_str(&format!(
        r##"<text x="{x:.1}" y="{y:.1}" font-size="10" text-anchor="end" fill="#666">session {n}</text>"##,
        x = W - PAD_R,
        y = H - 10.0
    ));

    // Match win %
    s.push_str(&format!(
        r##"<path d="{d}" fill="none" stroke="#1452a3" stroke-width="2" />"##,
        d = build_path(0)
    ));
    // Point win %
    s.push_str(&format!(
        r##"<path d="{d}" fill="none" stroke="#1a7a1a" stroke-width="2" />"##,
        d = build_path(1)
    ));
    // Dots for each session so single-point series are still visible.
    for (i, (m, p)) in series.iter().enumerate() {
        s.push_str(&format!(
            r##"<circle cx="{x:.1}" cy="{y:.1}" r="2.5" fill="#1452a3" />"##,
            x = x_for(i),
            y = y_for(*m)
        ));
        s.push_str(&format!(
            r##"<circle cx="{x:.1}" cy="{y:.1}" r="2.5" fill="#1a7a1a" />"##,
            x = x_for(i),
            y = y_for(*p)
        ));
    }
    s.push_str("</svg>");
    s
}

/// Format a unix epoch as `YYYY-MM-DD HH:MM:SS UTC`. Avoids a chrono dep —
/// computes the civil date from days-since-1970 with a standard algorithm.
fn format_epoch_utc(secs: i64) -> String {
    if secs <= 0 {
        return "—".to_string();
    }
    let days = secs.div_euclid(86_400);
    let time_of_day = secs.rem_euclid(86_400);
    let hh = (time_of_day / 3600) as u32;
    let mm = ((time_of_day / 60) % 60) as u32;
    let ss = (time_of_day % 60) as u32;
    let (y, mo, d) = civil_from_days(days);
    format!("{y:04}-{mo:02}-{d:02} {hh:02}:{mm:02}:{ss:02} UTC")
}

// Howard Hinnant's days-from-civil inverse. Returns (year, month, day) for a
// unix-epoch-day count (days since 1970-01-01, can be negative).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

// ─── Admin UI ────────────────────────────────────────────────────────────────
//
// /admin is gated by Caddy (basic-auth for non-local clients), so the
// handlers here trust the caller. The deploy is in
// ansible/configs/caddy/Caddyfile under the euchre.fewworddotrick.com host.

async fn admin_page(data: web::Data<AppState>) -> impl Responder {
    let summaries = match db::challenger_summaries(&data.bench.db) {
        Ok(v) => v,
        Err(e) => {
            log::error!("admin_page db error: {e}");
            return HttpResponse::InternalServerError().body("db error");
        }
    };

    // Active sessions in memory may have a challenger that hasn't completed
    // any session yet — surface them too so the admin can see/kill them.
    let active_extra: Vec<String> = {
        let active = data.bench.active_challengers.lock().unwrap();
        let in_db: std::collections::HashSet<&str> =
            summaries.iter().map(|s| s.challenger_id.as_str()).collect();
        active
            .keys()
            .filter(|k| !in_db.contains(k.as_str()))
            .cloned()
            .collect()
    };

    let mut html = String::from(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Platypus admin</title>
"#,
    );
    html.push_str(BENCH_PAGE_CSS);
    html.push_str("\n</head>\n<body>\n");
    html.push_str("<p><a href=\"/bench\">&larr; back to leaderboard</a></p>\n");
    html.push_str("<h1>Admin</h1>\n");
    html.push_str(
        "<p>Deleting a challenger wipes every recorded session for that \
         <code>challenger_id</code> from the leaderboard, history charts, and \
         results listing. It also drops any in-memory active session.</p>\n",
    );

    if summaries.is_empty() && active_extra.is_empty() {
        html.push_str("<p><em>No challengers on record.</em></p>\n");
    } else {
        html.push_str(
            "<table>\n<tr><th>Challenger</th><th>Sessions</th>\
             <th>Hands</th><th>Latest session</th><th>Action</th></tr>\n",
        );
        for s in &summaries {
            html.push_str(&format!(
                "<tr><td>{c}</td><td>{n}</td><td>{h}</td><td>{when}</td>\
                 <td>{form}</td></tr>\n",
                c = html_escape(&s.challenger_id),
                n = s.sessions,
                h = s.hands,
                when = format_epoch_utc(s.latest_completed_at),
                form = delete_form(&s.challenger_id),
            ));
        }
        for cid in &active_extra {
            html.push_str(&format!(
                "<tr><td>{c}</td><td>0</td><td>0</td>\
                 <td><em>active, no completed session</em></td>\
                 <td>{form}</td></tr>\n",
                c = html_escape(cid),
                form = delete_form(cid),
            ));
        }
        html.push_str("</table>\n");
    }

    html.push_str("</body>\n</html>\n");
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(html)
}

fn delete_form(challenger_id: &str) -> String {
    format!(
        r#"<form method="post" action="/admin/delete-challenger" onsubmit="return confirm('Delete all sessions for {esc_js}?');">
            <input type="hidden" name="challenger_id" value="{esc_html}">
            <button type="submit" style="background:#fff;border:1px solid #b00;color:#b00;padding:0.2rem 0.6rem;border-radius:3px;cursor:pointer;">Delete</button>
        </form>"#,
        esc_html = html_escape(challenger_id),
        // Apostrophes would break the confirm() string literal. Backslash-
        // escape them after html-escaping so the value still renders sanely.
        esc_js = html_escape(challenger_id).replace('\'', "\\'"),
    )
}

#[derive(Deserialize)]
struct DeleteChallengerForm {
    challenger_id: String,
}

async fn admin_delete_challenger(
    form: web::Form<DeleteChallengerForm>,
    data: web::Data<AppState>,
) -> impl Responder {
    let cid = form.into_inner().challenger_id;
    if cid.is_empty() {
        return HttpResponse::BadRequest().body("challenger_id is required");
    }

    let removed = match db::delete_challenger(&data.bench.db, &cid) {
        Ok(n) => n,
        Err(e) => {
            log::error!("admin_delete_challenger db error for {cid}: {e}");
            return HttpResponse::InternalServerError().body("db error");
        }
    };

    // Drop any in-memory active session this challenger has, so a stuck
    // session doesn't keep a freshly-deleted challenger pinned in active
    // state. Take both locks together to avoid TOCTOU between them.
    let dropped_session: Option<Uuid> = {
        let mut active = data.bench.active_challengers.lock().unwrap();
        let mut sessions = data.bench.sessions.lock().unwrap();
        let sid = active.remove(&cid);
        if let Some(id) = sid {
            sessions.remove(&id);
        }
        sid
    };

    info!(
        "admin deleted challenger|challenger={}|rows_removed={}|dropped_active_session={:?}",
        cid, removed, dropped_session
    );

    HttpResponse::SeeOther()
        .insert_header(("Location", "/admin"))
        .finish()
}

// ─── Route registration ──────────────────────────────────────────────────────

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/bench", web::get().to(ui_page))
        .route("/bench/help", web::get().to(help_docs))
        .route("/bench/agents", web::get().to(list_agents))
        .route("/bench/results", web::get().to(get_results))
        .route(
            "/bench/history/{challenger_id}/{agent_name}",
            web::get().to(history_page),
        )
        .route("/bench/sessions", web::post().to(start_session))
        .route("/bench/sessions/{id}/move", web::post().to(make_move))
        .route("/admin", web::get().to(admin_page))
        .route(
            "/admin/delete-challenger",
            web::post().to(admin_delete_challenger),
        );
}
