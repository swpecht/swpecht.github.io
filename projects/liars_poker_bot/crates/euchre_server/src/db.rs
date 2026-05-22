//! SQLite persistence for completed bench sessions.
//!
//! Active sessions live in memory only. When a session completes, its
//! aggregate result is appended here so the leaderboard, results listing,
//! and per-pair charts survive server restarts.

use std::{path::Path, sync::Mutex};

use rusqlite::{params, params_from_iter, Connection, Result as SqlResult};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct SessionRecord {
    pub session_id: String,
    pub challenger_id: String,
    pub agent_name: String,
    pub num_games: i64,
    pub started_at: i64,
    pub completed_at: i64,
    pub challenger_score: i64,
    pub agent_score: i64,
    pub challenger_match_wins: i64,
    pub agent_match_wins: i64,
    pub hands_played: i64,
}

pub fn open(path: &Path) -> SqlResult<Connection> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            // Ignore failure — the open() below will surface a clearer error
            // if the directory really is missing.
            let _ = std::fs::create_dir_all(parent);
        }
    }
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS sessions (
            session_id TEXT PRIMARY KEY,
            challenger_id TEXT NOT NULL,
            agent_name TEXT NOT NULL,
            num_games INTEGER NOT NULL,
            started_at INTEGER NOT NULL,
            completed_at INTEGER NOT NULL,
            challenger_score INTEGER NOT NULL,
            agent_score INTEGER NOT NULL,
            challenger_match_wins INTEGER NOT NULL,
            agent_match_wins INTEGER NOT NULL,
            hands_played INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_sessions_pair ON sessions(challenger_id, agent_name);
        CREATE INDEX IF NOT EXISTS idx_sessions_completed_at ON sessions(completed_at DESC);",
    )?;
    Ok(conn)
}

pub fn insert_session(conn: &Mutex<Connection>, rec: &SessionRecord) -> SqlResult<()> {
    conn.lock().unwrap().execute(
        "INSERT INTO sessions
            (session_id, challenger_id, agent_name, num_games, started_at,
             completed_at, challenger_score, agent_score,
             challenger_match_wins, agent_match_wins, hands_played)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            rec.session_id,
            rec.challenger_id,
            rec.agent_name,
            rec.num_games,
            rec.started_at,
            rec.completed_at,
            rec.challenger_score,
            rec.agent_score,
            rec.challenger_match_wins,
            rec.agent_match_wins,
            rec.hands_played,
        ],
    )?;
    Ok(())
}

fn row_to_record(row: &rusqlite::Row) -> SqlResult<SessionRecord> {
    Ok(SessionRecord {
        session_id: row.get(0)?,
        challenger_id: row.get(1)?,
        agent_name: row.get(2)?,
        num_games: row.get(3)?,
        started_at: row.get(4)?,
        completed_at: row.get(5)?,
        challenger_score: row.get(6)?,
        agent_score: row.get(7)?,
        challenger_match_wins: row.get(8)?,
        agent_match_wins: row.get(9)?,
        hands_played: row.get(10)?,
    })
}

const SELECT_COLS: &str = "session_id, challenger_id, agent_name, num_games, \
    started_at, completed_at, challenger_score, agent_score, \
    challenger_match_wins, agent_match_wins, hands_played";

/// List sessions, most-recent first, optionally filtered by challenger and/or agent.
pub fn list_sessions(
    conn: &Mutex<Connection>,
    challenger_id: Option<&str>,
    agent_name: Option<&str>,
) -> SqlResult<Vec<SessionRecord>> {
    let mut clauses: Vec<&str> = Vec::new();
    let mut args: Vec<String> = Vec::new();
    if let Some(c) = challenger_id {
        clauses.push("challenger_id = ?");
        args.push(c.to_string());
    }
    if let Some(a) = agent_name {
        clauses.push("agent_name = ?");
        args.push(a.to_string());
    }
    let where_clause = if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    };
    let sql = format!(
        "SELECT {SELECT_COLS} FROM sessions{where_clause} ORDER BY completed_at DESC, session_id DESC"
    );

    let conn = conn.lock().unwrap();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(args.iter()), row_to_record)?;
    rows.collect()
}

/// Sessions for a (challenger, agent) pair, ordered chronologically (oldest first).
/// Used to render the per-pair history chart.
pub fn pair_history(
    conn: &Mutex<Connection>,
    challenger_id: &str,
    agent_name: &str,
) -> SqlResult<Vec<SessionRecord>> {
    let conn = conn.lock().unwrap();
    let mut stmt = conn.prepare(&format!(
        "SELECT {SELECT_COLS} FROM sessions
         WHERE challenger_id = ? AND agent_name = ?
         ORDER BY completed_at ASC, session_id ASC"
    ))?;
    let rows = stmt.query_map(params![challenger_id, agent_name], row_to_record)?;
    rows.collect()
}

/// One row per (challenger_id, agent_name) — the latest completed session for
/// that pair. Used as the headline rate on the bench leaderboard.
pub fn latest_per_pair(conn: &Mutex<Connection>) -> SqlResult<Vec<SessionRecord>> {
    let conn = conn.lock().unwrap();
    // Use rowid as the secondary tiebreaker so multiple sessions completing
    // in the same second still pick a single deterministic "latest".
    let mut stmt = conn.prepare(&format!(
        "SELECT {SELECT_COLS} FROM sessions s
         WHERE rowid = (
             SELECT rowid FROM sessions s2
             WHERE s2.challenger_id = s.challenger_id AND s2.agent_name = s.agent_name
             ORDER BY completed_at DESC, rowid DESC
             LIMIT 1
         )
         ORDER BY challenger_id, agent_name"
    ))?;
    let rows = stmt.query_map([], row_to_record)?;
    rows.collect()
}

/// One row per challenger: total sessions, total hands played, latest
/// completed_at. Powers the admin UI.
#[derive(Debug, Clone, Serialize)]
pub struct ChallengerSummary {
    pub challenger_id: String,
    pub sessions: i64,
    pub hands: i64,
    pub latest_completed_at: i64,
}

pub fn challenger_summaries(conn: &Mutex<Connection>) -> SqlResult<Vec<ChallengerSummary>> {
    let conn = conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT challenger_id,
                COUNT(*) AS sessions,
                COALESCE(SUM(hands_played), 0) AS hands,
                COALESCE(MAX(completed_at), 0) AS latest
         FROM sessions
         GROUP BY challenger_id
         ORDER BY latest DESC, challenger_id ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(ChallengerSummary {
            challenger_id: row.get(0)?,
            sessions: row.get(1)?,
            hands: row.get(2)?,
            latest_completed_at: row.get(3)?,
        })
    })?;
    rows.collect()
}

/// Delete every recorded session for one challenger. Returns the number
/// of rows removed.
pub fn delete_challenger(conn: &Mutex<Connection>, challenger_id: &str) -> SqlResult<usize> {
    let conn = conn.lock().unwrap();
    conn.execute(
        "DELETE FROM sessions WHERE challenger_id = ?",
        params![challenger_id],
    )
}

/// Total session count per (challenger_id, agent_name).
pub fn session_counts(
    conn: &Mutex<Connection>,
) -> SqlResult<std::collections::HashMap<(String, String), i64>> {
    let conn = conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT challenger_id, agent_name, COUNT(*)
         FROM sessions
         GROUP BY challenger_id, agent_name",
    )?;
    let rows = stmt.query_map([], |row| {
        let c: String = row.get(0)?;
        let a: String = row.get(1)?;
        let n: i64 = row.get(2)?;
        Ok(((c, a), n))
    })?;
    let mut out = std::collections::HashMap::new();
    for r in rows {
        let (k, v) = r?;
        out.insert(k, v);
    }
    Ok(out)
}
