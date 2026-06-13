// Config commands.

use crate::bridge_ws::friction_message;
use crate::config::{read_config, write_config, AppConfig};
use crate::state::AppState;
use serde::Deserialize;

#[tauri::command]
pub fn get_config() -> Result<AppConfig, String> {
    Ok(read_config())
}

#[derive(Debug, Deserialize)]
pub struct ConfigUpdate {
    #[serde(rename = "apiKey")]
    pub api_key: Option<String>,
    pub model: Option<String>,
    #[serde(rename = "frictionLevel")]
    pub friction_level: Option<String>,
    #[serde(rename = "greetingEnabled")]
    pub greeting_enabled: Option<bool>,
    #[serde(rename = "onboardingComplete")]
    pub onboarding_complete: Option<bool>,
    #[serde(rename = "breakThresholdMins")]
    pub break_threshold_mins: Option<u32>,
    #[serde(rename = "sessionGapMins")]
    pub session_gap_mins: Option<u32>,
    #[serde(rename = "blockList")]
    pub block_list: Option<Vec<String>>,
}

/// Partial update: only the fields present in `config` are changed. After
/// writing, the current friction config is pushed to the connected extension so
/// Chrome's behavior tracks the settings immediately (no-op if nothing's
/// connected). `state` is injected by Tauri — the frontend call is unchanged.
#[tauri::command]
pub fn save_config(
    state: tauri::State<'_, AppState>,
    config: ConfigUpdate,
) -> Result<AppConfig, String> {
    let mut current = read_config();
    if let Some(key) = config.api_key {
        current.api_key = key;
    }
    if let Some(model) = config.model {
        current.model = model;
    }
    if let Some(level) = config.friction_level {
        current.friction_level = level;
    }
    if let Some(greeting) = config.greeting_enabled {
        current.greeting_enabled = greeting;
    }
    if let Some(done) = config.onboarding_complete {
        current.onboarding_complete = done;
    }
    if let Some(mins) = config.break_threshold_mins {
        current.break_threshold_mins = mins;
    }
    if let Some(mins) = config.session_gap_mins {
        current.session_gap_mins = mins;
    }
    if let Some(list) = config.block_list {
        current.block_list = list;
    }
    write_config(&current);
    let _ = state.bridge_tx.send(friction_message());
    Ok(current)
}
