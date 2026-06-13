// Tab / navigation commands.
//
// These drive the user's REAL Chrome via the companion extension over the
// localhost WebSocket bridge (Step 2) — Amdion never embeds a browser. When the
// extension isn't connected, `open_app` falls back to AppleScript so opening a
// URL still works; tab focus/close have no AppleScript path and no-op.
//
// They also keep the small local "open apps" bookkeeping the panel uses and emit
// `state-update`, and they keep stable names/signatures so the future agent can
// call them as the typed action surface.

use crate::state::{AppState, AppStateData, TabInfo};
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;
use tauri::Emitter;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAppData {
    pub id: String,
    pub name: String,
    pub url: String,
}

fn emit_state(app: &tauri::AppHandle, state: &tauri::State<'_, AppState>) -> AppStateData {
    let data = state.data.lock().unwrap().clone();
    let _ = app.emit("state-update", &data);
    data
}

/// Push an App→Ext command to the connected extension(s). Returns `false` when
/// nothing is connected, so the caller can decide on a fallback.
fn push_to_ext(state: &tauri::State<'_, AppState>, json: String) -> bool {
    if state.ext_connections.load(Ordering::SeqCst) == 0 {
        return false;
    }
    state.bridge_tx.send(json).is_ok()
}

/// Open a URL in the user's Chrome via AppleScript — the no-extension fallback.
/// The URL is sanitized so it can't break out of the AppleScript string literal.
fn applescript_open(url: &str) {
    let safe = url.replace('\\', "").replace('"', "%22");
    let script = format!(
        "tell application \"Google Chrome\" to open location \"{safe}\""
    );
    let _ = std::process::Command::new("osascript")
        .args(["-e", &script])
        .status();
}

#[tauri::command]
pub fn open_app(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    app_data: OpenAppData,
) -> Result<AppStateData, String> {
    {
        let mut s = state.data.lock().unwrap();
        if !s.active_tabs.iter().any(|t| t.id == app_data.id) {
            s.active_tabs.push(TabInfo {
                id: app_data.id.clone(),
                name: app_data.name.clone(),
                url: app_data.url.clone(),
            });
        }
        s.active_tab_id = Some(app_data.id.clone());
        s.is_home = false;
    }
    // Drive the real Chrome: over the bridge when the extension is connected,
    // else via AppleScript so the URL still opens.
    let cmd = serde_json::json!({
        "type": "open_tab",
        "payload": { "url": app_data.url },
    });
    if !push_to_ext(&state, cmd.to_string()) {
        applescript_open(&app_data.url);
    }
    Ok(emit_state(&app, &state))
}

#[tauri::command]
pub fn switch_tab(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    tab_id: String,
) -> Result<(), String> {
    {
        let mut s = state.data.lock().unwrap();
        if !s.active_tabs.iter().any(|t| t.id == tab_id) {
            return Ok(());
        }
        s.active_tab_id = Some(tab_id.clone());
        s.is_home = false;
    }
    // TODO(Step 3): `tab_id` is Amdion's internal id; the extension keys on
    // Chrome's numeric tabId. Correct today only for tabs the extension itself
    // tracked — the id↔tabId map lands with the Step-3 event store. No-ops
    // gracefully when the extension is disconnected.
    let cmd = serde_json::json!({ "type": "focus_tab", "payload": { "tabId": tab_id } });
    push_to_ext(&state, cmd.to_string());
    emit_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn close_tab(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    tab_id: String,
) -> Result<(), String> {
    {
        let mut s = state.data.lock().unwrap();
        s.active_tabs.retain(|t| t.id != tab_id);
        if s.active_tab_id.as_deref() == Some(&tab_id) {
            if let Some(last) = s.active_tabs.last() {
                s.active_tab_id = Some(last.id.clone());
            } else {
                s.active_tab_id = None;
                s.is_home = true;
            }
        }
    }
    // TODO(Step 3): same id caveat as `switch_tab`. No-ops when disconnected.
    let cmd = serde_json::json!({ "type": "close_tab", "payload": { "tabId": tab_id } });
    push_to_ext(&state, cmd.to_string());
    emit_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn go_home(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    {
        let mut s = state.data.lock().unwrap();
        s.active_tab_id = None;
        s.is_home = true;
    }
    emit_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn toggle_sidebar(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let collapsed = {
        let mut s = state.data.lock().unwrap();
        s.sidebar_collapsed = !s.sidebar_collapsed;
        s.sidebar_collapsed
    };
    emit_state(&app, &state);
    Ok(serde_json::json!({ "collapsed": collapsed }))
}

#[tauri::command]
pub fn toggle_right_sidebar(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let hidden = {
        let mut s = state.data.lock().unwrap();
        s.right_sidebar_hidden = !s.right_sidebar_hidden;
        s.right_sidebar_hidden
    };
    emit_state(&app, &state);
    Ok(serde_json::json!({ "hidden": hidden }))
}

#[tauri::command]
pub fn get_state(state: tauri::State<'_, AppState>) -> Result<AppStateData, String> {
    Ok(state.data.lock().unwrap().clone())
}
