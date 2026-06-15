// Read Mode commands.
//
// Tell the companion extension to enter/exit the in-page reader (content/
// reader.js) on the active tab. These just push the App→Ext `read_mode` control
// onto the bridge broadcast channel; the per-connection pump forwards it (a
// no-op if no extension is connected). Reading *preferences* ride the normal
// config path (save_config → read_prefs_message), not these commands.

use crate::config::read_config;
use crate::state::AppState;

fn read_mode_message(on: bool) -> String {
    serde_json::json!({ "type": "read_mode", "payload": { "on": on } }).to_string()
}

/// Optional "auto-Focus" half of the wrap: when a read starts or ends, run the
/// user's configured macOS Shortcut (e.g. one that toggles a Focus). Opt-in —
/// does nothing unless the relevant name is set. Called from the bridge's
/// `route_event` on every `read_started` / `read_ended`.
///
/// Spawned on a detached thread so the WS read loop never blocks on the
/// `shortcuts` CLI; the result is logged, not surfaced (a missing/renamed
/// Shortcut just no-ops). `shortcuts run` is non-interactive and ships on
/// macOS 12+.
pub fn on_read_boundary(started: bool) {
    let r = read_config().reading;
    let name = if started { r.focus_shortcut_start } else { r.focus_shortcut_end };
    let name = name.trim().to_string();
    if name.is_empty() {
        return;
    }
    std::thread::spawn(move || {
        match std::process::Command::new("shortcuts")
            .args(["run", &name])
            .status()
        {
            Ok(s) if s.success() => {}
            Ok(s) => eprintln!("[read] shortcut '{name}' exited with {s}"),
            Err(e) => eprintln!("[read] couldn't run shortcut '{name}': {e}"),
        }
    });
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
