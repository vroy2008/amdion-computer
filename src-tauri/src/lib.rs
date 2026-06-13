// Amdion — Tauri app setup and command registration.

mod commands;
mod config;
mod gemini;
mod state;

use state::AppState;

pub fn run() {
    let _ = dotenvy::dotenv();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            // Tab / navigation (state-only in Step 0; Step 2 drives real Chrome)
            commands::browser::open_app,
            commands::browser::switch_tab,
            commands::browser::close_tab,
            commands::browser::go_home,
            commands::browser::toggle_sidebar,
            commands::browser::toggle_right_sidebar,
            commands::browser::get_state,
            // Config + favorites
            commands::config::get_config,
            commands::config::save_config,
            commands::config::get_favorites,
            commands::config::add_favorite,
            // AI chat + transcription
            commands::chat::send_chat_message,
            commands::chat::transcribe_audio,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
