use super::shared::*;
use super::*;

pub(crate) fn admin_clear_all(state: &AppState) -> AppResult<Value> {
    state.storage.clear_all()?;
    clear_runtime_media(state)?;
    Ok(json!({ "success": true, "cleared": "all" }))
}

pub(crate) fn admin_expunge(state: &AppState, body: Value) -> AppResult<Value> {
    if body.get("confirm").and_then(Value::as_bool) != Some(true) {
        return Err(AppError::invalid_input("confirm must be true"));
    }
    let scopes = string_array_from_value(body.get("scopes"));
    if scopes.is_empty() {
        return Err(AppError::invalid_input("At least one expunge scope is required"));
    }
    let mut cleared_collections = Vec::new();
    for scope in scopes {
        match scope.as_str() {
            "chats" => clear_collections(
                state,
                &[
                    "chats",
                    "chat-folders",
                    "messages",
                    "gallery",
                    "agent-runs",
                    "knowledge-sources",
                ],
                &mut cleared_collections,
            )?,
            "characters" => {
                clear_collections(
                    state,
                    &[
                        "character-groups",
                        "character-versions",
                        "character-gallery",
                        "sprites",
                    ],
                    &mut cleared_collections,
                )?;
                preserve_professor_mari(state)?;
                cleared_collections.push("characters".to_string());
            }
            "personas" => clear_collections(
                state,
                &["personas", "persona-groups"],
                &mut cleared_collections,
            )?,
            "lorebooks" => clear_collections(
                state,
                &["lorebooks", "lorebook-entries", "lorebook-folders"],
                &mut cleared_collections,
            )?,
            "presets" => clear_collections(
                state,
                &[
                    "prompts",
                    "prompt-groups",
                    "prompt-sections",
                    "prompt-variables",
                    "chat-presets",
                ],
                &mut cleared_collections,
            )?,
            "connections" => clear_collections(
                state,
                &["connections", "connection-folders"],
                &mut cleared_collections,
            )?,
            "automation" => clear_collections(
                state,
                &["agents", "custom-tools", "regex-scripts", "themes", "extensions"],
                &mut cleared_collections,
            )?,
            "media" => {
                clear_collections(
                    state,
                    &[
                        "gallery",
                        "character-gallery",
                        "background-metadata",
                        "sprites",
                        "knowledge-sources",
                    ],
                    &mut cleared_collections,
                )?;
                clear_runtime_media(state)?;
            }
            other => return Err(AppError::invalid_input(format!("Unknown expunge scope: {other}"))),
        }
    }
    cleared_collections.sort();
    cleared_collections.dedup();
    Ok(json!({ "success": true, "clearedCollections": cleared_collections }))
}

fn clear_collections(
    state: &AppState,
    collections: &[&str],
    cleared: &mut Vec<String>,
) -> AppResult<()> {
    for collection in collections {
        state.storage.replace_all(collection, Vec::new())?;
        cleared.push((*collection).to_string());
    }
    Ok(())
}

fn preserve_professor_mari(state: &AppState) -> AppResult<()> {
    let kept = state
        .storage
        .list("characters")?
        .into_iter()
        .filter(is_professor_mari)
        .collect::<Vec<_>>();
    state.storage.replace_all("characters", kept)
}

fn is_professor_mari(character: &Value) -> bool {
    let name = character_name(character).to_ascii_lowercase();
    name.contains("professor mari") || character.get("id").and_then(Value::as_str) == Some("professor-mari")
}

fn character_name(character: &Value) -> String {
    character
        .get("name")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| {
            character
                .get("data")
                .and_then(Value::as_str)
                .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                .and_then(|data| data.get("name").and_then(Value::as_str).map(ToOwned::to_owned))
        })
        .unwrap_or_default()
}

fn clear_runtime_media(state: &AppState) -> AppResult<()> {
    for path in [
        state.data_dir.join("avatars"),
        state.data_dir.join("fonts"),
        state.data_dir.join("knowledge-sources"),
        state.game_assets.root().to_path_buf(),
        state.backgrounds.root().to_path_buf(),
    ] {
        if path.exists() {
            fs::remove_dir_all(&path)?;
        }
        fs::create_dir_all(&path)?;
    }
    Ok(())
}
