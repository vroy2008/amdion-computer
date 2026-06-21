// AI chat + audio transcription commands.
//
// The assistant is NOT part of V1. It is gated behind the off-by-default
// `assistant` Cargo feature: in the default (V1) build these commands are inert
// stubs and the Gemini client isn't compiled in, so the build makes no calls to
// Google. Build with `--features assistant` to enable it.

use tauri::Emitter;

#[cfg(feature = "assistant")]
use crate::config::read_config;
#[cfg(feature = "assistant")]
use crate::gemini::{call_gemini, call_gemini_with_image};

// ---- Enabled implementation (only with `--features assistant`) ----

#[cfg(feature = "assistant")]
#[tauri::command]
pub async fn send_chat_message(app: tauri::AppHandle, message: String) -> Result<(), String> {
    let config = read_config();
    if config.api_key.is_empty() {
        let _ = app.emit("chat-response", "API Key is missing. Please configure it in Settings.");
        return Ok(());
    }

    let prompt = format!(
        "You are Amdion, a minimalist attention assistant. The user is asking you: \"{}\". Be concise, direct, and helpful.",
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

#[cfg(feature = "assistant")]
#[tauri::command]
pub async fn transcribe_audio(base64_audio: String) -> Result<Option<String>, String> {
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

// ---- Inert stubs for the default V1 build (assistant disabled) ----

#[cfg(not(feature = "assistant"))]
#[tauri::command]
pub async fn send_chat_message(app: tauri::AppHandle, message: String) -> Result<(), String> {
    let _ = &message;
    let _ = app.emit("chat-response", "The assistant isn't enabled in this build.");
    Ok(())
}

#[cfg(not(feature = "assistant"))]
#[tauri::command]
pub async fn transcribe_audio(base64_audio: String) -> Result<Option<String>, String> {
    let _ = &base64_audio;
    Ok(None)
}
