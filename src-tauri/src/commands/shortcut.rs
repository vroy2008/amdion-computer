// Hotkey commands: rebind the global "summon the panel" shortcut at runtime.

use crate::config::{read_config, write_config};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

/// Rebind the global summon shortcut to `accelerator` (Tauri global-shortcut
/// syntax, e.g. "Alt+Space"). The replacement is registered first; only once it
/// takes do we retire the old binding and persist the change — so a bad combo
/// (bad syntax, or one another app already owns) leaves the working shortcut
/// untouched. Returns the saved accelerator on success.
#[tauri::command]
pub fn set_summon_shortcut(app: tauri::AppHandle, accelerator: String) -> Result<String, String> {
    let next = accelerator.trim().to_string();
    if next.is_empty() {
        return Err("Pick a shortcut first.".into());
    }

    let current = read_config().summon_shortcut;
    if next == current {
        return Ok(next); // nothing to do
    }

    let shortcuts = app.global_shortcut();

    // Register the replacement before touching the old one. If it won't bind,
    // bail without disturbing the shortcut that's currently working.
    shortcuts.register(next.as_str()).map_err(|_| {
        "That combination is taken (bad keys, or another app owns it). Try another.".to_string()
    })?;

    // The new one is live — drop the previous binding (ignore if it was never
    // registered, e.g. it had fallen back to the default at launch).
    let _ = shortcuts.unregister(current.as_str());

    let mut config = read_config();
    config.summon_shortcut = next.clone();
    write_config(&config);
    Ok(next)
}
