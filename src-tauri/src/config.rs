// Config + favorites: types and JSON file persistence.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FavoriteApp {
    pub id: String,
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(rename = "apiKey", default)]
    pub api_key: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default)]
    pub favorites: Vec<FavoriteApp>,
}

fn default_model() -> String {
    "gemini-3.1-flash-lite-preview".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: default_model(),
            favorites: vec![
                FavoriteApp { id: "1".into(), name: "Notion".into(), url: "https://notion.so".into() },
                FavoriteApp { id: "2".into(), name: "Spotify".into(), url: "https://open.spotify.com".into() },
                FavoriteApp { id: "3".into(), name: "Antigravity".into(), url: "https://antigravity.com".into() },
                FavoriteApp { id: "4".into(), name: "Drive".into(), url: "https://drive.google.com".into() },
            ],
        }
    }
}

fn config_path() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    exe_dir.join("config.json")
}

pub fn read_config() -> AppConfig {
    let path = config_path();
    let mut config = AppConfig::default();

    // Check env var for API key
    if let Ok(key) = std::env::var("GEMINI_API_KEY") {
        if !key.is_empty() {
            config.api_key = key;
        }
    }

    if path.exists() {
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(saved) = serde_json::from_str::<AppConfig>(&data) {
                if !saved.api_key.is_empty() {
                    config.api_key = saved.api_key;
                }
                if !saved.model.is_empty() {
                    config.model = saved.model;
                }
                if !saved.favorites.is_empty() {
                    config.favorites = saved.favorites;
                }
            }
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
