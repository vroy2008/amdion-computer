// Observer read commands (Step 3): typed daily stats over the event store —
// the read half of the agent-ready surface (the action half is `open_tab`,
// `apply_mac_tuning`, `save_config`).
//
// Derive-on-read: each call loads the local day's `events` and runs the pure
// classifier ([[crate::classify]]). The event log is the source of truth, so
// the session/block/break view always reflects the current thresholds. (The
// `sessions`/`blocks` tables exist as the documented materialization target for
// a future agent; v1 derives on read and leaves them empty.)

use crate::classify::{self, Event, Session};
use crate::config::read_config;
use crate::db::Db;
use chrono::{Duration, Local, NaiveDate, TimeZone};
use serde::Serialize;
use std::collections::HashMap;

const MS_PER_MIN: i64 = 60_000;
/// Cap one site-attribution span so leaving the browser for hours doesn't dump
/// all that idle time onto the last domain. Per-site time is approximate.
const MAX_SITE_SPAN_MS: i64 = 15 * MS_PER_MIN;

#[derive(Serialize)]
pub struct AppStat {
    pub bundle: String,
    pub name: String,
    pub ms: i64,
}

#[derive(Serialize)]
pub struct SiteStat {
    pub domain: String,
    pub ms: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailySummary {
    pub date: String,
    /// Span from the first event of the local day to now (respects sleep/breaks
    /// — the real "time on computer" the process-start placeholder stood in for).
    pub time_on_computer_ms: i64,
    /// Sum of block (active) durations.
    pub active_ms: i64,
    pub break_count: usize,
    pub switch_count: usize,
    pub apps: Vec<AppStat>,
    pub sites: Vec<SiteStat>,
}

/// `[start, end)` epoch-millis bounds of the given local day (default: today),
/// plus the normalized `YYYY-MM-DD` label.
fn day_bounds(date: &Option<String>) -> (String, i64, i64) {
    let day = date
        .as_deref()
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .unwrap_or_else(|| Local::now().date_naive());
    let start = day
        .and_hms_opt(0, 0, 0)
        .and_then(|naive| Local.from_local_datetime(&naive).earliest())
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0);
    let end = start + Duration::days(1).num_milliseconds();
    (day.format("%Y-%m-%d").to_string(), start, end)
}

/// Load the day's events ordered by `(ts, id)`. Errors degrade to an empty
/// slice — the Observer shows "nothing yet" rather than failing.
fn load_events(db: &Db, start: i64, end: i64) -> Vec<Event> {
    let Ok(conn) = db.0.lock() else { return Vec::new() };
    let Ok(mut stmt) = conn.prepare(
        "SELECT ts, kind, source, app, url, meta FROM events \
         WHERE ts >= ?1 AND ts < ?2 ORDER BY ts, id",
    ) else {
        return Vec::new();
    };
    let rows = stmt.query_map([start, end], |r| {
        Ok(Event {
            ts: r.get(0)?,
            kind: r.get(1)?,
            source: r.get(2)?,
            app: r.get(3)?,
            url: r.get(4)?,
            meta: r.get(5)?,
        })
    });
    match rows {
        Ok(it) => it.filter_map(Result::ok).collect(),
        Err(_) => Vec::new(),
    }
}

/// Read time, clamped to the day's end so a past day reads as fully elapsed.
fn now_clamped(end: i64) -> i64 {
    chrono::Utc::now().timestamp_millis().min(end)
}

fn build_timeline(events: &[Event], now: i64) -> classify::Timeline {
    let cfg = read_config();
    classify::classify(
        events,
        now,
        cfg.break_threshold_mins as i64 * MS_PER_MIN,
        cfg.session_gap_mins as i64 * MS_PER_MIN,
    )
}

/// The session/block/break timeline for a day. The trailing block carries
/// `open: true` so the UI can render it as live.
#[tauri::command]
pub fn get_sessions(db: tauri::State<'_, Db>, date: Option<String>) -> Result<Vec<Session>, String> {
    let (_date, start, end) = day_bounds(&date);
    let events = load_events(&db, start, end);
    Ok(build_timeline(&events, now_clamped(end)).sessions)
}

/// Aggregate stats for a day: time on computer, active time, break/switch
/// counts, and time per app and per site.
#[tauri::command]
pub fn get_daily_summary(
    db: tauri::State<'_, Db>,
    date: Option<String>,
) -> Result<DailySummary, String> {
    let (date_str, start, end) = day_bounds(&date);
    let events = load_events(&db, start, end);
    let cfg = read_config();
    Ok(summarize(
        &events,
        now_clamped(end),
        date_str,
        cfg.break_threshold_mins as i64 * MS_PER_MIN,
        cfg.session_gap_mins as i64 * MS_PER_MIN,
    ))
}

/// Pure aggregation over a day's events (IO-free + explicit thresholds, so it's
/// unit-testable): per-app time from the classified blocks, plus per-site time,
/// time on computer (first event → now), and the break/switch counts.
fn summarize(
    events: &[Event],
    now: i64,
    date: String,
    break_ms: i64,
    session_gap_ms: i64,
) -> DailySummary {
    let timeline = classify::classify(events, now, break_ms, session_gap_ms);

    let mut apps: HashMap<String, AppStat> = HashMap::new();
    let mut active_ms = 0i64;
    for s in &timeline.sessions {
        for b in &s.blocks {
            active_ms += (b.end_ts - b.start_ts).max(0);
            for a in &b.apps {
                let e = apps.entry(a.bundle.clone()).or_insert_with(|| AppStat {
                    bundle: a.bundle.clone(),
                    name: a.name.clone(),
                    ms: 0,
                });
                e.ms += a.ms;
            }
        }
    }
    let mut apps: Vec<AppStat> = apps.into_values().filter(|a| a.ms > 0).collect();
    apps.sort_by(|a, b| b.ms.cmp(&a.ms));

    let first_ts = events.first().map(|e| e.ts).unwrap_or(now);
    let time_on_computer_ms = (now - first_ts).max(0);

    DailySummary {
        date,
        time_on_computer_ms,
        active_ms,
        break_count: timeline.break_count,
        switch_count: timeline.switch_count,
        apps,
        sites: site_stats(events, now),
    }
}

/// Approximate time per site: attribute the gap between consecutive browser
/// URL events to the earlier event's domain, capped at `MAX_SITE_SPAN_MS`.
fn site_stats(events: &[Event], now: i64) -> Vec<SiteStat> {
    let url_events: Vec<&Event> = events
        .iter()
        .filter(|e| e.source == "browser" && e.url.as_deref().is_some_and(|u| !u.is_empty()))
        .collect();

    let mut durations: HashMap<String, i64> = HashMap::new();
    for (i, e) in url_events.iter().enumerate() {
        let Some(domain) = e.url.as_deref().and_then(domain_of) else {
            continue;
        };
        let next_ts = url_events.get(i + 1).map(|n| n.ts).unwrap_or(now);
        let span = (next_ts - e.ts).clamp(0, MAX_SITE_SPAN_MS);
        *durations.entry(domain).or_insert(0) += span;
    }

    let mut sites: Vec<SiteStat> = durations
        .into_iter()
        .filter(|(_, ms)| *ms > 0)
        .map(|(domain, ms)| SiteStat { domain, ms })
        .collect();
    sites.sort_by(|a, b| b.ms.cmp(&a.ms));
    sites.truncate(8);
    sites
}

/// Host of an http(s) URL, minus a leading `www.`. Non-web schemes
/// (chrome://, about:, …) return `None` so they don't pollute the site list.
fn domain_of(u: &str) -> Option<String> {
    let parsed = url::Url::parse(u).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let host = parsed.host_str()?;
    Some(host.strip_prefix("www.").unwrap_or(host).to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: i64 = 60_000;

    fn ev(min: i64, kind: &str, source: &str, app: Option<&str>, url: Option<&str>, meta: Option<&str>) -> Event {
        Event {
            ts: min * MIN,
            kind: kind.into(),
            source: source.into(),
            app: app.map(String::from),
            url: url.map(String::from),
            meta: meta.map(String::from),
        }
    }

    #[test]
    fn domain_of_strips_www_and_skips_non_web() {
        assert_eq!(domain_of("https://www.reddit.com/r/rust"), Some("reddit.com".into()));
        assert_eq!(domain_of("http://Example.com:8080/x"), Some("example.com".into()));
        assert_eq!(domain_of("chrome://newtab/"), None);
        assert_eq!(domain_of("not a url"), None);
    }

    // The seeded day: 3 sessions (two ≥30m breaks), apps Chrome/Terminal/Slack/
    // Code, sites github + reddit. Asserts the full read aggregation.
    #[test]
    fn summarize_matches_seeded_day() {
        let chrome = Some("com.google.Chrome");
        let evs = vec![
            ev(0, "sensing_start", "os", None, None, None),
            ev(0, "active", "os", None, None, None),
            ev(0, "app_focus", "os", chrome, None, Some(r#"{"name":"Google Chrome"}"#)),
            ev(2, "tab_navigated", "browser", None, Some("https://github.com/anthropics/amdion"), None),
            ev(20, "tab_navigated", "browser", None, Some("https://github.com/notifications"), None),
            ev(30, "app_focus", "os", Some("com.apple.Terminal"), None, Some(r#"{"name":"Terminal"}"#)),
            ev(60, "idle", "os", None, None, Some(r#"{"idleSecs":0}"#)),
            ev(100, "active", "os", None, None, None),
            ev(100, "app_focus", "os", Some("com.tinyspeck.slackmacgap"), None, Some(r#"{"name":"Slack"}"#)),
            ev(108, "app_focus", "os", chrome, None, Some(r#"{"name":"Google Chrome"}"#)),
            ev(110, "tab_navigated", "browser", None, Some("https://www.reddit.com/r/rust"), None),
            ev(130, "idle", "os", None, None, Some(r#"{"idleSecs":0}"#)),
            ev(160, "active", "os", None, None, None),
            ev(160, "app_focus", "os", Some("com.microsoft.VSCode"), None, Some(r#"{"name":"Code"}"#)),
        ];
        // thresholds: break 5m, session gap 30m; read time = 180m
        let s = summarize(&evs, 180 * MIN, "2026-06-13".into(), 5 * MIN, 30 * MIN);

        assert_eq!(s.break_count, 2);
        assert_eq!(s.switch_count, 2);
        assert_eq!(s.active_ms, (60 + 30 + 20) * MIN, "active = sum of block durations");
        assert_eq!(s.time_on_computer_ms, 180 * MIN, "first event → now");

        // Chrome dominates (30m in block1 + 22m in block2); all four apps present.
        assert_eq!(s.apps[0].name, "Google Chrome");
        assert_eq!(s.apps[0].ms, 52 * MIN);
        let app_ms = |b: &str| s.apps.iter().find(|a| a.bundle == b).map(|a| a.ms);
        assert_eq!(app_ms("com.apple.Terminal"), Some(30 * MIN));
        assert_eq!(app_ms("com.tinyspeck.slackmacgap"), Some(8 * MIN));
        assert_eq!(app_ms("com.microsoft.VSCode"), Some(20 * MIN));

        // Sites: github (two spans, each capped at 15m) + reddit (capped 15m).
        let site_ms = |d: &str| s.sites.iter().find(|x| x.domain == d).map(|x| x.ms);
        assert_eq!(site_ms("github.com"), Some(2 * MAX_SITE_SPAN_MS));
        assert_eq!(site_ms("reddit.com"), Some(MAX_SITE_SPAN_MS));
    }
}
