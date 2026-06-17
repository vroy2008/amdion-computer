// Attention layer — the panel-side triggers for the "one attention surface":
// Present (fullscreen + lock the wrap over any tab) and Capture (snapshot the
// active tab). These just push App→Ext control messages onto the bridge
// broadcast channel; the extension does the work (background.js), so they no-op
// gracefully when no extension is connected. Read Mode is the article-shaped
// face of the same surface (see commands/read.rs); capture from inside the
// reader and from selection chips arrives back over the bridge as `note_captured`.

use crate::state::AppState;

/// "Capture this tab": ask the extension to snapshot the active tab's viewport
/// (chrome.tabs.captureVisibleTab) and file it as a screenshot note. Works on
/// any tab — including Chrome's built-in PDF viewer, which content scripts can't
/// reach — because it captures rendered pixels, not the DOM.
#[tauri::command]
pub fn capture_tab(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let _ = state
        .bridge_tx
        .send(serde_json::json!({ "type": "capture_tab", "payload": {} }).to_string());
    Ok(())
}

/// "Focus this tab": flip the active Chrome window to fullscreen and raise the
/// distraction lock (the wrap) — the content-agnostic Present face for PDFs,
/// dashboards, video, anything Read Mode can't reformat. `on:false` releases it.
#[tauri::command]
pub fn present_mode(state: tauri::State<'_, AppState>, on: bool) -> Result<(), String> {
    let _ = state
        .bridge_tx
        .send(serde_json::json!({ "type": "present_mode", "payload": { "on": on } }).to_string());
    Ok(())
}

/// Open a note's source URL back in Chrome (over the bridge; no-op if the
/// extension is down). Mirrors the `open_tab` path browser.rs already drives.
#[tauri::command]
pub fn open_source(state: tauri::State<'_, AppState>, url: String) -> Result<(), String> {
    let _ = state
        .bridge_tx
        .send(serde_json::json!({ "type": "open_tab", "payload": { "url": url } }).to_string());
    Ok(())
}
