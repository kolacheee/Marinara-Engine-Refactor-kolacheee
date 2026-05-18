use crate::state::AppState;
use base64::{engine::general_purpose, Engine as _};
use marinara_core::{ensure_object, new_id, now_iso, now_millis, AppError, AppResult};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::fs;
use std::time::Duration;
use tauri::State;

#[path = "storage/admin.rs"]
mod admin;
#[path = "storage/agents.rs"]
mod agents;
#[path = "storage/avatars.rs"]
mod avatars;
#[path = "storage/backgrounds.rs"]
mod backgrounds;
#[path = "storage/bot_browser.rs"]
mod bot_browser;
#[path = "storage/characters.rs"]
mod characters;
#[path = "storage/chat_presets.rs"]
mod chat_presets;
#[path = "storage/chats.rs"]
mod chats;
#[path = "storage/custom_tools.rs"]
mod custom_tools;
#[path = "storage/exports.rs"]
mod exports;
#[path = "storage/fonts.rs"]
mod fonts;
#[path = "storage/game_assets.rs"]
mod game_assets;
#[path = "storage/generation.rs"]
mod generation;
#[path = "storage/http.rs"]
mod http;
#[path = "storage/images.rs"]
mod images;
#[path = "storage/imports.rs"]
mod imports;
#[path = "storage/integrations.rs"]
mod integrations;
#[path = "storage/knowledge.rs"]
mod knowledge;
#[path = "storage/llm.rs"]
mod llm;
#[path = "storage/lorebook_images.rs"]
mod lorebook_images;
#[path = "storage/media_uploads.rs"]
mod media_uploads;
#[path = "storage/prompts.rs"]
mod prompts;
#[path = "storage/profile.rs"]
mod profile;
#[path = "storage/router.rs"]
mod router;
#[path = "storage/shared.rs"]
mod shared;
#[path = "storage/sprites.rs"]
mod sprites;
#[path = "storage/translation.rs"]
mod translation;

use tauri::ipc::Channel;

#[tauri::command]
pub async fn api_request(
    state: State<'_, AppState>,
    method: String,
    path: String,
    body: Option<Value>,
) -> Result<Value, AppError> {
    router::route_request(
        &state,
        &method.to_uppercase(),
        &path,
        body.unwrap_or(Value::Null),
    )
    .await
}

#[tauri::command]
pub async fn api_stream_events(
    state: State<'_, AppState>,
    path: String,
    body: Option<Value>,
) -> Result<Vec<Value>, AppError> {
    router::stream_events(&state, path, body).await
}

#[tauri::command]
pub async fn api_stream_channel(
    state: State<'_, AppState>,
    path: String,
    body: Option<Value>,
    on_event: Channel<Value>,
) -> Result<(), AppError> {
    router::stream_events_channel(&state, path, body, on_event).await
}

#[tauri::command]
pub async fn load_url_binary(url: String, fallback_mime: Option<String>) -> Result<Value, AppError> {
    http::http_binary(&url, fallback_mime.as_deref().unwrap_or("application/octet-stream")).await
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
pub fn game_assets_list(state: State<'_, AppState>, path: Option<String>) -> Result<Value, AppError> {
    Ok(json!({
        "items": state.game_assets.list(path.as_deref())?,
        "root": state.game_assets.root().to_string_lossy()
    }))
}

#[tauri::command]
pub fn game_assets_tree(state: State<'_, AppState>) -> Result<Value, AppError> {
    game_assets::game_assets_tree(&state)
}

#[tauri::command]
pub fn game_assets_rescan(state: State<'_, AppState>) -> Result<Value, AppError> {
    game_assets::game_assets_rescan(&state)
}

#[tauri::command]
pub fn game_assets_create_folder(state: State<'_, AppState>, path: String) -> Result<Value, AppError> {
    state.game_assets.create_folder(&path)?;
    Ok(json!({ "path": path }))
}

#[tauri::command]
pub fn game_assets_delete_folder(
    state: State<'_, AppState>,
    path: String,
    recursive: Option<bool>,
) -> Result<Value, AppError> {
    state.game_assets.remove(&path, recursive.unwrap_or(false))?;
    Ok(json!({ "deleted": true }))
}

#[tauri::command]
pub fn game_assets_delete_file(state: State<'_, AppState>, path: String) -> Result<Value, AppError> {
    state.game_assets.remove(&path, false)?;
    Ok(json!({ "deleted": true }))
}

#[tauri::command]
pub fn game_assets_file_path(state: State<'_, AppState>, path: String) -> Result<Value, AppError> {
    Ok(json!({ "path": state.game_assets.absolute_path_string(&path)? }))
}

#[tauri::command]
pub fn game_assets_read_text(state: State<'_, AppState>, path: String) -> Result<Value, AppError> {
    Ok(json!({ "content": state.game_assets.read_text(&path)? }))
}

#[tauri::command]
pub fn game_assets_write_text(
    state: State<'_, AppState>,
    path: String,
    content: String,
) -> Result<Value, AppError> {
    state.game_assets.write_text(&path, &content)?;
    Ok(json!({ "saved": true }))
}

#[tauri::command]
pub fn game_assets_rename(
    state: State<'_, AppState>,
    path: String,
    new_name: String,
) -> Result<Value, AppError> {
    state.game_assets.rename(&path, &new_name)
}

#[tauri::command]
pub fn game_assets_move(
    state: State<'_, AppState>,
    path: String,
    target_folder: Option<String>,
) -> Result<Value, AppError> {
    state
        .game_assets
        .move_to_folder(&path, target_folder.as_deref().unwrap_or(""))
}

#[tauri::command]
pub fn game_assets_copy(
    state: State<'_, AppState>,
    path: String,
    target_folder: Option<String>,
) -> Result<Value, AppError> {
    state
        .game_assets
        .copy_to_folder(&path, target_folder.as_deref().unwrap_or(""))
}

#[tauri::command]
pub fn game_assets_move_bulk(
    state: State<'_, AppState>,
    paths: Vec<String>,
    target_folder: Option<String>,
) -> Result<Value, AppError> {
    Ok(state
        .game_assets
        .move_many(&paths, target_folder.as_deref().unwrap_or("")))
}

#[tauri::command]
pub fn game_assets_copy_bulk(
    state: State<'_, AppState>,
    paths: Vec<String>,
    target_folder: Option<String>,
) -> Result<Value, AppError> {
    Ok(state
        .game_assets
        .copy_many(&paths, target_folder.as_deref().unwrap_or("")))
}

#[tauri::command]
pub fn game_assets_delete_bulk(state: State<'_, AppState>, paths: Vec<String>) -> Result<Value, AppError> {
    Ok(state.game_assets.delete_many(&paths))
}

#[tauri::command]
pub fn game_assets_file_info(state: State<'_, AppState>, path: String) -> Result<Value, AppError> {
    state.game_assets.file_info(&path)
}

#[tauri::command]
pub fn game_assets_folder_description(
    state: State<'_, AppState>,
    path: String,
    description: String,
) -> Result<Value, AppError> {
    game_assets::game_assets_folder_description(&state, json!({ "path": path, "description": description }))
}

#[tauri::command]
pub fn game_assets_upload(state: State<'_, AppState>, body: Value) -> Result<Value, AppError> {
    game_assets::game_assets_upload(&state, body)
}

#[tauri::command]
pub fn game_assets_open_folder(
    state: State<'_, AppState>,
    subfolder: Option<String>,
) -> Result<Value, AppError> {
    game_assets::game_assets_open_folder(&state, json!({ "subfolder": subfolder }))
}

#[tauri::command]
pub fn background_file_path(state: State<'_, AppState>, filename: String) -> Result<Value, AppError> {
    Ok(json!({ "path": state.backgrounds.absolute_path_string(&filename)? }))
}

#[tauri::command]
pub fn lorebook_image_file_path(
    state: State<'_, AppState>,
    filename: String,
) -> Result<Value, AppError> {
    lorebook_images::lorebook_image_file_path(&state, &filename)
}

#[tauri::command]
pub async fn gif_search(q: Option<String>, limit: Option<u32>, pos: Option<String>) -> Result<Value, AppError> {
    let mut query = HashMap::new();
    if let Some(q) = q {
        query.insert("q".to_string(), q);
    }
    if let Some(limit) = limit {
        query.insert("limit".to_string(), limit.to_string());
    }
    if let Some(pos) = pos {
        query.insert("pos".to_string(), pos);
    }
    http::gifs_search(&shared::ParsedPath { parts: Vec::new(), query }).await
}

#[tauri::command]
pub async fn tts_config(state: State<'_, AppState>) -> Result<Value, AppError> {
    integrations::tts_call(&state, "GET", &["config"], Value::Null).await
}

#[tauri::command]
pub async fn tts_update_config(state: State<'_, AppState>, config: Value) -> Result<Value, AppError> {
    integrations::tts_call(&state, "PUT", &["config"], config).await
}

#[tauri::command]
pub async fn tts_voices(state: State<'_, AppState>) -> Result<Value, AppError> {
    integrations::tts_call(&state, "GET", &["voices"], Value::Null).await
}

#[tauri::command]
pub async fn tts_speak(state: State<'_, AppState>, input: Value) -> Result<Value, AppError> {
    integrations::tts_call(&state, "POST", &["speak"], input).await
}

#[tauri::command]
pub async fn haptic_status() -> Result<Value, AppError> {
    integrations::haptic_call(&["status"], Value::Null).await
}

#[tauri::command]
pub async fn haptic_connect(body: Option<Value>) -> Result<Value, AppError> {
    integrations::haptic_call(&["connect"], body.unwrap_or(Value::Null)).await
}

#[tauri::command]
pub async fn haptic_disconnect() -> Result<Value, AppError> {
    integrations::haptic_call(&["disconnect"], Value::Null).await
}

#[tauri::command]
pub async fn haptic_start_scan() -> Result<Value, AppError> {
    integrations::haptic_call(&["scan", "start"], Value::Null).await
}

#[tauri::command]
pub async fn haptic_stop_scan() -> Result<Value, AppError> {
    integrations::haptic_call(&["scan", "stop"], Value::Null).await
}

#[tauri::command]
pub async fn haptic_command(command: Value) -> Result<Value, AppError> {
    integrations::haptic_call(&["command"], command).await
}

#[tauri::command]
pub async fn haptic_stop_all() -> Result<Value, AppError> {
    integrations::haptic_call(&["stop-all"], Value::Null).await
}

async fn spotify_direct(
    state: State<'_, AppState>,
    method: &str,
    rest: &[&str],
    body: Value,
) -> Result<Value, AppError> {
    integrations::spotify_call(
        &state,
        method,
        rest,
        &shared::ParsedPath::new("/spotify"),
        body,
    )
    .await
}

#[tauri::command]
pub async fn spotify_status(state: State<'_, AppState>, body: Option<Value>) -> Result<Value, AppError> {
    spotify_direct(state, "POST", &["status"], body.unwrap_or(Value::Null)).await
}

#[tauri::command]
pub async fn spotify_authorize(state: State<'_, AppState>, input: Value) -> Result<Value, AppError> {
    spotify_direct(state, "POST", &["authorize"], input).await
}

#[tauri::command]
pub async fn spotify_exchange(state: State<'_, AppState>, callback_url: String) -> Result<Value, AppError> {
    spotify_direct(state, "POST", &["exchange"], json!({ "callbackUrl": callback_url })).await
}

#[tauri::command]
pub async fn spotify_disconnect(state: State<'_, AppState>, body: Option<Value>) -> Result<Value, AppError> {
    spotify_direct(state, "POST", &["disconnect"], body.unwrap_or(Value::Null)).await
}

#[tauri::command]
pub async fn spotify_player(state: State<'_, AppState>, body: Option<Value>) -> Result<Value, AppError> {
    spotify_direct(state, "GET", &["player"], body.unwrap_or(Value::Null)).await
}

#[tauri::command]
pub async fn spotify_devices(state: State<'_, AppState>, body: Option<Value>) -> Result<Value, AppError> {
    spotify_direct(state, "GET", &["devices"], body.unwrap_or(Value::Null)).await
}

#[tauri::command]
pub async fn spotify_search_tracks(state: State<'_, AppState>, input: Value) -> Result<Value, AppError> {
    spotify_direct(state, "POST", &["search-tracks"], input).await
}

#[tauri::command]
pub async fn spotify_play_track(state: State<'_, AppState>, input: Value) -> Result<Value, AppError> {
    spotify_direct(state, "POST", &["play-track"], input).await
}

#[tauri::command]
pub async fn spotify_dj_mari_playlist(state: State<'_, AppState>, input: Value) -> Result<Value, AppError> {
    spotify_direct(state, "POST", &["dj-mari-playlist"], input).await
}

#[tauri::command]
pub async fn spotify_player_play(state: State<'_, AppState>, body: Option<Value>) -> Result<Value, AppError> {
    spotify_direct(state, "PUT", &["player", "play"], body.unwrap_or(Value::Null)).await
}

#[tauri::command]
pub async fn spotify_player_pause(state: State<'_, AppState>, body: Option<Value>) -> Result<Value, AppError> {
    spotify_direct(state, "PUT", &["player", "pause"], body.unwrap_or(Value::Null)).await
}

#[tauri::command]
pub async fn spotify_player_next(state: State<'_, AppState>, body: Option<Value>) -> Result<Value, AppError> {
    spotify_direct(state, "POST", &["player", "next"], body.unwrap_or(Value::Null)).await
}

#[tauri::command]
pub async fn spotify_player_previous(state: State<'_, AppState>, body: Option<Value>) -> Result<Value, AppError> {
    spotify_direct(state, "POST", &["player", "previous"], body.unwrap_or(Value::Null)).await
}

#[tauri::command]
pub async fn spotify_player_transfer(state: State<'_, AppState>, body: Value) -> Result<Value, AppError> {
    spotify_direct(state, "PUT", &["player", "transfer"], body).await
}

#[tauri::command]
pub async fn spotify_player_volume(state: State<'_, AppState>, body: Value) -> Result<Value, AppError> {
    spotify_direct(state, "PUT", &["player", "volume"], body).await
}

#[tauri::command]
pub async fn spotify_player_shuffle(state: State<'_, AppState>, body: Value) -> Result<Value, AppError> {
    spotify_direct(state, "PUT", &["player", "shuffle"], body).await
}

#[tauri::command]
pub async fn spotify_player_repeat(state: State<'_, AppState>, body: Value) -> Result<Value, AppError> {
    spotify_direct(state, "PUT", &["player", "repeat"], body).await
}

#[tauri::command]
pub fn knowledge_sources_list(state: State<'_, AppState>) -> Result<Value, AppError> {
    knowledge::knowledge_sources_call(&state, "GET", &[], Value::Null)
}

#[tauri::command]
pub fn knowledge_source_upload(state: State<'_, AppState>, body: Value) -> Result<Value, AppError> {
    knowledge::knowledge_sources_call(&state, "POST", &["upload"], body)
}

#[tauri::command]
pub fn knowledge_source_delete(state: State<'_, AppState>, id: String) -> Result<Value, AppError> {
    knowledge::knowledge_sources_call(&state, "DELETE", &[&id], Value::Null)
}

#[tauri::command]
pub fn knowledge_source_text(state: State<'_, AppState>, id: String) -> Result<Value, AppError> {
    knowledge::knowledge_sources_call(&state, "GET", &[&id, "text"], Value::Null)
}

#[tauri::command]
pub fn import_marinara(state: State<'_, AppState>, envelope: Value) -> Result<Value, AppError> {
    imports::import_call(&state, &["marinara"], envelope)
}

#[tauri::command]
pub fn import_marinara_file(state: State<'_, AppState>, body: Value) -> Result<Value, AppError> {
    imports::import_call(&state, &["marinara-file"], body)
}

#[tauri::command]
pub fn import_st_character(state: State<'_, AppState>, body: Value) -> Result<Value, AppError> {
    imports::import_call(&state, &["st-character"], body)
}

#[tauri::command]
pub fn import_st_character_batch(state: State<'_, AppState>, body: Value) -> Result<Value, AppError> {
    imports::import_call(&state, &["st-character", "batch"], body)
}

#[tauri::command]
pub fn import_st_character_inspect(state: State<'_, AppState>, body: Value) -> Result<Value, AppError> {
    imports::import_call(&state, &["st-character", "inspect"], body)
}

#[tauri::command]
pub fn import_st_chat(state: State<'_, AppState>, body: Value) -> Result<Value, AppError> {
    imports::import_call(&state, &["st-chat"], body)
}

#[tauri::command]
pub fn import_st_chat_into_group(state: State<'_, AppState>, body: Value) -> Result<Value, AppError> {
    imports::import_call(&state, &["st-chat-into-group"], body)
}

#[tauri::command]
pub fn import_st_preset(state: State<'_, AppState>, payload: Value) -> Result<Value, AppError> {
    imports::import_call(&state, &["st-preset"], payload)
}

#[tauri::command]
pub fn import_st_lorebook(state: State<'_, AppState>, payload: Value) -> Result<Value, AppError> {
    imports::import_call(&state, &["st-lorebook"], payload)
}

#[tauri::command]
pub fn import_list_directory(state: State<'_, AppState>, path: String) -> Result<Value, AppError> {
    imports::import_call(&state, &["list-directory"], json!({ "path": path }))
}

#[tauri::command]
pub fn import_st_bulk_scan(state: State<'_, AppState>, payload: Value) -> Result<Value, AppError> {
    imports::import_call(&state, &["st-bulk", "scan"], payload)
}

#[tauri::command]
pub fn import_st_bulk_run(state: State<'_, AppState>, payload: Value) -> Result<Value, AppError> {
    imports::import_call(&state, &["st-bulk", "run"], payload)
}

#[tauri::command]
pub fn import_st_bulk_run_events(state: State<'_, AppState>, payload: Value) -> Result<Vec<Value>, AppError> {
    imports::import_stream_events(&state, &["st-bulk", "run"], payload)
}

#[tauri::command]
pub fn storage_list(
    state: State<'_, AppState>,
    entity: String,
    options: Option<Value>,
) -> Result<Value, AppError> {
    let mut rows = match options
        .as_ref()
        .and_then(|value| value.get("filters"))
        .and_then(Value::as_object)
    {
        Some(filters) if !filters.is_empty() => state.storage.list_where(&entity, filters)?,
        _ => state.storage.list(&entity)?,
    };

    let order_by = options
        .as_ref()
        .and_then(|value| value.get("orderBy"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty());
    let descending = options
        .as_ref()
        .and_then(|value| value.get("descending"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    rows.sort_by(|a, b| {
        let ordering = match order_by {
            Some(field) => compare_json_values(a.get(field), b.get(field)),
            None => compare_json_values(
                a.get("sortOrder").or_else(|| a.get("order")).or_else(|| a.get("createdAt")),
                b.get("sortOrder").or_else(|| b.get("order")).or_else(|| b.get("createdAt")),
            ),
        };
        if descending {
            ordering.reverse()
        } else {
            ordering
        }
    });

    if let Some(limit) = options
        .as_ref()
        .and_then(|value| value.get("limit"))
        .and_then(Value::as_u64)
        .map(|value| value as usize)
    {
        rows.truncate(limit);
    }

    Ok(Value::Array(rows))
}

#[tauri::command]
pub fn storage_get(
    state: State<'_, AppState>,
    entity: String,
    id: String,
) -> Result<Value, AppError> {
    Ok(state.storage.get(&entity, &id)?.unwrap_or(Value::Null))
}

#[tauri::command]
pub fn storage_create(
    state: State<'_, AppState>,
    entity: String,
    value: Value,
) -> Result<Value, AppError> {
    state
        .storage
        .create(&entity, shared::with_entity_defaults(&entity, value))
}

#[tauri::command]
pub fn storage_update(
    state: State<'_, AppState>,
    entity: String,
    id: String,
    patch: Value,
) -> Result<Value, AppError> {
    state.storage.patch(&entity, &id, patch)
}

#[tauri::command]
pub fn storage_delete(
    state: State<'_, AppState>,
    entity: String,
    id: String,
) -> Result<Value, AppError> {
    let deleted = state.storage.delete(&entity, &id)?;
    Ok(json!({ "deleted": deleted }))
}

#[tauri::command]
pub async fn llm_complete(
    state: State<'_, AppState>,
    request: Value,
) -> Result<Value, AppError> {
    llm::llm_complete(&state, request).await
}

#[tauri::command]
pub async fn llm_stream_channel(
    state: State<'_, AppState>,
    request: Value,
    on_event: tauri::ipc::Channel<Value>,
) -> Result<(), AppError> {
    llm::llm_stream_channel(&state, request, on_event).await
}

#[tauri::command]
pub async fn llm_list_models(
    state: State<'_, AppState>,
    connection_id: Option<String>,
) -> Result<Value, AppError> {
    llm::llm_models(&state, connection_id.as_deref()).await
}

fn compare_json_values(left: Option<&Value>, right: Option<&Value>) -> std::cmp::Ordering {
    match (left, right) {
        (Some(Value::Number(a)), Some(Value::Number(b))) => a
            .as_f64()
            .partial_cmp(&b.as_f64())
            .unwrap_or(std::cmp::Ordering::Equal),
        (Some(Value::String(a)), Some(Value::String(b))) => a.cmp(b),
        (Some(Value::Bool(a)), Some(Value::Bool(b))) => a.cmp(b),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
    }
}
