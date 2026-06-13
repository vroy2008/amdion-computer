// Shared application state.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabInfo {
    pub id: String,
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppStateData {
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
}

impl Default for AppStateData {
    fn default() -> Self {
        Self {
            active_tabs: Vec::new(),
            active_tab_id: None,
            is_home: true,
            sidebar_collapsed: false,
            right_sidebar_hidden: false,
        }
    }
}

pub struct AppState {
    pub data: Mutex<AppStateData>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            data: Mutex::new(AppStateData::default()),
        }
    }
}
