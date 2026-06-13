// Sensing engine (Step 3, macOS).
//
// A background thread polls two permission-light OS signals every ~10s and
// appends *transition* events (not a row per tick) to the SQLite store, so the
// classifier ([[crate::classify]]) can reconstruct the session/block/break
// timeline from durations:
//
//   - whole-machine idle seconds via CoreGraphics
//     (`CGEventSourceSecondsSinceLastEventType`) — no event tap, no permission.
//   - the frontmost app's bundle id + display name via `NSWorkspace`
//     (no Accessibility; window *titles* would need it, so they're deferred).
//
// Events emitted (`source = "os"`): `sensing_start` once at launch, `active`
// when the user returns, `idle` (carrying `idleSecs` so the break start can be
// back-dated) when they've been away past `break_threshold_mins`, `app_focus`
// on a frontmost-app change, and `shutdown` on a clean exit (so a crash is
// distinguishable from a quit — see the classifier). A `sensing-update` Tauri
// event mirrors the live state to the panel, emitted only when it changes.

#[cfg(target_os = "macos")]
mod imp {
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
        let mut was_active = idle_seconds() < break_secs();
        let mut last_app: Option<String> = None;
        let mut last_sig: Option<(bool, String)> = None;

        while running.load(Ordering::SeqCst) {
            let idle = idle_seconds();
            let now_active = idle < break_secs();

            // active ⇄ break transition
            if now_active != was_active {
                if now_active {
                    insert(&app, "active", None, None);
                    last_app = None; // force a fresh app_focus on resume
                } else {
                    let meta = serde_json::json!({ "idleSecs": idle.round() as i64 }).to_string();
                    insert(&app, "idle", None, Some(&meta));
                }
                was_active = now_active;
            }

            // frontmost-app change, tracked only while active
            let mut cur_app: Option<String> = None;
            if now_active {
                if let Some((bundle, name)) = frontmost_app() {
                    cur_app = Some(bundle.clone());
                    if last_app.as_deref() != Some(bundle.as_str()) {
                        let meta = serde_json::json!({ "name": name }).to_string();
                        insert(&app, "app_focus", Some(&bundle), Some(&meta));
                        last_app = Some(bundle);
                    }
                }
            }

            // live indicator: emit only when (active, app) changes
            let sig = (now_active, cur_app.clone().unwrap_or_default());
            if last_sig.as_ref() != Some(&sig) {
                let _ = app.emit(
                    "sensing-update",
                    SensingUpdate {
                        state: if now_active { "active" } else { "break" },
                        app: cur_app,
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
