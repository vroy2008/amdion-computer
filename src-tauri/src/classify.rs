// Activity classifier (Step 3).
//
// A PURE function over the ordered `events` log that derives the
// session / block / break timeline ([[activity-sensing-model]]). The event log
// is the source of truth; this runs on read (the Observer calls it), so fixing
// the rules retroactively corrects history — nothing is baked into permanent
// rows.
//
// Ontology:
//   block   — one continuous run of presence. The sensing thread emits an
//             `active` at its start and an `idle` once the user has been away
//             for `break_threshold_mins`, so sub-threshold idles are already
//             absorbed into the block. Browser tab events also count as
//             presence; OS `app_focus` events set the block's app context.
//   break   — the gap between two blocks (idle ≥ break_threshold).
//   session — a run of blocks whose breaks are all shorter than
//             `session_gap_mins`. A longer break, or a hard `locked` boundary,
//             starts a new session.
//
// Robustness: durations come from timestamp deltas, never wall-clock sampling.
// A long no-event stretch *inside* a block is normal (the thread only emits
// transitions, so an hour in one app produces no rows). "Sensing was down" is
// detected by a `sensing_start` with no preceding `shutdown`, NOT by a time
// gap — a crashed run's open tail is closed at its last known activity rather
// than extended. Negative/absurd deltas (clock jumps) are clamped.

use serde::Serialize;
use std::collections::HashMap;

/// One raw row from the `events` table, already ordered by `(ts, id)`.
#[derive(Debug, Clone)]
pub struct Event {
    pub ts: i64,
    pub kind: String,
    pub source: String,
    pub app: Option<String>,
    pub url: Option<String>,
    pub meta: Option<String>,
}

/// Time attributed to one app within a block (or summed across a day).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSpan {
    pub bundle: String,
    pub name: String,
    pub ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    pub start_ts: i64,
    pub end_ts: i64,
    /// True if this block is the still-open "now" run (closed synthetically at
    /// the read time), so the UI can render it as live rather than final.
    pub open: bool,
    /// Dominant app over the block (display name), if any OS focus was seen.
    pub primary_context: Option<String>,
    /// Per-app durations within the block.
    pub apps: Vec<AppSpan>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub start_ts: i64,
    pub end_ts: i64,
    pub open: bool,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Timeline {
    pub sessions: Vec<Session>,
    /// Number of breaks (inter-block idle gaps) in the range.
    pub break_count: usize,
    /// Number of OS app-focus switches across all blocks.
    pub switch_count: usize,
}

/// Cap any single attributed span so a clock step (NTP/manual) can't mint a
/// thousand-hour block. 24h is comfortably longer than any real block.
const MAX_SPAN_MS: i64 = 24 * 60 * 60 * 1000;

/// Mutable block under construction.
struct Building {
    start_ts: i64,
    cur_app: Option<(String, String)>, // (bundle, name)
    cur_app_since: i64,
    last_evidence_ts: i64,
    apps: HashMap<String, AppSpan>,
    switches: usize,
    /// Set when the gap *before* this block was a hard `locked` boundary.
    hard_boundary_before: bool,
}

impl Building {
    fn new(start_ts: i64, hard_boundary_before: bool) -> Self {
        Building {
            start_ts,
            cur_app: None,
            cur_app_since: start_ts,
            last_evidence_ts: start_ts,
            apps: HashMap::new(),
            switches: 0,
            hard_boundary_before,
        }
    }

    /// Add the current app's span up to `until`, then optionally switch to a new
    /// app. Clamped to non-negative and `MAX_SPAN_MS`.
    fn flush_app(&mut self, until: i64) {
        if let Some((bundle, name)) = &self.cur_app {
            let span = (until - self.cur_app_since).clamp(0, MAX_SPAN_MS);
            let entry = self
                .apps
                .entry(bundle.clone())
                .or_insert_with(|| AppSpan { bundle: bundle.clone(), name: name.clone(), ms: 0 });
            entry.ms += span;
        }
        self.cur_app_since = until;
    }

    fn finish(mut self, end_ts: i64, open: bool) -> Built {
        let end_ts = end_ts.max(self.start_ts);
        self.flush_app(end_ts);
        let mut apps: Vec<AppSpan> = self.apps.into_values().filter(|a| a.ms > 0).collect();
        apps.sort_by(|a, b| b.ms.cmp(&a.ms));
        let primary_context = apps.first().map(|a| a.name.clone());
        Built {
            block: Block { start_ts: self.start_ts, end_ts, open, primary_context, apps },
            hard_before: self.hard_boundary_before,
            switches: self.switches,
        }
    }
}

/// A finished block plus the session-grouping metadata (`hard_before`) and the
/// focus-switch count that don't belong on the public `Block`.
struct Built {
    block: Block,
    hard_before: bool,
    switches: usize,
}

/// Does this event count as positive evidence the user is present?
fn is_activity(ev: &Event) -> bool {
    match ev.kind.as_str() {
        "active" | "app_focus" => ev.source == "os",
        "tab_opened" | "tab_activated" | "tab_navigated" => ev.source == "browser",
        _ => false,
    }
}

/// Idle seconds carried on an OS `idle` event (so the break start can be
/// back-dated to when activity actually stopped).
fn idle_secs_of(ev: &Event) -> i64 {
    ev.meta
        .as_deref()
        .and_then(|m| serde_json::from_str::<serde_json::Value>(m).ok())
        .and_then(|v| v.get("idleSecs").and_then(|s| s.as_i64()))
        .unwrap_or(0)
}

/// Is this a browser `idle_state` with `locked` — i.e. the user walked away
/// (a hard session boundary regardless of the break threshold)?
fn is_locked(ev: &Event) -> bool {
    ev.kind == "idle_state"
        && ev.source == "browser"
        && ev
            .meta
            .as_deref()
            .and_then(|m| serde_json::from_str::<serde_json::Value>(m).ok())
            .and_then(|v| v.get("state").and_then(|s| s.as_str()).map(|s| s == "locked"))
            .unwrap_or(false)
}

/// Derive the timeline from an ordered event slice. `now` is the read time
/// (closes an open trailing block); `break_ms`/`session_gap_ms` come from config.
pub fn classify(events: &[Event], now: i64, _break_ms: i64, session_gap_ms: i64) -> Timeline {
    let mut blocks: Vec<Built> = Vec::new();
    let mut cur: Option<Building> = None;
    let mut pending_hard = false; // a locked boundary applies to the NEXT block

    let close = |cur: &mut Option<Building>, blocks: &mut Vec<Built>, end_ts: i64| {
        if let Some(b) = cur.take() {
            blocks.push(b.finish(end_ts, false));
        }
    };

    for ev in events {
        // Sensing restarted without a clean shutdown ⇒ the prior run crashed.
        // Close its open block at the last evidence we have, don't extend it.
        if ev.kind == "sensing_start" {
            if let Some(b) = &cur {
                let last = b.last_evidence_ts;
                close(&mut cur, &mut blocks, last);
            }
            continue;
        }
        if ev.kind == "shutdown" {
            close(&mut cur, &mut blocks, ev.ts);
            continue;
        }
        if is_locked(ev) {
            close(&mut cur, &mut blocks, ev.ts);
            pending_hard = true;
            continue;
        }
        // OS idle transition: the block ended when activity actually stopped,
        // ~idleSecs before this event fired.
        if ev.kind == "idle" && ev.source == "os" {
            if let Some(b) = &cur {
                let end = (ev.ts - idle_secs_of(ev) * 1000).max(b.start_ts);
                close(&mut cur, &mut blocks, end);
            }
            continue;
        }

        if is_activity(ev) {
            let b = cur.get_or_insert_with(|| {
                let started = std::mem::take(&mut pending_hard);
                Building::new(ev.ts, started)
            });
            b.last_evidence_ts = ev.ts;
            // OS focus change sets the block's app context.
            if ev.kind == "app_focus" {
                if let Some(bundle) = &ev.app {
                    let name = ev
                        .meta
                        .as_deref()
                        .and_then(|m| serde_json::from_str::<serde_json::Value>(m).ok())
                        .and_then(|v| v.get("name").and_then(|s| s.as_str()).map(String::from))
                        .unwrap_or_else(|| bundle.clone());
                    let changed = b.cur_app.as_ref().map(|(cb, _)| cb != bundle).unwrap_or(true);
                    if changed {
                        b.flush_app(ev.ts);
                        if b.cur_app.is_some() {
                            b.switches += 1;
                        }
                        b.cur_app = Some((bundle.clone(), name));
                    }
                }
            }
        }
    }

    // Close any still-open block at the read time and mark it live.
    if let Some(b) = cur.take() {
        let mut built = b.finish(now.max(0), true);
        built.block.open = true;
        blocks.push(built);
    }

    group_into_sessions(blocks, session_gap_ms)
}

/// Group consecutive blocks into sessions: a new session starts on the first
/// block, after a break ≥ `session_gap_ms`, or after a hard `locked` boundary.
fn group_into_sessions(builts: Vec<Built>, session_gap_ms: i64) -> Timeline {
    let mut sessions: Vec<Session> = Vec::new();
    let mut switch_count = 0usize;
    let break_count = builts.len().saturating_sub(1);

    let mut prev_end: Option<i64> = None;
    for built in builts {
        switch_count += built.switches;
        let blk = built.block;
        let new_session = match prev_end {
            None => true,
            Some(end) => built.hard_before || (blk.start_ts - end) >= session_gap_ms,
        };
        prev_end = Some(blk.end_ts);
        if new_session {
            sessions.push(Session { start_ts: blk.start_ts, end_ts: blk.end_ts, open: blk.open, blocks: vec![blk] });
        } else if let Some(s) = sessions.last_mut() {
            s.end_ts = blk.end_ts;
            s.open = blk.open;
            s.blocks.push(blk);
        }
    }

    Timeline { sessions, break_count, switch_count }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: i64 = 60 * 1000;

    fn ev(ts: i64, kind: &str, source: &str, app: Option<&str>, meta: Option<&str>) -> Event {
        Event {
            ts,
            kind: kind.into(),
            source: source.into(),
            app: app.map(String::from),
            url: None,
            meta: meta.map(String::from),
        }
    }

    // A single active run that goes idle becomes one block in one session.
    #[test]
    fn single_block() {
        let evs = vec![
            ev(0, "sensing_start", "os", None, None),
            ev(0, "active", "os", None, None),
            ev(0, "app_focus", "os", Some("com.apple.Safari"), Some(r#"{"name":"Safari"}"#)),
            ev(30 * MIN, "idle", "os", None, Some(r#"{"idleSecs":0}"#)),
        ];
        let t = classify(&evs, 31 * MIN, 5 * MIN, 30 * MIN);
        assert_eq!(t.sessions.len(), 1);
        assert_eq!(t.sessions[0].blocks.len(), 1);
        let b = &t.sessions[0].blocks[0];
        assert_eq!((b.start_ts, b.end_ts), (0, 30 * MIN));
        assert_eq!(b.primary_context.as_deref(), Some("Safari"));
        assert!(!b.open);
    }

    // A short break (< session_gap) keeps both blocks in the SAME session;
    // a long break splits into two sessions.
    #[test]
    fn break_vs_session_boundary() {
        // block A [0,10m], break 8m, block B [18m, 28m] → same session (8m<30m)
        let short = vec![
            ev(0, "active", "os", None, None),
            ev(10 * MIN, "idle", "os", None, Some(r#"{"idleSecs":0}"#)),
            ev(18 * MIN, "active", "os", None, None),
            ev(28 * MIN, "idle", "os", None, Some(r#"{"idleSecs":0}"#)),
        ];
        let t = classify(&short, 29 * MIN, 5 * MIN, 30 * MIN);
        assert_eq!(t.sessions.len(), 1, "8m break stays one session");
        assert_eq!(t.sessions[0].blocks.len(), 2);
        assert_eq!(t.break_count, 1);

        // same but break of 40m → two sessions
        let long = vec![
            ev(0, "active", "os", None, None),
            ev(10 * MIN, "idle", "os", None, Some(r#"{"idleSecs":0}"#)),
            ev(50 * MIN, "active", "os", None, None),
            ev(60 * MIN, "idle", "os", None, Some(r#"{"idleSecs":0}"#)),
        ];
        let t2 = classify(&long, 61 * MIN, 5 * MIN, 30 * MIN);
        assert_eq!(t2.sessions.len(), 2, "40m break splits sessions");
    }

    // `idle` back-dates the block end by idleSecs.
    #[test]
    fn idle_backdates_block_end() {
        let evs = vec![
            ev(0, "active", "os", None, None),
            // idle fired at 10m but user had been away 5m → block ends at 5m
            ev(10 * MIN, "idle", "os", None, Some(r#"{"idleSecs":300}"#)),
        ];
        let t = classify(&evs, 11 * MIN, 5 * MIN, 30 * MIN);
        assert_eq!(t.sessions[0].blocks[0].end_ts, 5 * MIN);
    }

    // A browser `locked` is a hard session boundary even under session_gap.
    #[test]
    fn locked_forces_new_session() {
        let evs = vec![
            ev(0, "active", "os", None, None),
            ev(5 * MIN, "idle_state", "browser", None, Some(r#"{"state":"locked"}"#)),
            ev(6 * MIN, "active", "os", None, None), // only 1m later
            ev(16 * MIN, "idle", "os", None, Some(r#"{"idleSecs":0}"#)),
        ];
        let t = classify(&evs, 17 * MIN, 5 * MIN, 30 * MIN);
        assert_eq!(t.sessions.len(), 2, "locked splits despite 1m gap");
    }

    // sensing_start without a preceding shutdown closes the crashed tail at the
    // last evidence, not extending it across the downtime.
    #[test]
    fn crash_closes_tail_at_last_evidence() {
        let evs = vec![
            ev(0, "active", "os", None, None),
            ev(2 * MIN, "app_focus", "os", Some("com.apple.Terminal"), Some(r#"{"name":"Terminal"}"#)),
            // crash: no shutdown; sensing restarts hours later
            ev(300 * MIN, "sensing_start", "os", None, None),
            ev(300 * MIN, "active", "os", None, None),
            ev(310 * MIN, "idle", "os", None, Some(r#"{"idleSecs":0}"#)),
        ];
        let t = classify(&evs, 311 * MIN, 5 * MIN, 30 * MIN);
        // first block closed at last evidence (2m), not extended to 300m
        assert_eq!(t.sessions[0].blocks[0].end_ts, 2 * MIN);
        assert_eq!(t.sessions.len(), 2);
    }

    // An un-closed trailing run is closed at `now` and flagged open.
    #[test]
    fn open_block_at_now() {
        let evs = vec![ev(0, "active", "os", None, None)];
        let t = classify(&evs, 12 * MIN, 5 * MIN, 30 * MIN);
        let b = &t.sessions[0].blocks[0];
        assert!(b.open);
        assert_eq!(b.end_ts, 12 * MIN);
    }
}
