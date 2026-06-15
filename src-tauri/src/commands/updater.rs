//! In-app auto-update.
//!
//! Amdion is a menu-bar app with no Dock presence, so "quit and re-download the
//! DMG" is a poor update path. Instead it checks GitHub Releases for a newer
//! *signed* build and installs it in place. Signatures are verified against the
//! minisign `pubkey` in `tauri.conf.json` before anything is written, so a
//! compromised release host can't push a malicious build.
//!
//! The newer version is staged on disk and applied on the next launch — we
//! never relaunch out from under the user mid-session.

use serde::Serialize;
use tauri_plugin_updater::UpdaterExt;

#[derive(Serialize)]
pub struct UpdateStatus {
    /// A newer build was found (and, on success, downloaded + staged).
    pub available: bool,
    /// The version that was staged, if any.
    pub version: Option<String>,
    /// The version currently running.
    pub current: String,
}

/// Check for a newer signed build and, if found, download + install it. Applied
/// on the next launch. Shared by the startup check and the manual command.
pub async fn check_and_install(app: &tauri::AppHandle) -> Result<UpdateStatus, String> {
    let current = app.package_info().version.to_string();
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await.map_err(|e| e.to_string())? {
        Some(update) => {
            let version = update.version.clone();
            update
                .download_and_install(|_chunk, _total| {}, || {})
                .await
                .map_err(|e| e.to_string())?;
            Ok(UpdateStatus {
                available: true,
                version: Some(version),
                current,
            })
        }
        None => Ok(UpdateStatus {
            available: false,
            version: None,
            current,
        }),
    }
}

/// Manual "Check for updates" entry point for the UI. Returns what happened so
/// the panel can show "You're up to date" or "Update ready — relaunch to apply".
#[tauri::command]
pub async fn check_for_updates(app: tauri::AppHandle) -> Result<UpdateStatus, String> {
    check_and_install(&app).await
}

/// Relaunch the app so a staged update is applied. Used by the "Relaunch now"
/// button the UI shows after a successful `check_for_updates`.
#[tauri::command]
pub fn relaunch_app(app: tauri::AppHandle) {
    app.restart();
}
