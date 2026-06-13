// Config + favorites commands.

use crate::config::{read_config, write_config, AppConfig, FavoriteApp};
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
}

#[tauri::command]
pub fn save_config(config: ConfigUpdate) -> Result<(), String> {
    let mut current = read_config();
    if let Some(key) = config.api_key {
        current.api_key = key;
    }
    if let Some(model) = config.model {
        current.model = model;
    }
    write_config(&current);
    Ok(())
}

#[tauri::command]
pub fn get_favorites() -> Result<Vec<FavoriteApp>, String> {
    Ok(read_config().favorites)
}

#[derive(Debug, Deserialize)]
pub struct AddFavoriteData {
    pub name: String,
    pub url: String,
}

#[tauri::command]
pub fn add_favorite(app_data: AddFavoriteData) -> Result<Vec<FavoriteApp>, String> {
    let mut config = read_config();
    let new_id = chrono::Utc::now().timestamp_millis().to_string();
    config.favorites.push(FavoriteApp {
        id: new_id,
        name: app_data.name,
        url: app_data.url,
    });
    write_config(&config);
    Ok(config.favorites)
}
