// Sensing engine (Step 3, macOS).
//
// A background thread polls permission-light OS signals every ~10s and appends
// *transition* events (not a row per tick) to the SQLite store, so the
// classifier ([[crate::classify]]) can reconstruct the session/block/break
// timeline from durations.
//
// This measures ATTENTION, not "computer powered on". The user is counted
// present only when all three hold:
//
//   - recent input: whole-machine idle seconds via CoreGraphics
//     (`CGEventSourceSecondsSinceLastEventType`) below the break threshold —
//     no event tap, no permission.
//   - screen unlocked: the frontmost app (via `NSWorkspace`) is NOT the lock
//     screen / login window or the password screensaver. While locked, macOS
//     reports `com.apple.loginwindow` as frontmost — counting that as activity
//     is exactly the "loginwindow tracked for hours" bug this avoids.
//   - awake: the poll thread freezes during system sleep, so a wall-clock jump
//     between polls means the machine slept — that whole stretch was away.
//
// `NSWorkspace` also gives the frontmost app's display name (no Accessibility;
// window *titles* would need it, so they're deferred).
//
// Events emitted (`source = "os"`): `sensing_start` once at launch, `active`
// when the user returns, `idle` (carrying `idleSecs` so the break start can be
// back-dated) when they walk away past `break_threshold_mins` OR after a sleep,
// `locked` when the screen locks / saver starts (a HARD session boundary, like
// the browser `idle_state:locked`), `app_focus` on a frontmost-app change
// (never for an away process), and `shutdown` on a clean exit (so a crash is
// distinguishable from a quit — see the classifier). A `sensing-update` Tauri
// event mirrors the live state to the panel, emitted only when it changes.

#[cfg(target_os = "macos")]
mod imp {
    use crate::classify::is_away_bundle;
    use crate::config::read_config;
    use crate::db::Db;
    use serde::Serialize;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;
    use tauri::{AppHandle, Emitter, Manager};

    /// Seconds between polls. Transition events make this cheap regardless.
    const POLL_SECS: u64 = 10;

    /// A gap between polls this much larger than `POLL_SECS` means the thread was
    /// frozen — the machine slept (or was suspended). 60s ≈ six missed polls,
    /// well clear of scheduler jitter / timer coalescing, so it flags real sleep
    /// without minting phantom breaks. The stretch is back-dated as away.
    const SLEEP_GAP_MS: i64 = 60_000;

    // CoreGraphics: seconds since the last HID input event, whole-machine. No
    // crate exposes this (core-graphics 0.25 has no idle API) and it needs no
    // permission. `kCGEventSourceStateHIDSystemState` = 1, `kCGAnyInputEventType`
    // = 0xFFFFFFFF.
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventSourceSecondsSinceLastEventType(state_id: u32, event_type: u32) -> f64;
    }
    const HID_SYSTEM_STATE: u32 = 1;
    const ANY_INPUT_EVENT: u32 = u32::MAX;

    fn idle_seconds() -> f64 {
        unsafe { CGEventSourceSecondsSinceLastEventType(HID_SYSTEM_STATE, ANY_INPUT_EVENT) }
    }

    /// Frontmost app's `(bundle_id, display_name)`. `NSWorkspace` is not
    /// `MainThreadOnly` and `NSRunningApplication` is `Send + Sync`, so reading
    /// from this background thread is sound (only momentary staleness, which a
    /// 10s cadence makes irrelevant). Returns `None` during transient nil (e.g.
    /// an app switch in flight, or an app with no bundle id).
    fn frontmost_app() -> Option<(String, String)> {
        use objc2_app_kit::NSWorkspace;
        let ws = NSWorkspace::sharedWorkspace();
        let app = ws.frontmostApplication()?;
        let bundle = app.bundleIdentifier()?.to_string();
        let name = app
            .localizedName()
            .map(|s| s.to_string())
            .unwrap_or_else(|| bundle.clone());
        Some((bundle, name))
    }

    #[derive(Serialize, Clone)]
    struct SensingUpdate {
        state: &'static str,
        app: Option<String>,
        #[serde(rename = "idleSecs")]
        idle_secs: i64,
    }

    /// Handle so the exit hook can stop the thread cleanly and write a single
    /// `shutdown` marker. `Thread` + `Arc<AtomicBool>` are `Send + Sync`.
    pub struct SensingHandle {
        running: Arc<AtomicBool>,
        thread: thread::Thread,
    }

    fn insert(app: &AppHandle, kind: &str, bundle: Option<&str>, meta: Option<&str>) {
        if let Some(db) = app.try_state::<Db>() {
            db.insert_event(kind, "os", bundle, None, meta);
        }
    }

    /// Spawn the polling thread and register the `SensingHandle` for `on_exit`.
    pub fn start(app: &AppHandle) {
        let running = Arc::new(AtomicBool::new(true));
        let flag = running.clone();
        let app_thread = app.clone();
        let join = thread::Builder::new()
            .name("amdion-sensing".into())
            .spawn(move || run_loop(app_thread, flag))
            .expect("spawn sensing thread");
        app.manage(SensingHandle { running, thread: join.thread().clone() });
    }

    /// On a clean exit: flip the flag, wake the parked thread, and write one
    /// `shutdown` event (idempotent — `swap` guards against ExitRequested +
    /// Exit both firing).
    pub fn on_exit(app: &AppHandle) {
        if let Some(h) = app.try_state::<SensingHandle>() {
            if h.running.swap(false, Ordering::SeqCst) {
                h.thread.unpark();
                insert(app, "shutdown", None, None);
            }
        }
    }

    fn run_loop(app: AppHandle, running: Arc<AtomicBool>) {
        insert(&app, "sensing_start", None, None);

        let break_secs = || (read_config().break_threshold_mins.max(1) as f64) * 60.0;
        let now_ms = || chrono::Utc::now().timestamp_millis();

        // present = recent input AND screen unlocked AND awake. Seed from the
        // idle counter (a fresh launch is a present user at an unlocked screen).
        let mut was_present = idle_seconds() < break_secs();
        let mut was_locked = false;
        let mut last_app: Option<String> = None;
        let mut last_sig: Option<(bool, String)> = None;
        let mut last_wall = now_ms();

        while running.load(Ordering::SeqCst) {
            // Sleep detection: the poll thread freezes during system sleep, so a
            // gap far larger than the cadence means the machine slept. That whole
            // stretch was away — close the open block back at the last poll
            // (back-dated via `idleSecs`) before reading fresh, post-wake signals.
            let wall = now_ms();
            let slept_ms = wall - last_wall;
            last_wall = wall;
            if slept_ms > SLEEP_GAP_MS && was_present {
                let meta = serde_json::json!({ "idleSecs": (slept_ms / 1000).max(0) }).to_string();
                insert(&app, "idle", None, Some(&meta));
                was_present = false;
                last_app = None;
            }

            let idle = idle_seconds();
            let front = frontmost_app();
            // Screen locked / saver up ⇒ no human present, whatever the idle says.
            let locked_now = front.as_ref().map(|(b, _)| is_away_bundle(b)).unwrap_or(false);

            // Lock is a HARD boundary (ends the session). Emit once on entry so
            // the classifier opens a fresh session when the user unlocks.
            if locked_now && !was_locked {
                insert(&app, "locked", None, None);
                was_present = false;
                last_app = None;
            }
            was_locked = locked_now;

            let present_now = !locked_now && idle < break_secs();

            // present ⇄ away transition (the lock path above already closed the
            // block, so here `idle` only covers a plain walk-away, not a lock)
            if present_now != was_present {
                if present_now {
                    insert(&app, "active", None, None);
                    last_app = None; // force a fresh app_focus on resume
                } else if !locked_now {
                    let meta = serde_json::json!({ "idleSecs": idle.round() as i64 }).to_string();
                    insert(&app, "idle", None, Some(&meta));
                }
                was_present = present_now;
            }

            // frontmost-app change, tracked only while present — so the lock
            // screen / screensaver (never present) is never attributed app time.
            let mut cur_bundle: Option<String> = None;
            let mut cur_name: Option<String> = None;
            if present_now {
                if let Some((bundle, name)) = &front {
                    cur_bundle = Some(bundle.clone());
                    cur_name = Some(name.clone());
                    if last_app.as_deref() != Some(bundle.as_str()) {
                        let meta = serde_json::json!({ "name": name }).to_string();
                        insert(&app, "app_focus", Some(bundle), Some(&meta));
                        last_app = Some(bundle.clone());
                    }
                }
            }

            // live indicator: emit only when (present, app) changes. `app` carries
            // the display name for the panel, not the bundle id.
            let sig = (present_now, cur_bundle.unwrap_or_default());
            if last_sig.as_ref() != Some(&sig) {
                let _ = app.emit(
                    "sensing-update",
                    SensingUpdate {
                        state: if present_now { "active" } else { "break" },
                        app: cur_name,
                        idle_secs: idle.round() as i64,
                    },
                );
                last_sig = Some(sig);
            }

            thread::park_timeout(Duration::from_secs(POLL_SECS));
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use tauri::AppHandle;
    pub fn start(_app: &AppHandle) {}
    pub fn on_exit(_app: &AppHandle) {}
}

pub use imp::{on_exit, start};
