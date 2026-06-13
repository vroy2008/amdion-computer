// Config: types and JSON file persistence.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(rename = "apiKey", default)]
    pub api_key: String,
    #[serde(default = "default_model")]
    pub model: String,

    // ── Behavior (Step 1) ──
    /// "off" | "soft" | "lockin". Wired to the extension in Step 2.
    #[serde(rename = "frictionLevel", default = "default_friction")]
    pub friction_level: String,
    /// Offer to record an intent at the start of each new session.
    #[serde(rename = "greetingEnabled", default = "default_true")]
    pub greeting_enabled: bool,
    /// Set once the first-run onboarding flow has been completed.
    #[serde(rename = "onboardingComplete", default)]
    pub onboarding_complete: bool,
    /// Idle minutes that count as a break (advanced; used by Step 3 sensing).
    #[serde(rename = "breakThresholdMins", default = "default_break_threshold")]
    pub break_threshold_mins: u32,
    /// Idle minutes that close a session (advanced; used by Step 3 sensing).
    #[serde(rename = "sessionGapMins", default = "default_session_gap")]
    pub session_gap_mins: u32,

    // ── Response / friction (Step 2) ──
    /// User-added domains the friction layer acts on, on top of the extension's
    /// built-in distraction list. Soft = nudge; Lock-In = block/redirect.
    #[serde(rename = "blockList", default)]
    pub block_list: Vec<String>,
}

fn default_model() -> String {
    "gemini-3.1-flash-lite-preview".to_string()
}

fn default_friction() -> String {
    "soft".to_string()
}

fn default_true() -> bool {
    true
}

fn default_break_threshold() -> u32 {
    5
}

fn default_session_gap() -> u32 {
    30
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: default_model(),
            friction_level: default_friction(),
            greeting_enabled: true,
            onboarding_complete: false,
            break_threshold_mins: default_break_threshold(),
            session_gap_mins: default_session_gap(),
            block_list: Vec::new(),
        }
    }
}

/// macOS bundle identifier — must match `tauri.conf.json`.
const APP_IDENTIFIER: &str = "com.amdion.desktop";

/// Amdion's per-user data directory:
/// `~/Library/Application Support/com.amdion.desktop`, created on first use.
///
/// Config and tuning snapshots live here, not next to the executable: once
/// Amdion is installed in `/Applications` its bundle is read-only, so the old
/// exe-adjacent path silently failed to persist. Shared with `tuning.rs`.
pub fn app_data_dir() -> PathBuf {
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| {
            home.join("Library")
                .join("Application Support")
                .join(APP_IDENTIFIER)
        })
        .unwrap_or_else(|| PathBuf::from("."));
    let _ = fs::create_dir_all(&base);
    base
}

fn config_path() -> PathBuf {
    app_data_dir().join("config.json")
}

pub fn read_config() -> AppConfig {
    let path = config_path();

    // Start from the saved file when present (serde `default`s fill any new
    // fields a stored config predates; unknown old fields are ignored),
    // otherwise from built-in defaults.
    let mut config = path
        .exists()
        .then(|| fs::read_to_string(&path).ok())
        .flatten()
        .and_then(|data| serde_json::from_str::<AppConfig>(&data).ok())
        .unwrap_or_default();

    // An env-provided API key wins (handy for dev without touching the file).
    if let Ok(key) = std::env::var("GEMINI_API_KEY") {
        if !key.is_empty() {
            config.api_key = key;
        }
    }
    config
}

pub fn write_config(config: &AppConfig) {
    let path = config_path();
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let _ = fs::write(path, json);
    }
}
