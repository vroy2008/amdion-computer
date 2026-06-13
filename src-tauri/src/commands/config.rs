// Config commands.

use crate::config::{read_config, write_config, AppConfig};
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
}

/// Partial update: only the fields present in `config` are changed.
#[tauri::command]
pub fn save_config(config: ConfigUpdate) -> Result<AppConfig, String> {
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
    write_config(&current);
    Ok(current)
}
