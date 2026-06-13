// Shared application state.

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabInfo {
    pub id: String,
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppStateData {
    // ── Opened apps (state-only in Step 0/1; Step 2 drives real Chrome) ──
    #[serde(rename = "activeTabs")]
    pub active_tabs: Vec<TabInfo>,
    #[serde(rename = "activeTabId")]
    pub active_tab_id: Option<String>,
    #[serde(rename = "isHome")]
    pub is_home: bool,
    #[serde(rename = "sidebarCollapsed")]
    pub sidebar_collapsed: bool,
    #[serde(rename = "rightSidebarHidden")]
    pub right_sidebar_hidden: bool,

    // ── Front door (Step 1) ──
    /// The user's stated intent for the current session, if any.
    pub intent: Option<String>,
    /// Epoch millis when this Amdion session (process) started. The greeting
    /// shows time-on-computer relative to this until the sensing engine
    /// (Step 3) provides a real login time.
    #[serde(rename = "sessionStartMs")]
    pub session_start_ms: i64,

    // ── Chrome bridge (Step 2) ──
    /// Whether the companion Chrome extension is currently connected over the
    /// localhost WebSocket bridge. Surfaced to the panel via `state-update`.
    #[serde(rename = "extensionConnected", default)]
    pub extension_connected: bool,
}

impl Default for AppStateData {
    fn default() -> Self {
        Self {
            active_tabs: Vec::new(),
            active_tab_id: None,
            is_home: true,
            sidebar_collapsed: false,
            right_sidebar_hidden: false,
            intent: None,
            session_start_ms: chrono::Utc::now().timestamp_millis(),
            extension_connected: false,
        }
    }
}

pub struct AppState {
    pub data: Mutex<AppStateData>,
    /// When the panel last auto-hid on blur. Used to distinguish a tray click
    /// that should *close* the panel (it just blurred shut) from one that
    /// should *open* it. See the tray handler in `lib.rs`.
    pub last_hide: Mutex<Option<Instant>>,

    // ── Chrome bridge (Step 2) ──
    /// App→extension command channel. Synchronous `#[tauri::command]` fns push
    /// already-serialized JSON here with a single non-blocking `send`; the WS
    /// server's per-connection pump forwards each frame. Broadcast fans out to
    /// every connected socket and auto-prunes ones whose receiver has dropped.
    pub bridge_tx: broadcast::Sender<String>,
    /// Live count of connected extensions — a cheap "is Chrome wired up?" check
    /// that decides whether `open_app` goes over the bridge or falls back to
    /// AppleScript.
    pub ext_connections: Arc<AtomicUsize>,
    /// One-time shared secret written to `bridge.json` and required in the
    /// extension's `hello`. Plumbed now as the hardening hook for the future
    /// Web-Store build (the unpacked dev build authenticates by pinned origin).
    pub bridge_token: String,
}

impl Default for AppState {
    fn default() -> Self {
        // Small buffer; control messages are re-derivable from `read_config`, so
        // a lagging consumer dropping the oldest frame is harmless.
        let (bridge_tx, _rx) = broadcast::channel(64);
        Self {
            data: Mutex::new(AppStateData::default()),
            last_hide: Mutex::new(None),
            bridge_tx,
            ext_connections: Arc::new(AtomicUsize::new(0)),
            bridge_token: gen_token(),
        }
    }
}

/// A 32-char hex token for the bridge handshake.
fn gen_token() -> String {
    let mut rng = rand::thread_rng();
    (0..32)
        .map(|_| char::from_digit(rng.gen_range(0..16), 16).unwrap())
        .collect()
}
