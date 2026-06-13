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
    let _ = win.set_size(LogicalSize::new(ONBOARD_W, ONBOARD_H));
    let _ = win.center();
    let _ = win.show();
    let _ = win.set_focus();
}

/// Shrink the window back to the panel size, dock it at the top-right under the
/// menu bar, and hide it — the "retreat into the menu bar" landing.
pub fn retreat_window(win: &tauri::WebviewWindow) {
    let _ = win.set_size(LogicalSize::new(PANEL_W, PANEL_H));
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

/// Record (or clear, with `None`) the user's stated intent for this session.
#[tauri::command]
pub fn set_intent(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    intent: Option<String>,
) -> Result<AppStateData, String> {
    {
        let mut s = state.data.lock().unwrap();
        s.intent = intent.filter(|i| !i.trim().is_empty());
    }
    Ok(emit_state(&app, &state))
}

/// Hide the panel (Escape, or after finishing onboarding). The menu-bar icon
/// or ⌘⇧Space brings it back.
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
