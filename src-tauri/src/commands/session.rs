// Front-door session commands: stated intent, panel show/hide, and the
// onboarding expand→retreat transition that teaches "Amdion lives up here."

use crate::state::{AppState, AppStateData};
use tauri::{Emitter, LogicalSize, Manager, PhysicalPosition};

/// Panel size when docked as the menu-bar dropdown (logical points).
const PANEL_W: f64 = 420.0;
const PANEL_H: f64 = 560.0;

fn emit_state(app: &tauri::AppHandle, state: &tauri::State<'_, AppState>) -> AppStateData {
    let data = state.data.lock().unwrap().clone();
    let _ = app.emit("state-update", &data);
    data
}

/// Onboarding window size (logical points) — a prominent, centered card, not a
/// full-screen takeover.
const ONBOARD_W: f64 = 600.0;
const ONBOARD_H: f64 = 640.0;

/// Grow the window to the centered onboarding card and show it — prominent the
/// first time, before it retreats into the menu bar on finish.
pub fn expand_window(win: &tauri::WebviewWindow) {
    // The window ships non-resizable (a fixed menu-bar panel). A non-resizable
    // window can clamp a programmatic grow, so lift the flag for the centered
    // onboarding card and restore it on retreat. No resize grips show either way
    // — the window is borderless (decorations: false).
    let _ = win.set_resizable(true);
    let _ = win.set_size(LogicalSize::new(ONBOARD_W, ONBOARD_H));
    let _ = win.center();
    let _ = win.show();
    let _ = win.set_focus();
}

/// Shrink the window back to the panel size, dock it at the top-right under the
/// menu bar, and hide it — the "retreat into the menu bar" landing.
pub fn retreat_window(win: &tauri::WebviewWindow) {
    let _ = win.set_size(LogicalSize::new(PANEL_W, PANEL_H));
    let _ = win.set_resizable(false);
    if let Some(m) = win
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| win.primary_monitor().ok().flatten())
    {
        let scale = win.scale_factor().unwrap_or(1.0);
        let pos = *m.position();
        let size = *m.size();
        let w_phys = (PANEL_W * scale) as i32;
        let margin = (12.0 * scale) as i32;
        let top = (32.0 * scale) as i32;
        let x = pos.x + size.width as i32 - w_phys - margin;
        let y = pos.y + top;
        let _ = win.set_position(PhysicalPosition::new(x, y));
    }
    let _ = win.hide();
}

/// Called when the panel is summoned (tray click / ⌃⇧A / tray menu). Decides
/// whether this arrival begins a NEW session — reusing the classifier's own
/// session boundary (an idle gap ≥ `sessionGap`, a hard lock, or the first
/// arrival of the day) — and if so logs a single `session_start` event, so the
/// record carries a real "I sat down" timestamp. Emits `panel-summoned
/// { newSession }` either way: the front door greets on a genuine arrival and
/// stays quiet on a re-summon mid-session.
///
/// Permissionless and quiet: the boundary is derived from the sensing already
/// running and the summon is an arrival the user makes themselves — no popup, no
/// notification, no new permission.
pub fn on_panel_summoned(app: &tauri::AppHandle) {
    let Some(db) = app.try_state::<crate::db::Db>() else {
        return;
    };
    let now = chrono::Utc::now().timestamp_millis();
    let session_start = crate::commands::observer::current_session_start(db.inner()).unwrap_or(now);
    let new_session = !db.has_event_since("session_start", session_start);
    if new_session {
        db.insert_event("session_start", "app", None, None, None);
    }
    let _ = app.emit("panel-summoned", serde_json::json!({ "newSession": new_session }));
}

/// The 3 built-in intents and the mode each maps onto (V1.md §3.2). Used as the
/// fallback when a preset is set without an explicit mode; a custom intent always
/// carries its own mode (chosen in the panel).
fn preset_mode(intent: &str) -> &'static str {
    match intent.trim().to_lowercase().as_str() {
        "deep work" => "lockin",
        "communication" => "soft",
        "exploration" => "off",
        _ => "soft", // unknown / custom without a mode → the gentle default
    }
}

/// Resolve the mode for an intent: the explicit `mode` when valid, else the preset
/// mapping. `None` when there's no intent — the extension then tracks only.
fn resolve_mode(intent: Option<&str>, mode: Option<&str>) -> Option<String> {
    let intent = intent?;
    if let Some(m) = mode {
        if matches!(m, "off" | "soft" | "lockin") {
            return Some(m.to_string());
        }
    }
    Some(preset_mode(intent).to_string())
}

/// Record (or clear, with `None`) the user's stated intent — and the mode it maps
/// onto — for this session, then push the intent → mode contract to the extension.
#[tauri::command]
pub fn set_intent(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    intent: Option<String>,
    mode: Option<String>,
) -> Result<AppStateData, String> {
    let cleaned = intent.filter(|i| !i.trim().is_empty());
    let resolved = resolve_mode(cleaned.as_deref(), mode.as_deref());
    {
        let mut s = state.data.lock().unwrap();
        s.intent = cleaned.clone();
        s.intent_mode = resolved.clone();
    }
    // Log a real, jotted intent to the on-device record — the longitudinal "what
    // I set out to do" trail for Reflect and the future agent. Clearing the
    // intent (`None`) isn't an event; only a set one is.
    if let Some(text) = cleaned.as_deref() {
        if let Some(db) = app.try_state::<crate::db::Db>() {
            let meta = serde_json::json!({ "intent": text, "mode": resolved.as_deref() }).to_string();
            db.insert_event("session_intent", "app", None, None, Some(&meta));
        }
    }
    // Push to the extension: the `intent` copy (so in-page nudge copy can adapt —
    // "You're here for X — is HOST part of that?") and the `intent_mode` contract.
    // A pick/clear is an explicit user action, so it ALWAYS re-asserts the intent
    // default, clearing any manual override (assert:true). No-op if nothing's
    // connected.
    let _ = state
        .bridge_tx
        .send(crate::bridge_ws::intent_message(cleaned.as_deref()));
    let level = resolved.as_deref().unwrap_or("off");
    let token = crate::bridge_ws::session_token(&app);
    let _ = state.bridge_tx.send(crate::bridge_ws::intent_mode_message(
        level,
        token,
        cleaned.as_deref(),
        true,
    ));
    Ok(emit_state(&app, &state))
}

/// Hide the panel (Escape, or after finishing onboarding). The menu-bar icon
/// or ⌃⇧A brings it back.
#[tauri::command]
pub fn hide_panel(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.hide();
    }
    Ok(())
}

#[tauri::command]
pub fn expand_for_onboarding(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        expand_window(&win);
    }
    Ok(())
}

#[tauri::command]
pub fn retreat_to_menubar(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        retreat_window(&win);
    }
    Ok(())
}
