use super::media_uploads::{
    decode_image_payload, extension_for_image_mime, safe_filename, unique_file_path,
};
use super::shared::*;
use super::*;
#[path = "imports/bulk_imports.rs"]
mod bulk_imports;
#[path = "imports/timestamps.rs"]
mod timestamps;
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use timestamps::{apply_timestamp_overrides, timestamp_overrides_from_value};

fn parse_object(raw: &[u8]) -> AppResult<Value> {
    Ok(serde_json::from_slice(raw)?)
}

fn parse_json_text(raw: &str) -> AppResult<Value> {
    Ok(serde_json::from_str(raw)?)
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "Imported".to_string())
}

fn modified_at(path: &Path) -> Value {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| Value::String(format!("{}", duration.as_millis())))
        .unwrap_or(Value::Null)
}

fn parse_chara_text(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    parse_json_text(trimmed).ok().or_else(|| {
        general_purpose::STANDARD
            .decode(trimmed)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
    })
}

fn extract_chara_from_png(bytes: &[u8]) -> AppResult<Value> {
    const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 8 || &bytes[..8] != PNG_SIGNATURE {
        return Err(AppError::invalid_input("Not a PNG character card"));
    }

    let mut offset = 8usize;
    let mut chara: Option<Value> = None;
    let mut ccv3: Option<Value> = None;
    while offset + 12 <= bytes.len() {
        let length = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        let chunk_type = &bytes[offset + 4..offset + 8];
        let data_start = offset + 8;
        let data_end = data_start.saturating_add(length);
        if data_end + 4 > bytes.len() {
            break;
        }
        let payload = &bytes[data_start..data_end];
        if chunk_type == b"tEXt" {
            if let Some(null_index) = payload.iter().position(|byte| *byte == 0) {
                let keyword = String::from_utf8_lossy(&payload[..null_index]);
                if keyword == "chara" || keyword == "ccv3" {
                    let text = String::from_utf8_lossy(&payload[null_index + 1..]);
                    if let Some(parsed) = parse_chara_text(&text) {
                        if keyword == "ccv3" {
                            ccv3 = Some(parsed);
                        } else {
                            chara = Some(parsed);
                        }
                    }
                }
            }
        } else if chunk_type == b"iTXt" {
            if let Some(null_index) = payload.iter().position(|byte| *byte == 0) {
                let keyword = String::from_utf8_lossy(&payload[..null_index]);
                if (keyword == "chara" || keyword == "ccv3") && null_index + 3 < payload.len() {
                    let compression_flag = payload[null_index + 1];
                    if compression_flag == 0 {
                        let language_start = null_index + 3;
                        if let Some(language_end_rel) =
                            payload[language_start..].iter().position(|byte| *byte == 0)
                        {
                            let translated_start = language_start + language_end_rel + 1;
                            if let Some(translated_end_rel) = payload[translated_start..]
                                .iter()
                                .position(|byte| *byte == 0)
                            {
                                let text_start = translated_start + translated_end_rel + 1;
                                let text = String::from_utf8_lossy(&payload[text_start..]);
                                if let Some(parsed) = parse_chara_text(&text) {
                                    if keyword == "ccv3" {
                                        ccv3 = Some(parsed);
                                    } else {
                                        chara = Some(parsed);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        offset = data_end + 4;
        if chunk_type == b"IEND" {
            break;
        }
    }

    ccv3
        .or(chara)
        .ok_or_else(|| AppError::invalid_input("No character data found in PNG. Make sure this is a valid character card with embedded metadata."))
}

fn read_zip_entry(bytes: &[u8], name: &str) -> AppResult<Option<Vec<u8>>> {
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|error| AppError::invalid_input(format!("Could not read zip archive: {error}")))?;
    let result = match archive.by_name(name) {
        Ok(mut file) => {
            let mut contents = Vec::new();
            file.read_to_end(&mut contents)?;
            Ok(Some(contents))
        }
        Err(zip::result::ZipError::FileNotFound) => Ok(None),
        Err(error) => Err(AppError::invalid_input(format!(
            "Could not read zip entry {name}: {error}"
        ))),
    };
    result
}

fn read_zip_entry_names(bytes: &[u8]) -> AppResult<Vec<String>> {
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|error| AppError::invalid_input(format!("Could not read zip archive: {error}")))?;
    let mut names = Vec::new();
    for index in 0..archive.len() {
        let file = archive.by_index(index).map_err(|error| {
            AppError::invalid_input(format!("Could not read zip entry: {error}"))
        })?;
        names.push(file.name().to_string());
    }
    Ok(names)
}

fn zip_entry_name_case_insensitive(names: &[String], expected: &str) -> Option<String> {
    names
        .iter()
        .find(|name| name.eq_ignore_ascii_case(expected))
        .cloned()
}

fn directory_listing(path: PathBuf) -> AppResult<Value> {
    if !path.is_dir() {
        return Ok(json!({ "success": false, "error": "Not a directory" }));
    }
    let mut folders: Vec<String> = fs::read_dir(&path)
        .map(|rows| {
            rows.filter_map(Result::ok)
                .filter(|entry| entry.path().is_dir())
                .filter_map(|entry| entry.file_name().to_str().map(ToOwned::to_owned))
                .filter(|name| !name.starts_with('.'))
                .collect()
        })
        .unwrap_or_default();
    folders.sort_by_key(|name| name.to_ascii_lowercase());
    Ok(json!({
        "success": true,
        "path": path.to_string_lossy(),
        "folderToken": path.to_string_lossy(),
        "folders": folders
    }))
}

fn image_mime_from_path(path: &str) -> &'static str {
    match Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "avif" => "image/avif",
        _ => "image/png",
    }
}

fn resolve_charx_asset(bytes: &[u8], uri: &str, ext: Option<&str>) -> AppResult<Option<String>> {
    if uri.starts_with("data:image/") {
        return Ok(Some(uri.to_string()));
    }
    let zip_path = if let Some(path) = uri.strip_prefix("embeded://") {
        Some(path)
    } else if let Some(path) = uri.strip_prefix("embedded://") {
        Some(path)
    } else if !uri.contains("://") && uri != "ccdefault:" {
        Some(uri)
    } else {
        None
    };
    let Some(zip_path) = zip_path else {
        return Ok(None);
    };
    let Some(asset) = read_zip_entry(bytes, zip_path)? else {
        return Ok(None);
    };
    let mime = ext
        .map(
            |value| match value.trim_start_matches('.').to_ascii_lowercase().as_str() {
                "jpg" | "jpeg" => "image/jpeg",
                "webp" => "image/webp",
                "gif" => "image/gif",
                "avif" => "image/avif",
                _ => "image/png",
            },
        )
        .unwrap_or_else(|| image_mime_from_path(zip_path));
    Ok(Some(format!(
        "data:{mime};base64,{}",
        general_purpose::STANDARD.encode(asset)
    )))
}

fn extract_charx(bytes: &[u8]) -> AppResult<Value> {
    let Some(card_bytes) = read_zip_entry(bytes, "card.json")? else {
        return Err(AppError::invalid_input(
            "Invalid .charx file: missing card.json at root.",
        ));
    };
    let mut card = parse_object(&card_bytes)?;
    let card_data = card
        .get("data")
        .filter(|value| value.is_object())
        .unwrap_or(&card);
    let mut avatar: Option<String> = None;
    if let Some(assets) = card_data.get("assets").and_then(Value::as_array) {
        let selected = assets
            .iter()
            .find(|asset| {
                asset.get("type").and_then(Value::as_str) == Some("icon")
                    && asset.get("name").and_then(Value::as_str) == Some("main")
            })
            .or_else(|| {
                assets
                    .iter()
                    .find(|asset| asset.get("type").and_then(Value::as_str) == Some("icon"))
            });
        if let Some(asset) = selected {
            if let Some(uri) = asset.get("uri").and_then(Value::as_str) {
                avatar = resolve_charx_asset(bytes, uri, asset.get("ext").and_then(Value::as_str))?;
            }
        }
    }
    if avatar.is_none() {
        for fallback in [
            "assets/icon/images/main.png",
            "assets/icon/images/main.webp",
            "assets/icon/images/main.jpg",
        ] {
            if let Some(asset) = read_zip_entry(bytes, fallback)? {
                let mime = image_mime_from_path(fallback);
                avatar = Some(format!(
                    "data:{mime};base64,{}",
                    general_purpose::STANDARD.encode(asset)
                ));
                break;
            }
        }
    }
    if let Some(avatar) = avatar {
        let object = card
            .as_object_mut()
            .ok_or_else(|| AppError::invalid_input("card.json must contain an object"))?;
        object.insert("_avatarDataUrl".to_string(), Value::String(avatar));
    }
    Ok(card)
}

fn parse_character_file(filename: &str, bytes: &[u8]) -> AppResult<Value> {
    let lower = filename.to_ascii_lowercase();
    if lower.ends_with(".png") {
        let mut payload = extract_chara_from_png(bytes)?;
        let object = payload.as_object_mut().ok_or_else(|| {
            AppError::invalid_input("Embedded character data must be a JSON object")
        })?;
        object.insert(
            "_avatarDataUrl".to_string(),
            Value::String(format!(
                "data:image/png;base64,{}",
                general_purpose::STANDARD.encode(bytes)
            )),
        );
        return Ok(payload);
    }
    if lower.ends_with(".charx") {
        return extract_charx(bytes);
    }
    parse_object(bytes).map_err(|_| {
        AppError::invalid_input("Invalid file format. Expected a JSON character card, PNG with embedded character data, or .charx file.")
    })
}

pub(crate) fn import_payload(body: Value) -> AppResult<Value> {
    if body.get("file").is_some() {
        let (_name, _content_type, bytes) = decode_uploaded_file(&body)?;
        let mut payload = parse_object(&bytes)?;
        if let Some(fields) = body.get("fields").and_then(Value::as_object) {
            if let Some(object) = payload.as_object_mut() {
                for (key, value) in fields {
                    object.insert(key.clone(), value.clone());
                }
            }
        }
        return Ok(payload);
    }
    Ok(body)
}

fn string_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect(),
        Some(Value::String(raw)) if !raw.trim().is_empty() => {
            serde_json::from_str::<Vec<String>>(raw).unwrap_or_else(|_| vec![raw.to_string()])
        }
        _ => Vec::new(),
    }
}

fn first_string(values: Vec<Option<&Value>>) -> String {
    values
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .find(|value| !value.is_empty())
        .unwrap_or("")
        .to_string()
}

fn source_character_data(payload: &Value) -> Value {
    if matches!(
        payload.get("spec").and_then(Value::as_str),
        Some("chara_card_v2" | "chara_card_v3")
    ) {
        return payload
            .get("data")
            .filter(|value| value.is_object())
            .cloned()
            .unwrap_or_else(|| payload.clone());
    }
    if payload.get("type").and_then(Value::as_str) == Some("character") {
        return payload
            .get("data")
            .filter(|value| value.is_object())
            .cloned()
            .unwrap_or_else(|| payload.clone());
    }
    payload.clone()
}

fn embedded_lorebook(payload: &Value) -> Option<Value> {
    let wrapped = source_character_data(payload);
    let mut candidates = Vec::new();
    if let Some(book) = payload.get("character_book") {
        candidates.push(book);
    }
    if let Some(book) = wrapped.get("character_book") {
        candidates.push(book);
    }
    if let Some(book) = payload
        .get("data")
        .and_then(|data| data.get("character_book"))
    {
        candidates.push(book);
    }
    candidates
        .into_iter()
        .filter(|book| lorebook_entry_count(book) > 0)
        .max_by_key(|book| lorebook_entry_count(book))
        .cloned()
}

fn alt_descriptions(data: &Value) -> Value {
    data.get("extensions")
        .and_then(|extensions| extensions.get("altDescriptions"))
        .or_else(|| {
            data.get("extensions")
                .and_then(|extensions| extensions.get("alt_descriptions"))
        })
        .or_else(|| data.get("altDescriptions"))
        .or_else(|| data.get("alternate_descriptions"))
        .filter(|value| value.is_array())
        .cloned()
        .unwrap_or_else(|| json!([]))
}

fn strip_stale_embedded_lorebook_pointer(data: &mut Value) {
    if let Some(book) = data.pointer_mut("/extensions/importMetadata/embeddedLorebook") {
        if let Some(object) = book.as_object_mut() {
            object.remove("lorebookId");
        }
    }
}

fn character_import_extensions(payload: &Value, data: &Value, embedded: Option<&Value>) -> Value {
    let mut extensions = data
        .get("extensions")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    extensions
        .entry("altDescriptions".to_string())
        .or_insert_with(|| alt_descriptions(data));

    let import_metadata = extensions
        .entry("importMetadata".to_string())
        .or_insert_with(|| json!({}));
    if let Some(import_metadata) = import_metadata.as_object_mut() {
        import_metadata.insert(
            "card".to_string(),
            json!({
                "spec": payload.get("spec").and_then(Value::as_str).unwrap_or("chara_card_v2"),
                "specVersion": payload.get("spec_version").and_then(Value::as_str).unwrap_or("2.0"),
                "format": payload.get("spec").and_then(Value::as_str).unwrap_or("chara_card_v2")
            }),
        );
        if let Some(book) = embedded {
            import_metadata.insert(
                "embeddedLorebook".to_string(),
                json!({
                    "hasEmbeddedLorebook": true,
                    "entries": lorebook_entry_count(book)
                }),
            );
        }
    }
    Value::Object(extensions)
}

fn normalize_character_data(payload: &Value, tag_mode: &str, existing_tags: &[String]) -> Value {
    let data = source_character_data(payload);
    let embedded = embedded_lorebook(payload);
    let mut tags = string_array(data.get("tags"));
    if tag_mode == "none" {
        tags.clear();
    } else if tag_mode == "existing" {
        let keys: Vec<String> = existing_tags.iter().map(|tag| tag.to_lowercase()).collect();
        tags.retain(|tag| keys.contains(&tag.to_lowercase()));
    }
    let mut normalized = json!({
        "name": first_string(vec![data.get("name"), payload.get("char_name"), payload.get("name")]).if_empty("Imported Character"),
        "description": first_string(vec![data.get("description"), payload.get("char_persona")]),
        "personality": first_string(vec![data.get("personality"), payload.get("personality")]),
        "scenario": first_string(vec![data.get("scenario"), payload.get("world_scenario")]),
        "first_mes": first_string(vec![data.get("first_mes"), payload.get("char_greeting"), payload.get("first_mes")]),
        "mes_example": first_string(vec![data.get("mes_example"), payload.get("example_dialogue")]),
        "creator_notes": first_string(vec![data.get("creator_notes"), payload.get("creatorcomment"), payload.get("comment")]),
        "system_prompt": first_string(vec![data.get("system_prompt"), payload.get("system_prompt")]),
        "post_history_instructions": first_string(vec![data.get("post_history_instructions"), payload.get("post_history_instructions")]),
        "tags": tags,
        "creator": first_string(vec![data.get("creator"), payload.get("creator")]),
        "character_version": first_string(vec![data.get("character_version"), payload.get("character_version")]).if_empty("1.0"),
        "alternate_greetings": string_array(data.get("alternate_greetings")),
        "extensions": character_import_extensions(payload, &data, embedded.as_ref()),
        "character_book": embedded.unwrap_or(Value::Null),
    });
    strip_stale_embedded_lorebook_pointer(&mut normalized);
    normalized
}

trait ImportStringFallback {
    fn if_empty(self, fallback: &str) -> String;
}

impl ImportStringFallback for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.trim().is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}

fn lorebook_entries(value: &Value) -> Vec<Value> {
    match value.get("entries") {
        Some(Value::Array(items)) => items.clone(),
        Some(Value::Object(map)) => map.values().cloned().collect(),
        _ => Vec::new(),
    }
}

fn lorebook_entry_count(value: &Value) -> usize {
    lorebook_entries(value).len()
}

fn number(value: Option<&Value>, fallback: i64) -> i64 {
    value
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_str().and_then(|raw| raw.parse::<i64>().ok()))
        })
        .unwrap_or(fallback)
}

fn optional_number(value: Option<&Value>) -> Value {
    value
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_str().and_then(|raw| raw.parse::<i64>().ok()))
        })
        .map_or(Value::Null, |value| json!(value))
}

fn bool_field(value: Option<&Value>, fallback: bool) -> bool {
    value.and_then(Value::as_bool).unwrap_or(fallback)
}

fn normalize_lorebook_entry(lorebook_id: &str, entry: &Value, index: usize) -> Value {
    let keys = entry.get("key").or_else(|| entry.get("keys"));
    let secondary = entry
        .get("keysecondary")
        .or_else(|| entry.get("secondary_keys"));
    let enabled = entry
        .get("disable")
        .and_then(Value::as_bool)
        .map(|disabled| !disabled)
        .unwrap_or_else(|| bool_field(entry.get("enabled"), true));
    let role = match entry.get("role").and_then(Value::as_str) {
        Some("user" | "assistant" | "system") => entry
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("system"),
        _ => "system",
    };
    let position = match entry.get("position") {
        Some(Value::String(raw)) if raw == "after_char" => 1,
        Some(Value::String(raw)) if raw == "at_depth" || raw == "depth" => 2,
        Some(Value::Number(raw)) => raw.as_i64().unwrap_or(0),
        _ => 0,
    };
    json!({
        "lorebookId": lorebook_id,
        "name": entry.get("comment").or_else(|| entry.get("name")).and_then(Value::as_str).unwrap_or(&format!("Entry {}", index + 1)),
        "content": string_field(entry, "content"),
        "description": string_field(entry, "description"),
        "keys": string_array(keys),
        "secondaryKeys": string_array(secondary),
        "enabled": enabled,
        "constant": bool_field(entry.get("constant"), false),
        "selective": bool_field(entry.get("selective"), false),
        "selectiveLogic": "and",
        "probability": optional_number(entry.get("probability")),
        "scanDepth": optional_number(entry.get("scanDepth").or_else(|| entry.get("scan_depth"))),
        "matchWholeWords": bool_field(entry.get("matchWholeWords").or_else(|| entry.get("match_whole_words")), false),
        "caseSensitive": bool_field(entry.get("caseSensitive").or_else(|| entry.get("case_sensitive")), false),
        "useRegex": bool_field(entry.get("useRegex").or_else(|| entry.get("regex")), false),
        "characterFilterMode": "any",
        "characterFilterIds": [],
        "characterTagFilterMode": "any",
        "characterTagFilters": [],
        "generationTriggerFilterMode": "any",
        "generationTriggerFilters": [],
        "additionalMatchingSources": [],
        "position": position,
        "depth": number(entry.get("depth"), 4),
        "order": number(entry.get("order").or_else(|| entry.get("insertion_order")).or_else(|| entry.get("uid")).or_else(|| entry.get("id")), 100),
        "role": role,
        "sticky": optional_number(entry.get("sticky")),
        "cooldown": optional_number(entry.get("cooldown")),
        "delay": optional_number(entry.get("delay")),
        "ephemeral": optional_number(entry.get("ephemeral")),
        "group": string_field(entry, "group"),
        "groupWeight": optional_number(entry.get("groupWeight")),
        "folderId": Value::Null,
        "preventRecursion": bool_field(entry.get("preventRecursion").or_else(|| entry.get("excludeRecursion")), false),
        "locked": bool_field(entry.get("locked"), false),
        "tag": "",
        "relationships": {},
        "dynamicState": {},
        "activationConditions": [],
        "schedule": Value::Null,
        "excludeFromVectorization": false,
    })
}

fn normalize_imported_lorebook_entry(lorebook_id: &str, entry: &Value, index: usize) -> Value {
    let mut object =
        ensure_object(normalize_lorebook_entry(lorebook_id, entry, index)).unwrap_or_default();
    if let Some(source) = entry.as_object() {
        for (key, value) in source {
            if key != "id" && key != "lorebookId" {
                object.insert(key.clone(), value.clone());
            }
        }
    }

    if !object.contains_key("keys") {
        if let Some(keys) = entry.get("key").or_else(|| entry.get("keys")) {
            object.insert(
                "keys".to_string(),
                Value::Array(
                    string_array(Some(keys))
                        .into_iter()
                        .map(Value::String)
                        .collect(),
                ),
            );
        }
    }
    if !object.contains_key("secondaryKeys") {
        if let Some(keys) = entry
            .get("keysecondary")
            .or_else(|| entry.get("secondary_keys"))
            .or_else(|| entry.get("secondaryKeys"))
        {
            object.insert(
                "secondaryKeys".to_string(),
                Value::Array(
                    string_array(Some(keys))
                        .into_iter()
                        .map(Value::String)
                        .collect(),
                ),
            );
        }
    }
    if let Some(disabled) = entry.get("disable").and_then(Value::as_bool) {
        object.insert("enabled".to_string(), Value::Bool(!disabled));
    }
    if let Some(position) = object.get("position").cloned() {
        let normalized_position = match position {
            Value::String(raw) if raw == "after_char" => Some(1),
            Value::String(raw) if raw == "at_depth" || raw == "depth" => Some(2),
            Value::String(raw) => raw.parse::<i64>().ok(),
            Value::Number(number) => number.as_i64(),
            _ => None,
        };
        if let Some(position) = normalized_position {
            object.insert("position".to_string(), json!(position));
        }
    }
    if !matches!(
        object.get("role").and_then(Value::as_str),
        Some("user" | "assistant" | "system")
    ) {
        object.insert("role".to_string(), Value::String("system".to_string()));
    }
    object.insert(
        "lorebookId".to_string(),
        Value::String(lorebook_id.to_string()),
    );
    for key in [
        "id",
        "key",
        "keysecondary",
        "secondary_keys",
        "disable",
        "uid",
    ] {
        object.remove(key);
    }
    Value::Object(object)
}

fn normalize_lorebook(
    payload: &Value,
    fallback_name: &str,
    character_id: Option<&str>,
) -> (Value, Vec<Value>) {
    let name = payload
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(fallback_name);
    let lorebook = json!({
        "name": name,
        "description": payload.get("description").and_then(Value::as_str).unwrap_or("Imported from SillyTavern"),
        "category": "uncategorized",
        "imagePath": Value::Null,
        "scanDepth": number(payload.get("scan_depth").or_else(|| payload.get("scanDepth")), 2),
        "tokenBudget": number(payload.get("token_budget").or_else(|| payload.get("tokenBudget")), 2048),
        "recursiveScanning": bool_field(payload.get("recursive_scanning").or_else(|| payload.get("recursiveScanning")), false),
        "maxRecursionDepth": number(payload.get("max_recursion_depth").or_else(|| payload.get("maxRecursionDepth")), 3),
        "characterId": Value::Null,
        "characterIds": character_id.map(|id| json!([id])).unwrap_or_else(|| json!([])),
        "personaId": Value::Null,
        "personaIds": [],
        "chatId": Value::Null,
        "isGlobal": false,
        "enabled": true,
        "tags": [],
        "generatedBy": "import",
        "sourceAgentId": Value::Null,
    });
    let entries = lorebook_entries(payload);
    (lorebook, entries)
}

fn create_lorebook_from_payload(
    state: &AppState,
    payload: &Value,
    fallback_name: &str,
    character_id: Option<&str>,
) -> AppResult<Value> {
    let (mut lorebook, entries) = normalize_lorebook(payload, fallback_name, character_id);
    apply_timestamp_overrides(&mut lorebook, &Value::Null, payload);
    let record = state.storage.create("lorebooks", lorebook)?;
    let lorebook_id = record
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    for (index, entry) in entries.iter().enumerate() {
        state.storage.create(
            "lorebook-entries",
            normalize_lorebook_entry(&lorebook_id, entry, index),
        )?;
    }
    Ok(json!({
        "success": true,
        "lorebookId": lorebook_id,
        "name": record.get("name").cloned().unwrap_or(Value::Null),
        "entriesImported": entries.len(),
        "lorebook": record
    }))
}

fn patch_imported_character_lorebook_pointer(
    state: &AppState,
    character_id: &str,
    lorebook_id: &str,
    entries_imported: usize,
) -> AppResult<()> {
    let character = get_required(state, "characters", character_id)?;
    let mut data = character
        .get("data")
        .and_then(Value::as_str)
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .unwrap_or_else(|| json!({}));
    let Some(data_object) = data.as_object_mut() else {
        return Ok(());
    };
    let extensions = data_object
        .entry("extensions".to_string())
        .or_insert_with(|| json!({}));
    let Some(extensions) = extensions.as_object_mut() else {
        return Ok(());
    };
    let import_metadata = extensions
        .entry("importMetadata".to_string())
        .or_insert_with(|| json!({}));
    let Some(import_metadata) = import_metadata.as_object_mut() else {
        return Ok(());
    };
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

fn import_st_character_payload(
    state: &AppState,
    payload: Value,
    filename: Option<String>,
    body: &Value,
) -> AppResult<Value> {
    let tag_mode = body
        .get("tagImportMode")
        .and_then(Value::as_str)
        .unwrap_or("all");
    let existing_tags: Vec<String> = state
        .storage
        .list("characters")?
        .into_iter()
        .flat_map(|row| {
            row.get("data")
                .and_then(Value::as_str)
                .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                .and_then(|data| data.get("tags").cloned())
                .map(|tags| string_array(Some(&tags)))
                .unwrap_or_default()
        })
        .collect();
    let data = normalize_character_data(&payload, tag_mode, &existing_tags);
    let name = data
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Imported Character")
        .to_string();
    let mut record = json!({
        "data": serde_json::to_string(&data)?,
        "comment": data.get("creator_notes").and_then(Value::as_str).unwrap_or(""),
        "avatarPath": payload
            .get("_avatarDataUrl")
            .and_then(Value::as_str)
            .map(|value| Value::String(value.to_string()))
            .unwrap_or(Value::Null),
        "format": payload.get("spec").and_then(Value::as_str).unwrap_or("chara_card_v2"),
    });
    apply_timestamp_overrides(&mut record, body, &payload);
    let character = state.storage.create("characters", record)?;

    let import_embedded = body
        .get("importEmbeddedLorebook")
        .and_then(Value::as_str)
        .map(|raw| raw != "false")
        .unwrap_or_else(|| {
            body.get("importEmbeddedLorebook")
                .and_then(Value::as_bool)
                .unwrap_or(true)
        });
    let embedded = embedded_lorebook(&payload);
    let mut lorebook_result = Value::Null;
    if import_embedded {
        if let Some(book) = embedded.as_ref() {
            let character_id = character.get("id").and_then(Value::as_str);
            lorebook_result = create_lorebook_from_payload(
                state,
                book,
                &format!("{name}'s Lorebook"),
                character_id,
            )?;
            if let (Some(character_id), Some(lorebook_id)) = (
                character_id,
                lorebook_result.get("lorebookId").and_then(Value::as_str),
            ) {
                patch_imported_character_lorebook_pointer(
                    state,
                    character_id,
                    lorebook_id,
                    lorebook_entry_count(book),
                )?;
            }
        }
    }

    Ok(json!({
        "success": true,
        "characterId": character.get("id").cloned().unwrap_or(Value::Null),
        "character": character,
        "name": name,
        "filename": filename,
        "embeddedLorebook": {
            "hasEmbeddedLorebook": embedded.as_ref().map(lorebook_entry_count).unwrap_or(0) > 0,
            "entries": embedded.as_ref().map(lorebook_entry_count).unwrap_or(0),
            "imported": lorebook_result.get("lorebookId").is_some(),
            "skipped": embedded.is_some() && !import_embedded
        },
        "lorebook": lorebook_result
    }))
}

pub(crate) fn import_st_character(state: &AppState, body: Value) -> AppResult<Value> {
    let payload = if body.get("file").is_some() {
        let uploaded = decode_uploaded_file_value(
            body.get("file")
                .ok_or_else(|| AppError::invalid_input("file is required"))?,
        )?;
        parse_character_file(&uploaded.name, &uploaded.bytes)?
    } else {
        body.clone()
    };
    import_st_character_payload(state, payload, None, &body)
}

fn import_st_character_batch(state: &AppState, body: Value) -> AppResult<Value> {
    let files = decode_uploaded_files(&body, "files")?;
    let mut timestamps_by_name: HashMap<String, Vec<Value>> = HashMap::new();
    if let Some(raw_timestamps) = body.get("fileTimestamps").and_then(Value::as_str) {
        if let Ok(Value::Array(entries)) = serde_json::from_str::<Value>(raw_timestamps) {
            for entry in entries {
                let Some(name) = entry.get("name").and_then(Value::as_str) else {
                    continue;
                };
                timestamps_by_name
                    .entry(name.to_string())
                    .or_default()
                    .push(entry.clone());
            }
        }
    }
    let mut results = Vec::new();
    for file in files {
        let filename = file.name.clone();
        let mut file_body = body.clone();
        if let Some(entry) = timestamps_by_name.get_mut(&filename).and_then(|entries| {
            if entries.is_empty() {
                None
            } else {
                Some(entries.remove(0))
            }
        }) {
            if let Some(last_modified) = entry.get("lastModified").cloned() {
                if let Some(object) = file_body.as_object_mut() {
                    object.insert(
                        "timestampOverrides".to_string(),
                        json!({ "createdAt": last_modified, "updatedAt": last_modified }),
                    );
                }
            }
        }
        let result = parse_character_file(&file.name, &file.bytes).and_then(|payload| {
            import_st_character_payload(state, payload, Some(filename.clone()), &file_body)
        });
        match result {
            Ok(mut value) => {
                if let Some(object) = value.as_object_mut() {
                    object.insert("filename".to_string(), Value::String(filename));
                }
                results.push(value);
            }
            Err(error) => results
                .push(json!({ "filename": filename, "success": false, "error": error.message })),
        }
    }
    Ok(json!({ "success": true, "results": results }))
}

fn inspect_st_character_batch(body: Value) -> AppResult<Value> {
    let files = decode_uploaded_files(&body, "files")?;
    let mut results = Vec::new();
    for file in files {
        let filename = file.name.clone();
        match parse_character_file(&file.name, &file.bytes) {
            Ok(payload) => {
                let data = normalize_character_data(&payload, "all", &[]);
                let embedded = embedded_lorebook(&payload);
                results.push(json!({
                    "filename": filename,
                    "success": true,
                    "name": data.get("name").cloned().unwrap_or(Value::Null),
                    "hasEmbeddedLorebook": embedded.as_ref().map(lorebook_entry_count).unwrap_or(0) > 0,
                    "embeddedLorebookEntries": embedded.as_ref().map(lorebook_entry_count).unwrap_or(0)
                }));
            }
            Err(error) => results.push(json!({
                "filename": filename,
                "success": false,
                "hasEmbeddedLorebook": false,
                "embeddedLorebookEntries": 0,
                "error": error.message
            })),
        }
    }
    Ok(json!({ "success": true, "results": results }))
}

fn import_marinara_file(state: &AppState, body: Value) -> AppResult<Value> {
    let uploaded = decode_uploaded_file_value(
        body.get("file")
            .ok_or_else(|| AppError::invalid_input("file is required"))?,
    )?;
    if uploaded.bytes.len() < 4 || uploaded.bytes[0] != 0x50 || uploaded.bytes[1] != 0x4b {
        return Err(AppError::invalid_input(
            "Not a .marinara file (zip signature missing)",
        ));
    }

    let names = read_zip_entry_names(&uploaded.bytes)?;
    const MAX_PACKAGE_ENTRIES: usize = 8;
    const MAX_DATA_JSON_BYTES: usize = 5 * 1024 * 1024;
    const MAX_AVATAR_BYTES: usize = 20 * 1024 * 1024;
    if names.len() > MAX_PACKAGE_ENTRIES {
        return Err(AppError::invalid_input(
            ".marinara file has too many entries",
        ));
    }

    let data_entry = zip_entry_name_case_insensitive(&names, "data.json")
        .ok_or_else(|| AppError::invalid_input(".marinara file is missing data.json"))?;
    let data_bytes = read_zip_entry(&uploaded.bytes, &data_entry)?
        .ok_or_else(|| AppError::invalid_input(".marinara file is missing data.json"))?;
    if data_bytes.len() > MAX_DATA_JSON_BYTES {
        return Err(AppError::invalid_input("data.json in .marinara file is too large"));
    }
    let mut envelope = parse_object(&data_bytes)?;

    let avatar_name = names
        .iter()
        .find(|name| {
            let lower = name.to_ascii_lowercase();
            Path::new(name)
                .file_name()
                .and_then(|value| value.to_str())
                .map(|filename| filename.to_ascii_lowercase().starts_with("avatar."))
                .unwrap_or(false)
                && matches!(
                    lower.rsplit('.').next(),
                    Some("png" | "jpg" | "jpeg" | "webp" | "gif" | "avif")
                )
        })
        .cloned();
    if let Some(avatar_name) = avatar_name {
        let avatar = read_zip_entry(&uploaded.bytes, &avatar_name)?
            .ok_or_else(|| AppError::invalid_input("Could not read .marinara avatar"))?;
        if avatar.len() > MAX_AVATAR_BYTES {
            return Err(AppError::invalid_input(
                "Avatar image in .marinara file is too large",
            ));
        }
        let mime = image_mime_from_path(&avatar_name);
        if let Some(data) = envelope.get_mut("data").and_then(Value::as_object_mut) {
            data.insert(
                "avatar".to_string(),
                Value::String(format!(
                    "data:{mime};base64,{}",
                    general_purpose::STANDARD.encode(avatar)
                )),
            );
        }
    }

    if let Some(timestamp_overrides) = body
        .get("timestampOverrides")
        .cloned()
        .or_else(|| body.get("__timestampOverrides").cloned())
    {
        if let Some(data) = envelope.get_mut("data").and_then(Value::as_object_mut) {
            let metadata = data
                .entry("metadata".to_string())
                .or_insert_with(|| json!({}));
            if let Some(metadata) = metadata.as_object_mut() {
                metadata.insert("timestamps".to_string(), timestamp_overrides);
            }
        }
    }

    import_marinara_envelope(state, envelope)
}

fn data_string_name(record: &Value) -> Option<String> {
    record
        .get("data")
        .and_then(Value::as_str)
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .and_then(|data| {
            data.get("name")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
}

fn data_image_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|value| value.starts_with("data:image/"))
        .map(ToOwned::to_owned)
}

fn remove_import_id(value: &mut Value) {
    if let Some(object) = value.as_object_mut() {
        object.remove("id");
    }
}

fn remove_fields(value: &mut Value, fields: &[&str]) {
    if let Some(object) = value.as_object_mut() {
        for field in fields {
            object.remove(*field);
        }
    }
}

fn hydrate_metadata_timestamps(value: &mut Value) {
    let Some(metadata) = value.get_mut("metadata").and_then(Value::as_object_mut) else {
        return;
    };
    if metadata.contains_key("timestamps") {
        return;
    }
    let created_at = metadata.get("createdAt").cloned();
    let updated_at = metadata.get("updatedAt").cloned();
    if created_at.is_none() && updated_at.is_none() {
        return;
    }
    metadata.insert(
        "timestamps".to_string(),
        json!({
            "createdAt": created_at.unwrap_or(Value::Null),
            "updatedAt": updated_at.unwrap_or(Value::Null)
        }),
    );
}

fn inherit_wrapper_timestamps(record: &mut Value, wrapper: &Value) {
    let Some(timestamps) = wrapper
        .get("metadata")
        .and_then(|metadata| metadata.get("timestamps"))
        .cloned()
    else {
        return;
    };
    let Some(object) = record.as_object_mut() else {
        return;
    };
    let metadata = object
        .entry("metadata".to_string())
        .or_insert_with(|| json!({}));
    if let Some(metadata) = metadata.as_object_mut() {
        metadata.entry("timestamps".to_string()).or_insert(timestamps);
    }
}

fn array_from_envelope(data: &Value, envelope: &Map<String, Value>, key: &str) -> Vec<Value> {
    data.get(key)
        .or_else(|| envelope.get(key))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn extension_from_filename(filename: &str) -> Option<&'static str> {
    match Path::new(filename)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => Some("jpg"),
        "webp" => Some("webp"),
        "gif" => Some("gif"),
        "avif" => Some("avif"),
        "png" => Some("png"),
        "svg" => Some("svg"),
        _ => None,
    }
}

fn import_image_filename(raw: Option<&str>, fallback: &str, ext: &str) -> String {
    let mut filename = raw
        .filter(|value| !value.trim().is_empty())
        .map(safe_filename)
        .unwrap_or_else(|| format!("{}.{}", safe_filename(fallback), ext));
    if Path::new(&filename).extension().is_none() {
        filename.push('.');
        filename.push_str(ext);
    }
    filename
}

fn restore_sprites(state: &AppState, target_id: &str, sprites: Option<&Value>) -> AppResult<usize> {
    let Some(items) = sprites.and_then(Value::as_array) else {
        return Ok(0);
    };
    if items.is_empty() || target_id.contains('/') || target_id.contains('\\') {
        return Ok(0);
    }
    let dir = state.data_dir.join("sprites").join(target_id);
    fs::create_dir_all(&dir)?;
    let mut imported = 0usize;
    for (index, sprite) in items.iter().enumerate() {
        let Some(image) = sprite
            .get("data")
            .or_else(|| sprite.get("url"))
            .and_then(Value::as_str)
            .filter(|value| value.starts_with("data:image/"))
        else {
            continue;
        };
        let (mime, bytes) = decode_image_payload(image, "sprite")?;
        let ext = extension_for_image_mime(&mime)
            .or_else(|| {
                sprite
                    .get("filename")
                    .and_then(Value::as_str)
                    .and_then(extension_from_filename)
            })
            .unwrap_or("png");
        let fallback = sprite
            .get("expression")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("sprite-{}", index + 1));
        let filename = import_image_filename(
            sprite.get("filename").and_then(Value::as_str),
            &fallback,
            ext,
        );
        let target = unique_file_path(&dir.join(filename))?;
        fs::write(target, bytes)?;
        imported += 1;
    }
    Ok(imported)
}

fn restore_character_gallery(
    state: &AppState,
    character_id: &str,
    gallery: Option<&Value>,
) -> AppResult<usize> {
    let Some(items) = gallery.and_then(Value::as_array) else {
        return Ok(0);
    };
    let mut imported = 0usize;
    for (index, item) in items.iter().enumerate() {
        let Some(data_url) = item
            .get("data")
            .or_else(|| item.get("url"))
            .and_then(Value::as_str)
            .filter(|value| value.starts_with("data:image/"))
        else {
            continue;
        };
        let (mime, _) = decode_image_payload(data_url, "gallery image")?;
        let ext = extension_for_image_mime(&mime).unwrap_or("png");
        let filename = import_image_filename(
            item.get("filename").and_then(Value::as_str),
            &format!("gallery-{}", index + 1),
            ext,
        );
        state.storage.create(
            "character-gallery",
            json!({
                "characterId": character_id,
                "filePath": filename,
                "filename": filename,
                "url": data_url,
                "prompt": item.get("prompt").cloned().unwrap_or_else(|| json!("")),
                "provider": item.get("provider").cloned().unwrap_or_else(|| json!("")),
                "model": item.get("model").cloned().unwrap_or_else(|| json!("")),
                "width": item.get("width").cloned().unwrap_or(Value::Null),
                "height": item.get("height").cloned().unwrap_or(Value::Null)
            }),
        )?;
        imported += 1;
    }
    Ok(imported)
}

fn import_marinara_character(state: &AppState, data: Value) -> AppResult<Value> {
    if data.get("spec").is_some() && data.get("data").is_some_and(Value::is_object) {
        let mut character_data = data.get("data").cloned().unwrap_or_else(|| json!({}));
        strip_stale_embedded_lorebook_pointer(&mut character_data);
        let mut record = json!({
            "data": serde_json::to_string(&character_data)?,
            "comment": data
                .get("metadata")
                .and_then(|metadata| metadata.get("comment"))
                .and_then(Value::as_str)
                .unwrap_or(""),
            "avatarPath": data_image_string(data.get("avatar")).map(Value::String).unwrap_or(Value::Null),
            "format": data.get("spec").and_then(Value::as_str).unwrap_or("chara_card_v2"),
        });
        if let Some(avatar) = data_image_string(data.get("avatar")) {
            if let Some(object) = record.as_object_mut() {
                object.insert("avatar".to_string(), Value::String(avatar));
            }
        }
        let mut timestamp_payload = data.clone();
        hydrate_metadata_timestamps(&mut timestamp_payload);
        apply_timestamp_overrides(&mut record, &Value::Null, &timestamp_payload);
        let character = state.storage.create("characters", record)?;
        let character_id = character
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::new("storage_error", "Created character is missing an id"))?
            .to_string();
        let sprites_imported = restore_sprites(state, &character_id, data.get("sprites"))?;
        let gallery_imported =
            restore_character_gallery(state, &character_id, data.get("gallery"))?;
        let name = character_data
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Imported Character")
            .to_string();
        return Ok(json!({
            "success": true,
            "type": "marinara_character",
            "id": character_id,
            "characterId": character_id,
            "name": name,
            "character": character,
            "spritesImported": sprites_imported,
            "galleryImported": gallery_imported
        }));
    }

    let looks_like_storage_record = data.get("data").is_some()
        || data.get("format").is_some()
        || data.get("avatarPath").is_some();
    if !looks_like_storage_record {
        return import_st_character_payload(state, data, None, &Value::Null);
    }

    let mut source = data.clone();
    remove_fields(&mut source, &["id", "sprites", "gallery", "metadata"]);
    let mut record_value = with_entity_defaults("characters", source.clone());
    if let Some(avatar) = data.get("avatar").and_then(Value::as_str) {
        if let Some(record) = record_value.as_object_mut() {
            record.insert("avatarPath".to_string(), Value::String(avatar.to_string()));
            record.insert("avatar".to_string(), Value::String(avatar.to_string()));
        }
    }
    let mut timestamp_payload = data.clone();
    hydrate_metadata_timestamps(&mut timestamp_payload);
    apply_timestamp_overrides(&mut record_value, &Value::Null, &timestamp_payload);
    let record = state.storage.create("characters", record_value)?;
    let character_id = record
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::new("storage_error", "Created character is missing an id"))?
        .to_string();
    let sprites_imported = restore_sprites(state, &character_id, data.get("sprites"))?;
    let gallery_imported = restore_character_gallery(state, &character_id, data.get("gallery"))?;
    let name = data_string_name(&record)
        .or_else(|| {
            record
                .get("name")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "Imported Character".to_string());
    Ok(json!({
        "success": true,
        "type": "marinara_character",
        "id": record.get("id").cloned().unwrap_or(Value::Null),
        "characterId": record.get("id").cloned().unwrap_or(Value::Null),
        "name": name,
        "character": record,
        "spritesImported": sprites_imported,
        "galleryImported": gallery_imported
    }))
}

fn import_marinara_persona(state: &AppState, data: Value) -> AppResult<Value> {
    let mut source = data.clone();
    remove_fields(&mut source, &["id", "metadata", "avatar", "sprites"]);
    let mut record_value = with_entity_defaults("personas", source);
    if let Some(avatar) = data.get("avatar").and_then(Value::as_str) {
        if let Some(record) = record_value.as_object_mut() {
            record.insert("avatarPath".to_string(), Value::String(avatar.to_string()));
            record.insert("avatar".to_string(), Value::String(avatar.to_string()));
        }
    }
    let mut timestamp_payload = data.clone();
    hydrate_metadata_timestamps(&mut timestamp_payload);
    apply_timestamp_overrides(&mut record_value, &Value::Null, &timestamp_payload);
    let record = state.storage.create("personas", record_value)?;
    let persona_id = record
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::new("storage_error", "Created persona is missing an id"))?
        .to_string();
    let sprites_imported = restore_sprites(state, &persona_id, data.get("sprites"))?;
    Ok(json!({
        "success": true,
        "type": "marinara_persona",
        "id": record.get("id").cloned().unwrap_or(Value::Null),
        "name": record.get("name").cloned().unwrap_or(Value::Null),
        "spritesImported": sprites_imported
    }))
}

fn import_marinara_lorebook(
    state: &AppState,
    envelope: &Map<String, Value>,
    data: Value,
) -> AppResult<Value> {
    let mut lorebook_data = data
        .get("lorebook")
        .cloned()
        .unwrap_or_else(|| data.clone());
    inherit_wrapper_timestamps(&mut lorebook_data, &data);
    remove_import_id(&mut lorebook_data);
    remove_fields(&mut lorebook_data, &["entries", "folders"]);
    let mut lorebook = with_entity_defaults("lorebooks", lorebook_data.clone());
    if let Some(image) = data
        .get("avatar")
        .or_else(|| data.get("image"))
        .and_then(Value::as_str)
    {
        if let Some(record) = lorebook.as_object_mut() {
            record.insert("imagePath".to_string(), Value::String(image.to_string()));
        }
    }
    let mut timestamp_payload = lorebook_data.clone();
    hydrate_metadata_timestamps(&mut timestamp_payload);
    apply_timestamp_overrides(&mut lorebook, &Value::Null, &timestamp_payload);
    let record = state.storage.create("lorebooks", lorebook)?;
    let lorebook_id = record
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let mut folder_id_map: HashMap<String, String> = HashMap::new();
    let mut pending_folder_parents: Vec<(String, String)> = Vec::new();
    for folder in array_from_envelope(&data, envelope, "folders") {
        let old_id = folder
            .get("id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let old_parent_id = folder
            .get("parentFolderId")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let mut folder_record = ensure_object(folder)?;
        folder_record.remove("id");
        folder_record.remove("lorebookId");
        folder_record.insert("lorebookId".to_string(), Value::String(lorebook_id.clone()));
        if old_parent_id.is_some() {
            folder_record.insert("parentFolderId".to_string(), Value::Null);
        }
        let created = state
            .storage
            .create("lorebook-folders", Value::Object(folder_record))?;
        if let (Some(old_id), Some(new_id)) = (
            old_id,
            created
                .get("id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
        ) {
            folder_id_map.insert(old_id, new_id);
        }
        if let (Some(new_id), Some(old_parent_id)) = (
            created
                .get("id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            old_parent_id,
        ) {
            pending_folder_parents.push((new_id, old_parent_id));
        }
    }
    for (folder_id, old_parent_id) in pending_folder_parents {
        if let Some(new_parent_id) = folder_id_map.get(&old_parent_id) {
            state.storage.patch(
                "lorebook-folders",
                &folder_id,
                json!({ "parentFolderId": new_parent_id }),
            )?;
        }
    }

    let mut exported_entries = array_from_envelope(&data, envelope, "entries");
    if exported_entries.is_empty() {
        exported_entries = lorebook_entries(&data);
    }
    for (index, entry) in exported_entries.iter().enumerate() {
        let mut normalized = normalize_imported_lorebook_entry(&lorebook_id, entry, index);
        if let Some(old_folder_id) = entry.get("folderId").and_then(Value::as_str) {
            if let Some(object) = normalized.as_object_mut() {
                object.insert(
                    "folderId".to_string(),
                    folder_id_map
                        .get(old_folder_id)
                        .map(|id| Value::String(id.clone()))
                        .unwrap_or(Value::Null),
                );
            }
        }
        state.storage.create("lorebook-entries", normalized)?;
    }

    Ok(json!({
        "success": true,
        "type": "marinara_lorebook",
        "id": lorebook_id,
        "lorebookId": lorebook_id,
        "name": record.get("name").cloned().unwrap_or(Value::Null),
        "entriesImported": exported_entries.len(),
        "foldersImported": folder_id_map.len(),
        "lorebook": record
    }))
}

fn import_marinara_preset(
    state: &AppState,
    envelope: &Map<String, Value>,
    data: Value,
) -> AppResult<Value> {
    let mut preset_data = data.get("preset").cloned().unwrap_or_else(|| data.clone());
    inherit_wrapper_timestamps(&mut preset_data, &data);
    remove_import_id(&mut preset_data);
    remove_fields(
        &mut preset_data,
        &["sections", "groups", "choiceBlocks", "variables"],
    );
    let mut record_value = with_entity_defaults("prompts", preset_data.clone());
    let mut timestamp_payload = preset_data.clone();
    hydrate_metadata_timestamps(&mut timestamp_payload);
    apply_timestamp_overrides(&mut record_value, &Value::Null, &timestamp_payload);
    let record = state.storage.create("prompts", record_value)?;
    let preset_id = record
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::new("storage_error", "Created preset is missing an id"))?
        .to_string();

    let mut group_id_map: HashMap<String, String> = HashMap::new();
    let mut pending_group_parents: Vec<(String, String)> = Vec::new();
    for group in array_from_envelope(&data, envelope, "groups") {
        let old_id = group
            .get("id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let old_parent_id = group
            .get("parentGroupId")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let mut group_record = ensure_object(group)?;
        group_record.remove("id");
        group_record.remove("presetId");
        group_record.insert("presetId".to_string(), Value::String(preset_id.clone()));
        if old_parent_id.is_some() {
            group_record.insert("parentGroupId".to_string(), Value::Null);
        }
        let created = state
            .storage
            .create("prompt-groups", Value::Object(group_record))?;
        if let (Some(old_id), Some(new_id)) = (
            old_id,
            created
                .get("id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
        ) {
            group_id_map.insert(old_id, new_id);
        }
        if let (Some(new_id), Some(old_parent_id)) = (
            created
                .get("id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            old_parent_id,
        ) {
            pending_group_parents.push((new_id, old_parent_id));
        }
    }
    for (group_id, old_parent_id) in pending_group_parents {
        if let Some(new_parent_id) = group_id_map.get(&old_parent_id) {
            state.storage.patch(
                "prompt-groups",
                &group_id,
                json!({ "parentGroupId": new_parent_id }),
            )?;
        }
    }

    let mut sections_imported = 0usize;
    for section in array_from_envelope(&data, envelope, "sections") {
        let mut section_record = ensure_object(section)?;
        section_record.remove("id");
        section_record.remove("presetId");
        section_record.insert("presetId".to_string(), Value::String(preset_id.clone()));
        if let Some(old_group_id) = section_record.get("groupId").and_then(Value::as_str) {
            if let Some(new_group_id) = group_id_map.get(old_group_id) {
                section_record.insert("groupId".to_string(), Value::String(new_group_id.clone()));
            }
        }
        state
            .storage
            .create("prompt-sections", Value::Object(section_record))?;
        sections_imported += 1;
    }

    let mut variables_imported = 0usize;
    let mut variables = array_from_envelope(&data, envelope, "choiceBlocks");
    if variables.is_empty() {
        variables = array_from_envelope(&data, envelope, "variables");
    }
    for variable in variables {
        let mut variable_record = ensure_object(variable)?;
        variable_record.remove("id");
        variable_record.remove("presetId");
        variable_record.insert("presetId".to_string(), Value::String(preset_id.clone()));
        state
            .storage
            .create("prompt-variables", Value::Object(variable_record))?;
        variables_imported += 1;
    }

    Ok(json!({
        "success": true,
        "type": "marinara_preset",
        "id": preset_id,
        "name": record.get("name").cloned().unwrap_or(Value::Null),
        "preset": record,
        "groupsImported": group_id_map.len(),
        "sectionsImported": sections_imported,
        "variablesImported": variables_imported
    }))
}

fn import_marinara_envelope(state: &AppState, envelope: Value) -> AppResult<Value> {
    let object = envelope
        .as_object()
        .ok_or_else(|| AppError::invalid_input("Invalid Marinara import envelope"))?;
    if object.get("version").and_then(Value::as_i64) != Some(1) {
        return Err(AppError::invalid_input(
            "Unsupported Marinara import version",
        ));
    }
    let import_type = object.get("type").and_then(Value::as_str).unwrap_or("");
    let mut data = object.get("data").cloned().unwrap_or(Value::Null);
    hydrate_metadata_timestamps(&mut data);
    if let Some((created_at, updated_at)) = timestamp_overrides_from_value(
        object
            .get("timestampOverrides")
            .or_else(|| object.get("__timestampOverrides")),
    ) {
        if let Some(data_object) = data.as_object_mut() {
            let metadata = data_object
                .entry("metadata".to_string())
                .or_insert_with(|| json!({}));
            if let Some(metadata_object) = metadata.as_object_mut() {
                metadata_object.insert(
                    "timestamps".to_string(),
                    json!({ "createdAt": created_at, "updatedAt": updated_at }),
                );
            }
        }
    }
    match import_type {
        "marinara_character" => import_marinara_character(state, data),
        "marinara_persona" => import_marinara_persona(state, data),
        "marinara_lorebook" => import_marinara_lorebook(state, object, data),
        "marinara_preset" => import_marinara_preset(state, object, data),
        _ => Err(AppError::invalid_input(format!(
            "Unknown Marinara import type: {import_type}"
        ))),
    }
}

pub(crate) fn import_call(state: &AppState, rest: &[&str], body: Value) -> AppResult<Value> {
    match rest {
        ["marinara"] => {
            let payload = import_payload(body)?;
            import_marinara_envelope(state, payload)
        }
        ["marinara-file"] => import_marinara_file(state, body),
        ["st-character"] => import_st_character(state, body),
        ["st-character", "batch"] => import_st_character_batch(state, body),
        ["st-character", "inspect"] => inspect_st_character_batch(body),
        ["st-chat"] => bulk_imports::import_st_chat(state, body),
        ["st-chat-into-group"] => bulk_imports::import_st_chat_into_group(state, body),
        ["st-preset"] => state
            .storage
            .create(
                "prompts",
                with_entity_defaults("prompts", import_payload(body)?),
            )
            .map(|record| json!({ "success": true, "preset": record })),
        ["st-lorebook"] => {
            let payload = import_payload(body)?;
            create_lorebook_from_payload(
                state,
                &payload,
                payload
                    .get("__filename")
                    .and_then(Value::as_str)
                    .unwrap_or("Imported Lorebook"),
                None,
            )
        }
        ["list-directory"] => {
            let path = body.get("path").and_then(Value::as_str).unwrap_or("");
            let base = if path.trim().is_empty() {
                std::env::var("USERPROFILE")
                    .or_else(|_| std::env::var("HOME"))
                    .unwrap_or_else(|_| ".".to_string())
            } else {
                path.to_string()
            };
            let resolved = PathBuf::from(base);
            directory_listing(resolved)
        }
        ["st-bulk", "scan"] => bulk_imports::scan_st_folder(body),
        ["st-bulk", "run"] => bulk_imports::run_st_bulk_import(state, body),
        _ => Err(AppError::new(
            "route_not_found",
            format!("Unknown import route: /{}", rest.join("/")),
        )),
    }
}

pub(crate) fn import_stream_events(
    state: &AppState,
    rest: &[&str],
    body: Value,
) -> AppResult<Vec<Value>> {
    match rest {
        ["st-bulk", "run"] | ["st-bulk", "run-stream"] => {
            bulk_imports::run_st_bulk_import_events(state, body)
        }
        _ => Err(AppError::new(
            "stream_not_supported",
            format!("Streaming is not supported for /import/{}", rest.join("/")),
        )),
    }
}
