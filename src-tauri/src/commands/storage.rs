use crate::state::AppState;
use base64::{engine::general_purpose, Engine as _};
use marinara_core::{ensure_object, new_id, now_iso, now_millis, AppError, AppResult};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::fs;
use std::time::Duration;
use tauri::State;

mod agents;
mod admin;
mod avatars;
mod backgrounds;
mod backup;
mod bot_browser;
mod chat_presets;
mod characters;
mod chats;
mod encounter;
mod exports;
mod fonts;
mod game;
mod game_assets;
mod generation;
mod http;
mod images;
mod imports;
mod integrations;
mod knowledge;
mod llm;
mod prompts;
mod router;
mod scene;
mod shared;
mod sprites;
mod translation;

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
