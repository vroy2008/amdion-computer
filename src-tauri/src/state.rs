// Shared application state.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::Instant;

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
        }
    }
}

pub struct AppState {
    pub data: Mutex<AppStateData>,
    /// When the panel last auto-hid on blur. Used to distinguish a tray click
    /// that should *close* the panel (it just blurred shut) from one that
    /// should *open* it. See the tray handler in `lib.rs`.
    pub last_hide: Mutex<Option<Instant>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            data: Mutex::new(AppStateData::default()),
            last_hide: Mutex::new(None),
        }
    }
}
