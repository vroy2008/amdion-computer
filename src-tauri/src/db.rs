// SQLite event store (Step 3).
//
// The deterministic activity log Amdion senses: every OS/browser transition is
// appended to `events` as a raw row. `sessions` and `blocks` are a *derived*
// cache the classifier recomputes from `events` (the source of truth), so the
// session/block/break timeline can be re-derived if the rules change. The DB
// lives in the app-data dir alongside `config.json` (the bundle is read-only
// once installed — see `config::app_data_dir`).
//
// A single `Mutex<Connection>` serializes the two writers (the WS bridge task
// and the sensing thread) and the Observer's read queries. WAL keeps readers
// from blocking the writer; critical sections stay tiny and are never held
// across an `.await`.

use crate::config::app_data_dir;
use rusqlite::Connection;
use std::sync::Mutex;
use std::time::Duration;

/// Bump when `SCHEMA_V1`/later migrations change; `migrate` applies each step
/// past the DB's stored `PRAGMA user_version`.
const SCHEMA_VERSION: i64 = 1;

const SCHEMA_V1: &str = "\
CREATE TABLE IF NOT EXISTS events (
    id     INTEGER PRIMARY KEY AUTOINCREMENT,
    ts     INTEGER NOT NULL,   -- UTC epoch millis (matches state.session_start_ms)
    kind   TEXT    NOT NULL,   -- tab_*/idle_state/app_focus/idle/active/sensing_start/shutdown
    source TEXT    NOT NULL,   -- 'os' | 'browser'
    app    TEXT,               -- bundle id (os) — stable grouping key; null for browser
    url    TEXT,
    meta   TEXT                -- JSON blob, event-specific (e.g. display name, payload)
);
CREATE INDEX IF NOT EXISTS idx_events_ts ON events(ts);

CREATE TABLE IF NOT EXISTS sessions (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    start_ts INTEGER NOT NULL,
    end_ts   INTEGER
);

CREATE TABLE IF NOT EXISTS blocks (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id      INTEGER NOT NULL,
    start_ts        INTEGER NOT NULL,
    end_ts          INTEGER,
    primary_context TEXT
);
";

/// Managed Tauri state. `Mutex<Connection>` is `Send + Sync`, so `Db` can be
/// reached from both the async bridge task and the sensing thread via
/// `app.try_state::<Db>()`.
pub struct Db(pub Mutex<Connection>);

impl Db {
    /// Open (or create) `amdion.db`, set pragmas, and migrate. Falls back to an
    /// in-memory store if the file can't be opened, so the app still runs (it
    /// just won't persist) rather than panicking on startup.
    pub fn new() -> Self {
        let path = app_data_dir().join("amdion.db");
        let conn = Connection::open(&path).unwrap_or_else(|e| {
            eprintln!(
                "[db] failed to open {}: {e}; falling back to in-memory store",
                path.display()
            );
            Connection::open_in_memory().expect("in-memory sqlite")
        });
        if let Err(e) = configure(&conn) {
            eprintln!("[db] configure/migrate failed: {e}");
        }
        Db(Mutex::new(conn))
    }

    /// Append one raw event, stamped with the current UTC epoch-millis. Errors
    /// are logged, never propagated — a dropped event must not crash a sensing
    /// tick or a WS frame. (`ts, id` give the classifier a total order.)
    pub fn insert_event(
        &self,
        kind: &str,
        source: &str,
        app: Option<&str>,
        url: Option<&str>,
        meta: Option<&str>,
    ) {
        let ts = chrono::Utc::now().timestamp_millis();
        match self.0.lock() {
            Ok(conn) => {
                if let Err(e) = conn.execute(
                    "INSERT INTO events (ts, kind, source, app, url, meta) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![ts, kind, source, app, url, meta],
                ) {
                    eprintln!("[db] insert_event({kind}) failed: {e}");
                }
            }
            Err(e) => eprintln!("[db] lock poisoned: {e}"),
        }
    }
}

impl Default for Db {
    fn default() -> Self {
        Self::new()
    }
}

/// Pragmas + migrations. `journal_mode`/`synchronous` go through `execute_batch`
/// (which discards the row `PRAGMA journal_mode=WAL` returns); `busy_timeout`
/// uses the typed helper.
fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.busy_timeout(Duration::from_secs(5))?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
    migrate(conn)
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 1 {
        conn.execute_batch(SCHEMA_V1)?;
    }
    if version < SCHEMA_VERSION {
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    }
    Ok(())
}
