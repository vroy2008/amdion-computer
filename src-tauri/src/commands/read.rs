// Read Mode commands.
//
// Tell the companion extension to enter/exit the in-page reader (content/
// reader.js) on the active tab. These just push the App→Ext `read_mode` control
// onto the bridge broadcast channel; the per-connection pump forwards it (a
// no-op if no extension is connected). Reading *preferences* ride the normal
// config path (save_config → read_prefs_message), not these commands.

use crate::state::AppState;

fn read_mode_message(on: bool) -> String {
    serde_json::json!({ "type": "read_mode", "payload": { "on": on } }).to_string()
}

/// "Read this tab": ask the extension to open the reader on the active tab.
#[tauri::command]
pub fn enter_read_mode(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let _ = state.bridge_tx.send(read_mode_message(true));
    Ok(())
}

/// Close the reader on the active tab.
#[tauri::command]
pub fn exit_read_mode(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let _ = state.bridge_tx.send(read_mode_message(false));
    Ok(())
}
