// Amdion Notes — the read/manage half of the Attention layer's capture store.
//
// Captures themselves arrive FROM the Chrome extension over the bridge
// (`note_captured` → `bridge_ws::persist_note`, the reliable inbound path);
// these commands let the panel browse, search, render, and delete them. Like the
// Observer, this is a typed surface the future agent can call to compile notes
// into a digest.
//
// Screenshots are PNG/JPEG files under `app-data/notes/`; the DB stores only the
// relative path. `get_note_image` reads one back as a `data:` URL (the panel CSP
// allows `img-src data:`), so we don't need an asset-protocol scope entry.

use crate::config::app_data_dir;
use crate::db::{Db, Note};
use base64::Engine;

/// Newest-first notes for the panel's Notes view (capped).
#[tauri::command]
pub fn list_notes(db: tauri::State<'_, Db>, limit: Option<i64>) -> Result<Vec<Note>, String> {
    Ok(db.list_notes(limit.unwrap_or(200).clamp(1, 1000)))
}

/// Notes matching a query (body / title / url substring). Empty query → recent.
#[tauri::command]
pub fn search_notes(
    db: tauri::State<'_, Db>,
    q: String,
    limit: Option<i64>,
) -> Result<Vec<Note>, String> {
    let limit = limit.unwrap_or(200).clamp(1, 1000);
    let q = q.trim();
    Ok(if q.is_empty() {
        db.list_notes(limit)
    } else {
        db.search_notes(q, limit)
    })
}

/// One note's screenshot as a `data:` URL, or `None` if it has no image. Read
/// lazily by the panel when rendering a card, so the list query stays light.
#[tauri::command]
pub fn get_note_image(db: tauri::State<'_, Db>, id: i64) -> Result<Option<String>, String> {
    let Some(rel) = db.note_image_path(id) else {
        return Ok(None);
    };
    let abs = app_data_dir().join(&rel);
    let bytes = std::fs::read(&abs).map_err(|e| e.to_string())?;
    let mime = if rel.ends_with(".jpg") || rel.ends_with(".jpeg") {
        "image/jpeg"
    } else {
        "image/png"
    };
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(Some(format!("data:{mime};base64,{b64}")))
}

/// Delete one note and its screenshot file (best-effort on the file).
#[tauri::command]
pub fn delete_note(db: tauri::State<'_, Db>, id: i64) -> Result<(), String> {
    if let Some(rel) = db.delete_note(id) {
        let _ = std::fs::remove_file(app_data_dir().join(rel));
    }
    Ok(())
}
