use super::shared::*;
use super::*;
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;

pub(crate) fn export_record(
    state: &AppState,
    kind: &str,
    collection: &str,
    id: &str,
    format: Option<&str>,
) -> AppResult<Value> {
    let record = get_required(state, collection, id)?;
    if format == Some("compatible") {
        return Ok(compatible_record(collection, record));
    }
    Ok(json!({
        "type": kind,
        "version": 1,
        "exportedAt": now_iso(),
        "data": record
    }))
}

pub(crate) fn export_records(
    state: &AppState,
    kind: &str,
    collection: &str,
    body: Value,
) -> AppResult<Value> {
    let ids = string_array_from_value(body.get("ids"));
    let format = body.get("format").and_then(Value::as_str);
    let mut items = Vec::new();
    for id in ids {
        if let Some(record) = state.storage.get(collection, &id)? {
            items.push(if format == Some("compatible") {
                compatible_record(collection, record)
            } else {
                record
            });
        }
    }
    let mut zip = ExportZip::new();
    zip.add_json(
        "manifest.json",
        &json!({
            "type": kind,
            "version": 1,
            "exportedAt": now_iso(),
            "collection": collection,
            "count": items.len()
        }),
    )?;
    for item in &items {
        let id = item.get("id").and_then(Value::as_str).unwrap_or("record");
        let parsed_data_name = item
            .get("data")
            .and_then(Value::as_str)
            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
            .and_then(|data| data.get("name").and_then(Value::as_str).map(ToOwned::to_owned));
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .or(parsed_data_name)
            .unwrap_or_else(|| id.to_string());
        zip.add_json(
            &format!(
                "{}/{}-{}.json",
                collection,
                safe_export_name(&name, "record"),
                safe_export_name(id, "id")
            ),
            item,
        )?;
    }
    Ok(binary_download(
        zip.finish()?,
        "application/zip",
        &format!("{kind}.zip"),
    ))
}

pub(crate) fn export_character_png(state: &AppState, id: &str) -> AppResult<Value> {
    let character = get_required(state, "characters", id)?;
    let card = json!({
        "spec": "chara_card_v2",
        "spec_version": "2.0",
        "data": character_data_value(&character)
    });
    let name = card
        .get("data")
        .and_then(|data| data.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("character");
    Ok(binary_download(
        character_card_png(&card)?,
        "image/png",
        &format!("{}.png", safe_export_name(name, "character")),
    ))
}

pub(crate) fn import_character_embedded_lorebook(
    state: &AppState,
    character_id: &str,
) -> AppResult<Value> {
    let character = get_required(state, "characters", character_id)?;
    let data = character_data_value(&character);
    let book = data
        .get("character_book")
        .or_else(|| {
            data.get("data")
                .and_then(|inner| inner.get("character_book"))
        })
        .cloned()
        .unwrap_or(Value::Null);
    let entries = book
        .get("entries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if entries.is_empty() {
        return Err(AppError::invalid_input(
            "Character does not contain an embedded lorebook",
        ));
    }
    let name = character
        .get("name")
        .and_then(Value::as_str)
        .or_else(|| data.get("name").and_then(Value::as_str))
        .unwrap_or("Character");
    let lorebook = state.storage.create(
        "lorebooks",
        with_entity_defaults(
            "lorebooks",
            json!({
                "name": format!("{name} Lorebook"),
                "description": "Imported from embedded character book",
                "category": "character",
                "characterId": character_id,
                "sourceCharacterId": character_id
            }),
        ),
    )?;
    let lorebook_id = lorebook
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::new("storage_error", "Created lorebook is missing an id"))?
        .to_string();
    let mut imported = 0;
    for (index, entry) in entries.into_iter().enumerate() {
        let normalized = normalize_character_book_entry(&entry, index, &lorebook_id);
        state.storage.create("lorebook-entries", normalized)?;
        imported += 1;
    }
    patch_character_embedded_lorebook_pointer(state, character_id, &lorebook_id, imported)?;
    Ok(json!({
        "success": true,
        "lorebookId": lorebook_id,
        "entriesImported": imported,
        "reimported": false
    }))
}

pub(crate) fn export_prompt(state: &AppState, preset_id: &str) -> AppResult<Value> {
    Ok(json!({
        "type": "marinara_preset",
        "version": 1,
        "exportedAt": now_iso(),
        "data": get_required(state, "prompts", preset_id)?,
        "groups": list_collection(state, "prompt-groups", Some(("presetId", preset_id)))?,
        "sections": list_collection(state, "prompt-sections", Some(("presetId", preset_id)))?,
        "variables": list_collection(state, "prompt-variables", Some(("presetId", preset_id)))?
    }))
}

pub(crate) fn preview_prompt(state: &AppState, preset_id: &str, body: Value) -> AppResult<Value> {
    let preset = get_required(state, "prompts", preset_id)?;
    let chat_id = body.get("chatId").and_then(Value::as_str).unwrap_or("");
    let mut messages = Vec::new();
    if let Some(system) = preset
        .get("systemPrompt")
        .or_else(|| preset.get("system"))
        .or_else(|| preset.get("prompt"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        messages.push(json!({ "role": "system", "content": system }));
    }
    if !chat_id.is_empty() {
        for message in super::chats::messages_for_chat(state, chat_id)? {
            messages.push(json!({
                "role": message.get("role").and_then(Value::as_str).unwrap_or("user"),
                "content": message.get("content").and_then(Value::as_str).unwrap_or("")
            }));
        }
    }
    if messages.is_empty() {
        messages.push(json!({ "role": "system", "content": preset.get("name").and_then(Value::as_str).unwrap_or("Prompt preset") }));
    }
    let message_count = messages.len();
    Ok(json!({
        "messages": messages,
        "parameters": preset.get("parameters").cloned().unwrap_or_else(|| json!({})),
        "messageCount": message_count
    }))
}

pub(crate) fn export_lorebook(
    state: &AppState,
    lorebook_id: &str,
    format: Option<&str>,
) -> AppResult<Value> {
    let lorebook = get_required(state, "lorebooks", lorebook_id)?;
    let entries = list_collection(state, "lorebook-entries", Some(("lorebookId", lorebook_id)))?;
    if format == Some("compatible") {
        return Ok(json!({
            "name": lorebook.get("name").cloned().unwrap_or_else(|| json!("Lorebook")),
            "description": lorebook.get("description").cloned().unwrap_or(Value::Null),
            "entries": entries
        }));
    }
    Ok(json!({
        "type": "marinara_lorebook",
        "version": 1,
        "exportedAt": now_iso(),
        "data": lorebook,
        "entries": entries,
        "folders": list_collection(state, "lorebook-folders", Some(("lorebookId", lorebook_id)))?
    }))
}

pub(crate) fn export_lorebooks(state: &AppState, body: Value) -> AppResult<Value> {
    let ids = string_array_from_value(body.get("ids"));
    let format = body.get("format").and_then(Value::as_str);
    let mut items = Vec::new();
    for id in ids {
        items.push(export_lorebook(state, &id, format)?);
    }
    let mut zip = ExportZip::new();
    zip.add_json(
        "manifest.json",
        &json!({
            "type": "marinara_lorebooks",
            "version": 1,
            "exportedAt": now_iso(),
            "count": items.len()
        }),
    )?;
    for item in &items {
        let name = item
            .get("data")
            .and_then(|data| data.get("name"))
            .and_then(Value::as_str)
            .or_else(|| item.get("name").and_then(Value::as_str))
            .unwrap_or("lorebook");
        let id = item
            .get("data")
            .and_then(|data| data.get("id"))
            .and_then(Value::as_str)
            .or_else(|| item.get("id").and_then(Value::as_str))
            .unwrap_or("lorebook");
        zip.add_json(
            &format!(
                "lorebooks/{}-{}.json",
                safe_export_name(name, "lorebook"),
                safe_export_name(id, "id")
            ),
            item,
        )?;
    }
    Ok(binary_download(
        zip.finish()?,
        "application/zip",
        "marinara_lorebooks.zip",
    ))
}

fn compatible_record(collection: &str, record: Value) -> Value {
    if collection == "characters" {
        return character_data_value(&record);
    }
    record
}

fn character_data_value(character: &Value) -> Value {
    match character.get("data") {
        Some(Value::String(raw)) => serde_json::from_str(raw).unwrap_or_else(|_| json!({})),
        Some(value) => value.clone(),
        None => character.clone(),
    }
}

fn normalize_character_book_entry(entry: &Value, index: usize, lorebook_id: &str) -> Value {
    let keys = entry
        .get("keys")
        .or_else(|| entry.get("key"))
        .cloned()
        .unwrap_or_else(|| json!([]));
    json!({
        "lorebookId": lorebook_id,
        "name": entry.get("name").or_else(|| entry.get("comment")).and_then(Value::as_str).unwrap_or("Entry"),
        "content": entry.get("content").and_then(Value::as_str).unwrap_or(""),
        "keys": keys,
        "secondaryKeys": entry.get("secondary_keys").or_else(|| entry.get("secondaryKeys")).cloned().unwrap_or_else(|| json!([])),
        "constant": entry.get("constant").and_then(Value::as_bool).unwrap_or(false),
        "selective": entry.get("selective").and_then(Value::as_bool).unwrap_or(false),
        "enabled": entry.get("enabled").and_then(Value::as_bool).unwrap_or(true),
        "order": entry.get("insertion_order").or_else(|| entry.get("order")).and_then(Value::as_i64).unwrap_or(index as i64),
        "position": entry.get("position").and_then(Value::as_str).unwrap_or("before_char"),
        "folderId": Value::Null
    })
}

fn patch_character_embedded_lorebook_pointer(
    state: &AppState,
    character_id: &str,
    lorebook_id: &str,
    entries_imported: usize,
) -> AppResult<()> {
    let character = get_required(state, "characters", character_id)?;
    let mut data = character_data_value(&character);
    let data_object = data
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_input("Character data is not an object"))?;
    let extensions = data_object
        .entry("extensions".to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_input("Character extensions are not an object"))?;
    let import_metadata = extensions
        .entry("importMetadata".to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_input("Character import metadata is not an object"))?;
    import_metadata.insert(
        "embeddedLorebook".to_string(),
        json!({
            "hasEmbeddedLorebook": true,
            "lorebookId": lorebook_id,
            "entriesImported": entries_imported
        }),
    );
    state.storage.patch(
        "characters",
        character_id,
        json!({ "data": serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string()) }),
    )?;
    Ok(())
}

struct ExportZip {
    writer: zip::ZipWriter<Cursor<Vec<u8>>>,
    options: SimpleFileOptions,
}

impl ExportZip {
    fn new() -> Self {
        Self {
            writer: zip::ZipWriter::new(Cursor::new(Vec::new())),
            options: SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        }
    }

    fn add_json(&mut self, path: &str, value: &Value) -> AppResult<()> {
        self.writer
            .start_file(path.replace('\\', "/"), self.options)
            .map_err(zip_error)?;
        self.writer.write_all(&serde_json::to_vec_pretty(value)?)?;
        Ok(())
    }

    fn finish(self) -> AppResult<Vec<u8>> {
        Ok(self.writer.finish().map_err(zip_error)?.into_inner())
    }
}

fn character_card_png(card: &Value) -> AppResult<Vec<u8>> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let chara = general_purpose::STANDARD.encode(serde_json::to_vec(card)?);
        encoder
            .add_text_chunk("chara".to_string(), chara)
            .map_err(|error| AppError::new("png_export_error", error.to_string()))?;
        let mut writer = encoder
            .write_header()
            .map_err(|error| AppError::new("png_export_error", error.to_string()))?;
        writer
            .write_image_data(&[0, 0, 0, 0])
            .map_err(|error| AppError::new("png_export_error", error.to_string()))?;
    }
    Ok(bytes)
}

fn binary_download(bytes: Vec<u8>, content_type: &str, filename: &str) -> Value {
    json!({
        "base64": general_purpose::STANDARD.encode(bytes),
        "contentType": content_type,
        "filename": filename
    })
}

fn zip_error(error: zip::result::ZipError) -> AppError {
    AppError::new("zip_error", error.to_string())
}

fn safe_export_name(name: &str, fallback: &str) -> String {
    let sanitized = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string();
    if sanitized.is_empty() {
        fallback.to_string()
    } else {
        sanitized
    }
}
