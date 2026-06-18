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
use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;

/// Bump when `SCHEMA_V1`/later migrations change; `migrate` applies each step
/// past the DB's stored `PRAGMA user_version`.
const SCHEMA_VERSION: i64 = 2;

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

/// V2: Amdion Notes — the capture store behind the Attention layer's capture
/// capability (highlights, typed notes, viewport screenshots). Additive: the
/// `events`/`sessions`/`blocks` tables are untouched, matching the append-only
/// migration convention. Screenshots live as PNG/JPEG FILES under
/// `app-data/notes/`; only the relative `image_path` is stored, so the DB stays
/// lean and Observer queries stay fast. `body` is the searchable text (a
/// highlighted quote or a typed note). The schema is agent-ready: a flat,
/// queryable log an agent can read to compile a digest.
const SCHEMA_V2: &str = "\
CREATE TABLE IF NOT EXISTS notes (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    ts           INTEGER NOT NULL,   -- UTC epoch millis (capture time)
    kind         TEXT    NOT NULL,   -- 'highlight' | 'note' | 'screenshot'
    source       TEXT    NOT NULL,   -- 'pdf' | 'web'
    source_url   TEXT,
    source_title TEXT,
    page         TEXT,               -- PDF page / scroll anchor (free text); nullable
    body         TEXT,               -- the quote or typed text (searchable)
    image_path   TEXT,               -- relative path under app-data ('notes/<file>'); nullable
    meta         TEXT,               -- JSON: capture method, crop rect, dpr
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_notes_ts ON notes(ts);
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

    /// True if any event of `kind` has been logged at or after `since_ts`. The
    /// front door uses this to tell a fresh session arrival (no `session_start`
    /// yet within the current session) from a re-summon (already greeted), so it
    /// logs and greets exactly once per session.
    pub fn has_event_since(&self, kind: &str, since_ts: i64) -> bool {
        match self.0.lock() {
            Ok(conn) => conn
                .query_row(
                    "SELECT COUNT(*) FROM events WHERE kind = ?1 AND ts >= ?2",
                    rusqlite::params![kind, since_ts],
                    |r| r.get::<_, i64>(0),
                )
                .map(|n| n > 0)
                .unwrap_or(false),
            Err(_) => false,
        }
    }

    // ── Notes (Amdion Notes — the Attention layer's capture store) ───────────

    /// Insert one captured note, stamped now. `image_path` is the relative path
    /// of an already-written screenshot file (the bytes are decoded and saved to
    /// disk by the caller, never stored in the row). Returns the new row id, or
    /// `None` if the write failed (logged, never propagated — a dropped capture
    /// must not crash the WS read loop).
    #[allow(clippy::too_many_arguments)]
    pub fn insert_note(
        &self,
        kind: &str,
        source: &str,
        source_url: Option<&str>,
        source_title: Option<&str>,
        page: Option<&str>,
        body: Option<&str>,
        image_path: Option<&str>,
        meta: Option<&str>,
    ) -> Option<i64> {
        let now = chrono::Utc::now().timestamp_millis();
        match self.0.lock() {
            Ok(conn) => {
                match conn.execute(
                    "INSERT INTO notes \
                     (ts, kind, source, source_url, source_title, page, body, image_path, meta, created_at, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    rusqlite::params![
                        now, kind, source, source_url, source_title, page, body, image_path, meta, now, now
                    ],
                ) {
                    Ok(_) => Some(conn.last_insert_rowid()),
                    Err(e) => {
                        eprintln!("[db] insert_note({kind}) failed: {e}");
                        None
                    }
                }
            }
            Err(e) => {
                eprintln!("[db] lock poisoned: {e}");
                None
            }
        }
    }

    /// The most-recent notes, newest first. Errors degrade to an empty list so
    /// the panel shows "nothing yet" rather than failing.
    pub fn list_notes(&self, limit: i64) -> Vec<Note> {
        let Ok(conn) = self.0.lock() else { return Vec::new() };
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, ts, kind, source, source_url, source_title, page, body, image_path \
             FROM notes ORDER BY ts DESC, id DESC LIMIT ?1",
        ) else {
            return Vec::new();
        };
        let rows = stmt.query_map([limit], map_note);
        match rows {
            Ok(it) => it.filter_map(Result::ok).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Notes whose body / title / url match `q` (case-insensitive substring),
    /// newest first.
    pub fn search_notes(&self, q: &str, limit: i64) -> Vec<Note> {
        let Ok(conn) = self.0.lock() else { return Vec::new() };
        let like = format!("%{}%", q.replace('%', "\\%").replace('_', "\\_"));
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, ts, kind, source, source_url, source_title, page, body, image_path \
             FROM notes \
             WHERE body LIKE ?1 ESCAPE '\\' OR source_title LIKE ?1 ESCAPE '\\' OR source_url LIKE ?1 ESCAPE '\\' \
             ORDER BY ts DESC, id DESC LIMIT ?2",
        ) else {
            return Vec::new();
        };
        let rows = stmt.query_map(rusqlite::params![like, limit], map_note);
        match rows {
            Ok(it) => it.filter_map(Result::ok).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// The relative `image_path` for one note, if it has a screenshot.
    pub fn note_image_path(&self, id: i64) -> Option<String> {
        let conn = self.0.lock().ok()?;
        conn.query_row("SELECT image_path FROM notes WHERE id = ?1", [id], |r| r.get(0))
            .ok()
            .flatten()
    }

    /// Delete one note, returning its `image_path` (if any) so the caller can
    /// remove the screenshot file from disk.
    pub fn delete_note(&self, id: i64) -> Option<String> {
        let conn = self.0.lock().ok()?;
        let path: Option<String> = conn
            .query_row("SELECT image_path FROM notes WHERE id = ?1", [id], |r| r.get(0))
            .ok()
            .flatten();
        if let Err(e) = conn.execute("DELETE FROM notes WHERE id = ?1", [id]) {
            eprintln!("[db] delete_note({id}) failed: {e}");
        }
        path
    }
}

/// One captured note as the panel sees it. `hasImage` lets the UI request the
/// screenshot lazily (via `get_note_image`) instead of inlining base64 into
/// every list row.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: i64,
    pub ts: i64,
    pub kind: String,
    pub source: String,
    pub source_url: Option<String>,
    pub source_title: Option<String>,
    pub page: Option<String>,
    pub body: Option<String>,
    pub has_image: bool,
}

/// Shared row → `Note` mapping for the list/search queries (same column order).
fn map_note(r: &rusqlite::Row) -> rusqlite::Result<Note> {
    let image_path: Option<String> = r.get(8)?;
    Ok(Note {
        id: r.get(0)?,
        ts: r.get(1)?,
        kind: r.get(2)?,
        source: r.get(3)?,
        source_url: r.get(4)?,
        source_title: r.get(5)?,
        page: r.get(6)?,
        body: r.get(7)?,
        has_image: image_path.is_some(),
    })
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
    if version < 2 {
        conn.execute_batch(SCHEMA_V2)?;
    }
    if version < SCHEMA_VERSION {
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    }
    Ok(())
}
