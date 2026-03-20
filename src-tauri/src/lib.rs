use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{Emitter, Manager};

// ============================================================
// State
// ============================================================

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
    pub scanning: Mutex<bool>,
    pub agent_running: Mutex<bool>,
    pub journal_recording: Mutex<bool>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            data: Mutex::new(AppStateData::default()),
            scanning: Mutex::new(false),
            agent_running: Mutex::new(false),
            journal_recording: Mutex::new(false),
        }
    }
}

// ============================================================
// Config types
// ============================================================

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

fn read_config() -> AppConfig {
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

fn write_config(config: &AppConfig) {
    let path = config_path();
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let _ = fs::write(path, json);
    }
}

// ============================================================
// Journal types & helpers
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub timestamp: String,
    pub time: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

fn journals_dir() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = exe_dir.join("journals");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn today_journal_path() -> PathBuf {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    journals_dir().join(format!("{}.json", today))
}

fn read_journal(path: &PathBuf) -> Vec<serde_json::Value> {
    if path.exists() {
        if let Ok(data) = fs::read_to_string(path) {
            if let Ok(entries) = serde_json::from_str::<Vec<serde_json::Value>>(&data) {
                return entries;
            }
        }
    }
    Vec::new()
}

fn append_journal_entry(
    entry_type: &str,
    extra: serde_json::Value,
    app: &tauri::AppHandle,
) -> Option<serde_json::Value> {
    let path = today_journal_path();
    let mut entries = read_journal(&path);

    let now = chrono::Local::now();
    let mut entry = serde_json::json!({
        "timestamp": now.to_rfc3339(),
        "time": now.format("%I:%M %p").to_string(),
        "type": entry_type,
    });

    if let serde_json::Value::Object(map) = extra {
        if let serde_json::Value::Object(ref mut obj) = entry {
            for (k, v) in map {
                obj.insert(k, v);
            }
        }
    }

    entries.push(entry.clone());
    if let Ok(json) = serde_json::to_string_pretty(&entries) {
        let _ = fs::write(path, json);
    }

    let _ = app.emit("journal-update", &entry);
    Some(entry)
}

// ============================================================
// Gemini API client
// ============================================================

#[derive(Debug, Serialize, Deserialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum GeminiPart {
    Text { text: String },
    InlineData { #[serde(rename = "inlineData")] inline_data: InlineData },
}

#[derive(Debug, Serialize, Deserialize)]
struct InlineData {
    #[serde(rename = "mimeType")]
    mime_type: String,
    data: String,
}

#[derive(Debug, Deserialize)]
struct GeminiResponse {
    candidates: Option<Vec<GeminiCandidate>>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: Option<GeminiCandidateContent>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidateContent {
    parts: Option<Vec<GeminiResponsePart>>,
}

#[derive(Debug, Deserialize)]
struct GeminiResponsePart {
    text: Option<String>,
}

async fn call_gemini(api_key: &str, model: &str, prompt: &str) -> Result<String, String> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, api_key
    );

    let request = GeminiRequest {
        contents: vec![GeminiContent {
            role: "user".into(),
            parts: vec![GeminiPart::Text { text: prompt.into() }],
        }],
    };

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    let body: GeminiResponse = resp
        .json()
        .await
        .map_err(|e| format!("Parse failed: {}", e))?;

    body.candidates
        .and_then(|c| c.into_iter().next())
        .and_then(|c| c.content)
        .and_then(|c| c.parts)
        .and_then(|p| p.into_iter().next())
        .and_then(|p| p.text)
        .ok_or_else(|| "Empty response from Gemini".into())
}

async fn call_gemini_with_image(
    api_key: &str,
    model: &str,
    prompt: &str,
    image_base64: &str,
    mime_type: &str,
) -> Result<String, String> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, api_key
    );

    let request = GeminiRequest {
        contents: vec![GeminiContent {
            role: "user".into(),
            parts: vec![
                GeminiPart::Text { text: prompt.into() },
                GeminiPart::InlineData {
                    inline_data: InlineData {
                        mime_type: mime_type.into(),
                        data: image_base64.into(),
                    },
                },
            ],
        }],
    };

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    let body: GeminiResponse = resp
        .json()
        .await
        .map_err(|e| format!("Parse failed: {}", e))?;

    body.candidates
        .and_then(|c| c.into_iter().next())
        .and_then(|c| c.content)
        .and_then(|c| c.parts)
        .and_then(|p| p.into_iter().next())
        .and_then(|p| p.text)
        .ok_or_else(|| "Empty response from Gemini".into())
}

fn clean_json_response(text: &str) -> &str {
    let mut s = text.trim();
    if s.starts_with("```json") {
        s = &s[7..];
    } else if s.starts_with("```") {
        s = &s[3..];
    }
    if s.ends_with("```") {
        s = &s[..s.len() - 3];
    }
    s.trim()
}

// ============================================================
// Tauri Commands — Window / Tab Management
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAppData {
    pub id: String,
    pub name: String,
    pub url: String,
}

#[tauri::command]
async fn open_app(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    app_data: OpenAppData,
) -> Result<AppStateData, String> {
    {
        let mut s = state.data.lock().unwrap();
        if !s.active_tabs.iter().any(|t| t.id == app_data.id) {
            s.active_tabs.push(TabInfo {
                id: app_data.id.clone(),
                name: app_data.name.clone(),
                url: app_data.url.clone(),
            });
        }
        s.active_tab_id = Some(app_data.id.clone());
        s.is_home = false;
    }

    // Open in a new webview window
    let label = format!("app_{}", app_data.id);
    let url: tauri::Url = app_data.url.parse::<tauri::Url>().map_err(|e| e.to_string())?;

    // Close existing window with same label if any
    if let Some(existing) = app.get_webview_window(&label) {
        let _ = existing.close();
    }

    let _ = tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::External(url))
        .title(&app_data.name)
        .inner_size(1200.0, 800.0)
        .decorations(true)
        .build()
        .map_err(|e| e.to_string())?;

    let s = state.data.lock().unwrap();
    let data = s.clone();

    // Emit state update to main window
    let _ = app.emit("state-update", &data);

    Ok(data)
}

#[tauri::command]
fn switch_tab(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    tab_id: String,
) -> Result<(), String> {
    {
        let mut s = state.data.lock().unwrap();
        if s.active_tabs.iter().any(|t| t.id == tab_id) {
            s.active_tab_id = Some(tab_id.clone());
            s.is_home = false;
        }
    }

    // Focus the corresponding window
    let label = format!("app_{}", tab_id);
    if let Some(win) = app.get_webview_window(&label) {
        let _ = win.set_focus();
    }

    let s = state.data.lock().unwrap();
    let _ = app.emit("state-update", &*s);
    Ok(())
}

#[tauri::command]
fn close_tab(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    tab_id: String,
) -> Result<(), String> {
    // Close the window
    let label = format!("app_{}", tab_id);
    if let Some(win) = app.get_webview_window(&label) {
        let _ = win.close();
    }

    {
        let mut s = state.data.lock().unwrap();
        s.active_tabs.retain(|t| t.id != tab_id);
        if s.active_tab_id.as_deref() == Some(&tab_id) {
            if let Some(last) = s.active_tabs.last() {
                s.active_tab_id = Some(last.id.clone());
            } else {
                s.active_tab_id = None;
                s.is_home = true;
            }
        }
    }

    let s = state.data.lock().unwrap();
    let _ = app.emit("state-update", &*s);
    Ok(())
}

#[tauri::command]
fn go_home(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    {
        let mut s = state.data.lock().unwrap();
        s.active_tab_id = None;
        s.is_home = true;
    }
    let s = state.data.lock().unwrap();
    let _ = app.emit("state-update", &*s);
    Ok(())
}

#[tauri::command]
fn toggle_sidebar(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let collapsed;
    {
        let mut s = state.data.lock().unwrap();
        s.sidebar_collapsed = !s.sidebar_collapsed;
        collapsed = s.sidebar_collapsed;
    }
    let s = state.data.lock().unwrap();
    let _ = app.emit("state-update", &*s);
    Ok(serde_json::json!({ "collapsed": collapsed }))
}

#[tauri::command]
fn toggle_right_sidebar(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let hidden;
    {
        let mut s = state.data.lock().unwrap();
        s.right_sidebar_hidden = !s.right_sidebar_hidden;
        hidden = s.right_sidebar_hidden;
    }
    let s = state.data.lock().unwrap();
    let _ = app.emit("state-update", &*s);
    Ok(serde_json::json!({ "hidden": hidden }))
}

#[tauri::command]
fn get_state(state: tauri::State<'_, AppState>) -> Result<AppStateData, String> {
    let s = state.data.lock().unwrap();
    Ok(s.clone())
}

// ============================================================
// Tauri Commands — Config
// ============================================================

#[tauri::command]
fn get_config() -> Result<AppConfig, String> {
    Ok(read_config())
}

#[derive(Debug, Deserialize)]
pub struct ConfigUpdate {
    #[serde(rename = "apiKey")]
    pub api_key: Option<String>,
    pub model: Option<String>,
}

#[tauri::command]
fn save_config(config: ConfigUpdate) -> Result<(), String> {
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
fn get_favorites() -> Result<Vec<FavoriteApp>, String> {
    let config = read_config();
    Ok(config.favorites)
}

#[derive(Debug, Deserialize)]
pub struct AddFavoriteData {
    pub name: String,
    pub url: String,
}

#[tauri::command]
fn add_favorite(app_data: AddFavoriteData) -> Result<Vec<FavoriteApp>, String> {
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

// ============================================================
// Tauri Commands — AI Chat & Scanning
// ============================================================

#[tauri::command]
async fn send_chat_message(app: tauri::AppHandle, message: String) -> Result<(), String> {
    let config = read_config();
    if config.api_key.is_empty() {
        let _ = app.emit("chat-response", "API Key is missing. Please configure it in Settings.");
        return Ok(());
    }

    let prompt = format!(
        "You are Amdion, a minimalist focus assistant. The user is asking you: \"{}\". Be concise, direct, and helpful.",
        message
    );

    tokio::spawn(async move {
        match call_gemini(&config.api_key, &config.model, &prompt).await {
            Ok(reply) => {
                let _ = app.emit("chat-response", reply.trim());
            }
            Err(e) => {
                let _ = app.emit("chat-response", format!("Sorry, I encountered an error: {}", e));
            }
        }
    });

    Ok(())
}

#[tauri::command]
fn set_loop_state(
    state: tauri::State<'_, AppState>,
    #[allow(unused)] state_val: bool,
) -> Result<(), String> {
    let mut scanning = state.scanning.lock().unwrap();
    *scanning = state_val;
    // Background scanning would require screen capture - noted for future
    Ok(())
}

#[tauri::command]
fn trigger_manual_scan() -> Result<(), String> {
    // Screen capture + Gemini analysis - noted for future with xcap crate
    Ok(())
}

// ============================================================
// Tauri Commands — Agent
// ============================================================

#[tauri::command]
async fn send_agent_action(app: tauri::AppHandle, task: String) -> Result<(), String> {
    let config = read_config();
    if config.api_key.is_empty() {
        let _ = app.emit("agent-update", serde_json::json!({
            "type": "error",
            "message": "API Key is missing. Please configure it in Settings."
        }));
        let _ = app.emit("agent-update", serde_json::json!({ "type": "finished" }));
        return Ok(());
    }

    let _ = app.emit("agent-update", serde_json::json!({
        "type": "start",
        "message": format!("Starting task: \"{}\"", task)
    }));

    // Agent loop with screen capture would go here
    // For now, explain that agent needs screen capture capability
    let _ = app.emit("agent-update", serde_json::json!({
        "type": "done",
        "message": "Agent actions require screen capture, which is being implemented for Tauri. Use chat mode for now."
    }));
    let _ = app.emit("agent-update", serde_json::json!({ "type": "finished" }));

    Ok(())
}

#[tauri::command]
fn stop_agent(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut running = state.agent_running.lock().unwrap();
    *running = false;
    Ok(())
}

// ============================================================
// Tauri Commands — Journal
// ============================================================

#[tauri::command]
fn set_journal_state(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    #[allow(unused)] state_val: bool,
) -> Result<(), String> {
    let mut recording = state.journal_recording.lock().unwrap();
    *recording = state_val;
    if state_val {
        append_journal_entry("journal_start", serde_json::json!({"summary": "Recording started"}), &app);
    } else {
        append_journal_entry("journal_stop", serde_json::json!({"summary": "Recording stopped"}), &app);
    }
    Ok(())
}

#[tauri::command]
fn get_journal() -> Result<Vec<serde_json::Value>, String> {
    Ok(read_journal(&today_journal_path()))
}

#[tauri::command]
fn get_journal_dates() -> Result<Vec<String>, String> {
    let dir = journals_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut dates: Vec<String> = fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".json") {
                Some(name.trim_end_matches(".json").to_string())
            } else {
                None
            }
        })
        .collect();
    dates.sort();
    dates.reverse();
    Ok(dates)
}

#[tauri::command]
fn get_journal_by_date(date: String) -> Result<Vec<serde_json::Value>, String> {
    let path = journals_dir().join(format!("{}.json", date));
    Ok(read_journal(&path))
}

#[tauri::command]
async fn get_journal_graph(date: String) -> Result<serde_json::Value, String> {
    let path = journals_dir().join(format!("{}.json", date));
    let entries = read_journal(&path);
    if entries.is_empty() {
        return Ok(serde_json::json!({"nodes": [], "edges": []}));
    }

    let config = read_config();
    if config.api_key.is_empty() {
        return Ok(serde_json::json!({"nodes": [], "edges": []}));
    }

    let summaries: Vec<String> = entries.iter().map(|e| {
        let time = e.get("time").and_then(|v| v.as_str()).unwrap_or("");
        let etype = e.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let summary = e.get("summary").or(e.get("message")).or(e.get("task"))
            .and_then(|v| v.as_str()).unwrap_or("");
        format!("[{}] ({}) {}", time, etype, summary)
    }).collect();

    let prompt = format!(
        r#"Analyze these daily activity journal entries and extract a knowledge graph of key topics, apps, and activities.

Journal entries:
{}

Return ONLY valid JSON in this exact format:
{{"nodes": [{{"id": "spotify", "label": "Spotify", "type": "app"}}], "edges": [{{"source": "spotify", "target": "music", "label": "listening"}}]}}

Node types: "app", "topic", "action", "person"
Rules: Extract 5-15 nodes maximum, connect related nodes, use short labels. Return ONLY the JSON."#,
        summaries.join("\n")
    );

    match call_gemini(&config.api_key, &config.model, &prompt).await {
        Ok(text) => {
            let cleaned = clean_json_response(&text);
            serde_json::from_str(cleaned)
                .map_err(|e| format!("Failed to parse graph: {}", e))
        }
        Err(e) => Err(e),
    }
}

#[tauri::command]
async fn transcribe_audio(base64_audio: String) -> Result<Option<String>, String> {
    let config = read_config();
    if config.api_key.is_empty() {
        return Ok(None);
    }

    match call_gemini_with_image(
        &config.api_key,
        &config.model,
        "Transcribe the following audio to text. Return ONLY the transcribed text, nothing else. If the audio is empty or unintelligible, return an empty string.",
        &base64_audio,
        "audio/webm",
    ).await {
        Ok(text) => Ok(Some(text.trim().to_string())),
        Err(_) => Ok(None),
    }
}

// ============================================================
// App entry point
// ============================================================

pub fn run() {
    let _ = dotenvy::dotenv();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            // Window/Tab
            open_app,
            switch_tab,
            close_tab,
            go_home,
            toggle_sidebar,
            toggle_right_sidebar,
            get_state,
            // Config
            get_config,
            save_config,
            get_favorites,
            add_favorite,
            // AI
            send_chat_message,
            set_loop_state,
            trigger_manual_scan,
            // Agent
            send_agent_action,
            stop_agent,
            // Journal
            set_journal_state,
            get_journal,
            get_journal_dates,
            get_journal_by_date,
            get_journal_graph,
            transcribe_audio,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
