// Tab / navigation commands.
//
// NOTE (Step 0): the old embedded-webview implementation (Tauri `add_child` /
// show / hide / close / reposition) has been removed — Amdion no longer embeds a
// browser. These commands now only track UI state and emit `state-update`.
//
// TODO(Step 2): re-point `open_app` / `switch_tab` / `close_tab` / `go_home` to
// drive the user's REAL Chrome via the companion extension over the localhost
// WebSocket bridge (AppleScript fallback). See docs/IMPLEMENTATION_PLAN.md.

use crate::state::{AppState, AppStateData, TabInfo};
use serde::{Deserialize, Serialize};
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
        s.active_tab_id = Some(tab_id);
        s.is_home = false;
    }
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
