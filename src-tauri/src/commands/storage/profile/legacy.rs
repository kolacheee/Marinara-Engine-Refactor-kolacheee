use super::super::shared::materialize_message_swipe_fields;
use super::assets::{normalize_legacy_profile_asset_paths, restore_legacy_profile_json_assets};
use super::insert_profile_import_aliases;
use crate::state::AppState;
use marinara_core::AppResult;
use serde_json::{json, Map, Value};

const LEGACY_PROFILE_TABLES: &[(&str, &str)] = &[
    ("characters", "characters"),
    ("character_groups", "character-groups"),
    ("character_card_versions", "character-versions"),
    ("personas", "personas"),
    ("persona_groups", "persona-groups"),
    ("lorebooks", "lorebooks"),
    ("lorebook_entries", "lorebook-entries"),
    ("lorebook_folders", "lorebook-folders"),
    ("prompt_presets", "prompts"),
    ("prompt_groups", "prompt-groups"),
    ("prompt_sections", "prompt-sections"),
    ("choice_blocks", "prompt-variables"),
    ("chat_presets", "chat-presets"),
    ("agent_configs", "agents"),
    ("agent_runs", "agent-runs"),
    ("agent_memory", "agent-memory"),
    ("custom_themes", "themes"),
    ("installed_extensions", "extensions"),
    ("api_connections", "connections"),
    ("api_connection_folders", "connection-folders"),
    ("chats", "chats"),
    ("chat_folders", "chat-folders"),
    ("messages", "messages"),
    ("custom_tools", "custom-tools"),
    ("regex_scripts", "regex-scripts"),
    ("app_settings", "app-settings"),
    ("chat_images", "gallery"),
    ("character_images", "character-gallery"),
    ("background_metadata", "background-metadata"),
    ("knowledge_sources", "knowledge-sources"),
    ("game_state_snapshots", "game-state-snapshots"),
    ("game_checkpoints", "game-checkpoints"),
];

pub(super) fn import_legacy_profile_tables(
    state: &AppState,
    data: &Map<String, Value>,
    tables: &Map<String, Value>,
) -> AppResult<Value> {
    let files = data.get("fileStorage").and_then(|value| value.get("files"));
    let restored_assets = restore_legacy_profile_json_assets(state, files)?;
    import_legacy_profile_tables_with_restored_assets(state, tables, restored_assets)
}

pub(super) fn import_legacy_profile_tables_with_restored_assets(
    state: &AppState,
    tables: &Map<String, Value>,
    restored_assets: usize,
) -> AppResult<Value> {
    let mut imported = Map::new();
    for (table, collection) in LEGACY_PROFILE_TABLES {
        let mut rows = table_rows(tables, table);
        match *collection {
            "lorebooks" => add_legacy_lorebook_links(&mut rows, tables),
            "chats" => add_legacy_chat_memories(&mut rows, tables),
            "messages" => add_legacy_message_swipes(&mut rows, tables),
            _ => {}
        }
        for row in &mut rows {
            normalize_legacy_profile_asset_paths(state, row);
        }
        state.storage.replace_all(collection, rows.clone())?;
        imported.insert((*collection).to_string(), json!(rows.len()));
    }
    imported.insert("files".to_string(), json!(restored_assets));
    insert_profile_import_aliases(&mut imported);
    Ok(json!({ "success": true, "imported": imported }))
}

fn table_rows(tables: &Map<String, Value>, table: &str) -> Vec<Value> {
    tables
        .get(table)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn add_legacy_lorebook_links(rows: &mut [Value], tables: &Map<String, Value>) {
    let character_links = table_rows(tables, "lorebook_character_links");
    let persona_links = table_rows(tables, "lorebook_persona_links");
    for row in rows {
        let Some(object) = row.as_object_mut() else {
            continue;
        };
        let Some(lorebook_id) = object.get("id").and_then(Value::as_str) else {
            continue;
        };
        let mut character_ids =
            linked_ids(&character_links, "lorebookId", lorebook_id, "characterId");
        let mut persona_ids = linked_ids(&persona_links, "lorebookId", lorebook_id, "personaId");
        if let Some(character_id) = object
            .get("characterId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
        {
            push_unique(&mut character_ids, character_id);
        }
        if let Some(persona_id) = object
            .get("personaId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
        {
            push_unique(&mut persona_ids, persona_id);
        }
        object
            .entry("characterIds".to_string())
            .or_insert_with(|| json!(character_ids));
        object
            .entry("personaIds".to_string())
            .or_insert_with(|| json!(persona_ids));
    }
}

fn linked_ids(rows: &[Value], source_key: &str, source_id: &str, target_key: &str) -> Vec<String> {
    rows.iter()
        .filter(|row| row.get(source_key).and_then(Value::as_str) == Some(source_id))
        .filter_map(|row| row.get(target_key).and_then(Value::as_str))
        .map(ToOwned::to_owned)
        .collect()
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|item| item == value) {
        values.push(value.to_string());
    }
}

fn add_legacy_chat_memories(rows: &mut [Value], tables: &Map<String, Value>) {
    let memory_chunks = table_rows(tables, "memory_chunks");
    if memory_chunks.is_empty() {
        return;
    }
    for row in rows {
        let Some(object) = row.as_object_mut() else {
            continue;
        };
        let Some(chat_id) = object.get("id").and_then(Value::as_str) else {
            continue;
        };
        let memories = memory_chunks
            .iter()
            .filter(|chunk| chunk.get("chatId").and_then(Value::as_str) == Some(chat_id))
            .cloned()
            .map(normalize_legacy_memory_chunk)
            .collect::<Vec<_>>();
        if !memories.is_empty() {
            object.insert("memories".to_string(), Value::Array(memories));
        }
    }
}

fn normalize_legacy_memory_chunk(mut chunk: Value) -> Value {
    let has_embedding = chunk
        .get("embedding")
        .and_then(Value::as_array)
        .map(|values| !values.is_empty())
        .unwrap_or(false);
    if let Some(object) = chunk.as_object_mut() {
        object.insert("hasEmbedding".to_string(), json!(has_embedding));
        object.insert(
            "embeddingStatus".to_string(),
            Value::String(
                if has_embedding {
                    "vectorized"
                } else {
                    "unavailable"
                }
                .to_string(),
            ),
        );
    }
    chunk
}

fn add_legacy_message_swipes(rows: &mut [Value], tables: &Map<String, Value>) {
    let swipes = table_rows(tables, "message_swipes");
    if swipes.is_empty() {
        return;
    }
    for row in rows {
        let Some(object) = row.as_object_mut() else {
            continue;
        };
        let Some(message_id) = object.get("id").and_then(Value::as_str) else {
            continue;
        };
        let mut message_swipes = swipes
            .iter()
            .filter(|swipe| swipe.get("messageId").and_then(Value::as_str) == Some(message_id))
            .cloned()
            .collect::<Vec<_>>();
        if message_swipes.is_empty() {
            continue;
        }
        message_swipes.sort_by_key(|swipe| swipe.get("index").and_then(Value::as_i64).unwrap_or(0));
        object.insert("swipes".to_string(), Value::Array(message_swipes));
        materialize_message_swipe_fields(row);
    }
}
