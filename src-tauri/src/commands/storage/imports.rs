use super::shared::*;
use super::*;
mod bulk_imports;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

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
        Some(Value::String(raw)) if !raw.trim().is_empty() => vec![raw.to_string()],
        _ => Vec::new(),
    }
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
    wrapped
        .get("character_book")
        .filter(|book| lorebook_entry_count(book) > 0)
        .cloned()
        .or_else(|| {
            payload
                .get("character_book")
                .filter(|book| lorebook_entry_count(book) > 0)
                .cloned()
        })
}

fn normalize_character_data(payload: &Value, tag_mode: &str, existing_tags: &[String]) -> Value {
    let data = source_character_data(payload);
    let mut tags = string_array(data.get("tags"));
    if tag_mode == "none" {
        tags.clear();
    } else if tag_mode == "existing" {
        let keys: Vec<String> = existing_tags.iter().map(|tag| tag.to_lowercase()).collect();
        tags.retain(|tag| keys.contains(&tag.to_lowercase()));
    }
    json!({
        "name": data.get("name").or_else(|| payload.get("char_name")).and_then(Value::as_str).unwrap_or("Imported Character"),
        "description": data.get("description").or_else(|| payload.get("char_persona")).and_then(Value::as_str).unwrap_or(""),
        "personality": string_field(&data, "personality"),
        "scenario": data.get("scenario").or_else(|| payload.get("world_scenario")).and_then(Value::as_str).unwrap_or(""),
        "first_mes": data.get("first_mes").or_else(|| payload.get("char_greeting")).and_then(Value::as_str).unwrap_or(""),
        "mes_example": data.get("mes_example").or_else(|| payload.get("example_dialogue")).and_then(Value::as_str).unwrap_or(""),
        "creator_notes": string_field(&data, "creator_notes"),
        "system_prompt": string_field(&data, "system_prompt"),
        "post_history_instructions": string_field(&data, "post_history_instructions"),
        "tags": tags,
        "creator": string_field(&data, "creator"),
        "character_version": data.get("character_version").and_then(Value::as_str).unwrap_or("1.0"),
        "alternate_greetings": string_array(data.get("alternate_greetings")),
        "extensions": data.get("extensions").filter(|value| value.is_object()).cloned().unwrap_or_else(|| json!({ "altDescriptions": [] })),
        "character_book": embedded_lorebook(payload).unwrap_or(Value::Null),
    })
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
    let (lorebook, entries) = normalize_lorebook(payload, fallback_name, character_id);
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
    let record = json!({
        "data": serde_json::to_string(&data)?,
        "comment": data.get("creator_notes").and_then(Value::as_str).unwrap_or(""),
        "avatarPath": payload
            .get("_avatarDataUrl")
            .and_then(Value::as_str)
            .map(|value| Value::String(value.to_string()))
            .unwrap_or(Value::Null),
        "format": payload.get("spec").and_then(Value::as_str).unwrap_or("chara_card_v2"),
    });
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
    let mut results = Vec::new();
    for file in files {
        let filename = file.name.clone();
        let result = parse_character_file(&file.name, &file.bytes).and_then(|payload| {
            import_st_character_payload(state, payload, Some(filename.clone()), &body)
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

fn import_marinara_package(state: &AppState, body: Value) -> AppResult<Value> {
    let uploaded = decode_uploaded_file_value(
        body.get("file")
            .ok_or_else(|| AppError::invalid_input("file is required"))?,
    )?;
    if uploaded.bytes.len() < 4 || uploaded.bytes[0] != 0x50 || uploaded.bytes[1] != 0x4b {
        return Err(AppError::invalid_input(
            "Not a .marinara package (zip signature missing)",
        ));
    }

    let names = read_zip_entry_names(&uploaded.bytes)?;
    const MAX_PACKAGE_ENTRIES: usize = 8;
    const MAX_DATA_JSON_BYTES: usize = 5 * 1024 * 1024;
    const MAX_AVATAR_BYTES: usize = 20 * 1024 * 1024;
    if names.len() > MAX_PACKAGE_ENTRIES {
        return Err(AppError::invalid_input(
            ".marinara package has too many entries",
        ));
    }

    let data_bytes = read_zip_entry(&uploaded.bytes, "data.json")?
        .ok_or_else(|| AppError::invalid_input(".marinara package is missing data.json"))?;
    if data_bytes.len() > MAX_DATA_JSON_BYTES {
        return Err(AppError::invalid_input("data.json in package is too large"));
    }
    let mut envelope = parse_object(&data_bytes)?;

    let avatar_name = names
        .iter()
        .find(|name| {
            let lower = name.to_ascii_lowercase();
            lower.starts_with("avatar.")
                && matches!(
                    lower.rsplit('.').next(),
                    Some("png" | "jpg" | "jpeg" | "webp" | "gif" | "avif")
                )
        })
        .cloned();
    if let Some(avatar_name) = avatar_name {
        let avatar = read_zip_entry(&uploaded.bytes, &avatar_name)?
            .ok_or_else(|| AppError::invalid_input("Could not read package avatar"))?;
        if avatar.len() > MAX_AVATAR_BYTES {
            return Err(AppError::invalid_input(
                "Avatar image in package is too large",
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
    let data = object.get("data").cloned().unwrap_or(Value::Null);
    match import_type {
        "marinara_character" => import_st_character_payload(state, data, None, &Value::Null),
        "marinara_persona" => {
            let record = state
                .storage
                .create("personas", with_entity_defaults("personas", data))?;
            Ok(
                json!({ "success": true, "type": import_type, "id": record.get("id").cloned().unwrap_or(Value::Null), "name": record.get("name").cloned().unwrap_or(Value::Null) }),
            )
        }
        "marinara_lorebook" => {
            create_lorebook_from_payload(state, &data, "Imported Lorebook", None)
        }
        "marinara_preset" => {
            let record = state
                .storage
                .create("prompts", with_entity_defaults("prompts", data))?;
            Ok(
                json!({ "success": true, "type": import_type, "id": record.get("id").cloned().unwrap_or(Value::Null), "name": record.get("name").cloned().unwrap_or(Value::Null) }),
            )
        }
        _ => Err(AppError::invalid_input(format!(
            "Unknown Marinara import type: {import_type}"
        ))),
    }
}

pub(crate) fn import_call(state: &AppState, rest: &[&str], body: Value) -> AppResult<Value> {
    match rest {
        ["marinara"] => {
            let payload = import_payload(body)?;
            if let Some(collections) = payload.get("collections").and_then(Value::as_object) {
                for (collection, rows) in collections {
                    if BACKUP_COLLECTIONS.contains(&collection.as_str()) {
                        if let Some(rows) = rows.as_array() {
                            state.storage.replace_all(collection, rows.clone())?;
                        }
                    }
                }
                return Ok(json!({ "success": true }));
            }
            import_marinara_envelope(state, payload)
        }
        ["marinara-package"] => import_marinara_package(state, body),
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
        ["pick-folder"] => match rfd::FileDialog::new().pick_folder() {
            Some(path) => directory_listing(path),
            None => Ok(json!({ "success": false, "cancelled": true })),
        },
        ["st-bulk", "scan"] => bulk_imports::scan_st_folder(body),
        ["st-bulk", "run"] => bulk_imports::run_st_bulk_import(state, body),
        _ => Err(AppError::new(
            "route_not_found",
            format!("Unknown import route: /{}", rest.join("/")),
        )),
    }
}
