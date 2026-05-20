use super::{exports, http, profile, prompts, shared};
use crate::state::AppState;
use marinara_core::AppError;
use serde_json::{json, Value};
use tauri::State;

#[tauri::command]
pub async fn load_url_binary(
    url: String,
    fallback_mime: Option<String>,
) -> Result<Value, AppError> {
    http::http_binary(
        &url,
        fallback_mime
            .as_deref()
            .unwrap_or("application/octet-stream"),
    )
    .await
}

#[tauri::command]
pub fn profile_export(state: State<'_, AppState>) -> Result<Value, AppError> {
    profile::profile_snapshot(&state)
}

#[tauri::command]
pub fn profile_import(state: State<'_, AppState>, envelope: Value) -> Result<Value, AppError> {
    profile::profile_call(
        &state,
        "POST",
        &["import"],
        &shared::ParsedPath::new("/profile/import"),
        envelope,
    )
}

#[tauri::command]
pub fn profile_import_file(state: State<'_, AppState>, path: String) -> Result<Value, AppError> {
    profile::import_profile_file_path(&state, &path)
}

#[tauri::command]
pub fn prompt_export(state: State<'_, AppState>, preset_id: String) -> Result<Value, AppError> {
    exports::export_prompt(&state, &preset_id)
}

#[tauri::command]
pub fn prompts_export_bulk(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> Result<Value, AppError> {
    exports::export_records(&state, "marinara_presets", "prompts", json!({ "ids": ids }))
}

#[tauri::command]
pub fn character_export(
    state: State<'_, AppState>,
    id: String,
    format: Option<String>,
) -> Result<Value, AppError> {
    exports::export_record(
        &state,
        "marinara_character",
        "characters",
        &id,
        format.as_deref(),
    )
}

#[tauri::command]
pub fn character_export_png(state: State<'_, AppState>, id: String) -> Result<Value, AppError> {
    exports::export_character_png(&state, &id)
}

#[tauri::command]
pub fn character_embedded_lorebook_import(
    state: State<'_, AppState>,
    id: String,
) -> Result<Value, AppError> {
    exports::import_character_embedded_lorebook(&state, &id)
}

#[tauri::command]
pub fn characters_export_bulk(
    state: State<'_, AppState>,
    ids: Vec<String>,
    format: Option<String>,
) -> Result<Value, AppError> {
    exports::export_records(
        &state,
        "marinara_characters",
        "characters",
        json!({ "ids": ids, "format": format }),
    )
}

#[tauri::command]
pub fn persona_export(
    state: State<'_, AppState>,
    id: String,
    format: Option<String>,
) -> Result<Value, AppError> {
    exports::export_record(
        &state,
        "marinara_persona",
        "personas",
        &id,
        format.as_deref(),
    )
}

#[tauri::command]
pub fn personas_export_bulk(
    state: State<'_, AppState>,
    ids: Vec<String>,
    format: Option<String>,
) -> Result<Value, AppError> {
    exports::export_records(
        &state,
        "marinara_personas",
        "personas",
        json!({ "ids": ids, "format": format }),
    )
}

#[tauri::command]
pub fn lorebook_export(
    state: State<'_, AppState>,
    id: String,
    format: Option<String>,
) -> Result<Value, AppError> {
    exports::export_lorebook(&state, &id, format.as_deref())
}

#[tauri::command]
pub fn lorebooks_export_bulk(
    state: State<'_, AppState>,
    ids: Vec<String>,
    format: Option<String>,
) -> Result<Value, AppError> {
    exports::export_lorebooks(&state, json!({ "ids": ids, "format": format }))
}

#[tauri::command]
pub async fn lorebook_vectorize(
    state: State<'_, AppState>,
    id: String,
    body: Value,
) -> Result<Value, AppError> {
    prompts::vectorize_lorebook(&state, &id, body).await
}
