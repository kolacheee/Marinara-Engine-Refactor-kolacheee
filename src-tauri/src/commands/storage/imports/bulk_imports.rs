use super::*;

fn bool_option(value: Option<&Value>) -> Option<bool> {
    match value {
        Some(Value::Bool(value)) => Some(*value),
        Some(Value::Number(value)) => value.as_i64().map(|value| value != 0),
        Some(Value::String(raw)) => match raw.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "y" | "on" => Some(true),
            "false" | "0" | "no" | "n" | "off" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn selected_ids(options: &Value, key: &str) -> Vec<String> {
    options
        .get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn resolve_st_data_dir(root: &Path) -> Option<PathBuf> {
    let default_user = root.join("data").join("default-user");
    if default_user.join("characters").is_dir() {
        return Some(default_user);
    }
    let data_parent = root.join("data");
    if let Ok(entries) = fs::read_dir(&data_parent) {
        for entry in entries.filter_map(Result::ok) {
            let candidate = entry.path();
            if candidate.is_dir() && candidate.join("characters").is_dir() {
                return Some(candidate);
            }
        }
    }
    let public = root.join("public");
    if public.join("characters").is_dir() {
        return Some(public);
    }
    if root.join("characters").is_dir() {
        return Some(root.to_path_buf());
    }
    None
}

fn path_id(category: &str, data_dir: &Path, path: &Path) -> String {
    let relative = path
        .strip_prefix(data_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    format!("{category}:{relative}")
}

fn path_from_id(data_dir: &Path, category: &str, id: &str) -> AppResult<PathBuf> {
    let prefix = format!("{category}:");
    let relative = id
        .strip_prefix(&prefix)
        .ok_or_else(|| AppError::invalid_input(format!("Invalid {category} import id")))?;
    if relative.contains("..") {
        return Err(AppError::invalid_input(
            "Import id must not contain parent path segments",
        ));
    }
    Ok(data_dir.join(relative))
}

fn list_files(dir: &Path, extensions: &[&str], recursive: bool) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if !dir.is_dir() {
        return files;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return files;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() && recursive {
            files.extend(list_files(&path, extensions, true));
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| format!(".{}", ext.to_ascii_lowercase()))
            .unwrap_or_default();
        if extensions.iter().any(|allowed| *allowed == ext) {
            files.push(path);
        }
    }
    files.sort();
    files
}

fn scan_item(category: &str, data_dir: &Path, path: &Path) -> Value {
    json!({
        "id": path_id(category, data_dir, path),
        "path": path.to_string_lossy(),
        "name": file_stem(path),
        "modifiedAt": modified_at(path),
    })
}

pub(super) fn scan_st_folder(body: Value) -> AppResult<Value> {
    let root = body
        .get("folderPath")
        .and_then(Value::as_str)
        .filter(|path| !path.trim().is_empty())
        .ok_or_else(|| AppError::invalid_input("folderPath is required"))?;
    let root = PathBuf::from(root);
    if !root.exists() {
        return Ok(json!({
            "success": false,
            "error": "Folder does not exist",
            "characters": [],
            "chats": [],
            "groupChats": [],
            "presets": [],
            "lorebooks": [],
            "backgrounds": [],
            "personas": []
        }));
    }
    let Some(data_dir) = resolve_st_data_dir(&root) else {
        return Ok(json!({
            "success": false,
            "error": "Could not find SillyTavern data directory. Make sure the path points to your SillyTavern installation folder.",
            "characters": [],
            "chats": [],
            "groupChats": [],
            "presets": [],
            "lorebooks": [],
            "backgrounds": [],
            "personas": []
        }));
    };

    let characters: Vec<Value> = list_files(
        &data_dir.join("characters"),
        &[".json", ".png", ".charx"],
        false,
    )
    .into_iter()
    .map(|path| {
        let mut item = scan_item("characters", &data_dir, &path);
        if let Some(object) = item.as_object_mut() {
            object.insert(
                "format".to_string(),
                Value::String(
                    path.extension()
                        .and_then(|ext| ext.to_str())
                        .unwrap_or("json")
                        .to_ascii_lowercase(),
                ),
            );
        }
        item
    })
    .collect();
    let chats: Vec<Value> = list_files(&data_dir.join("chats"), &[".jsonl"], true)
        .into_iter()
        .map(|path| {
            let mut item = scan_item("chats", &data_dir, &path);
            if let Some(object) = item.as_object_mut() {
                let folder_name = path
                    .parent()
                    .and_then(|path| path.file_name())
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default();
                object.insert("folderName".to_string(), Value::String(folder_name.clone()));
                object.insert("characterName".to_string(), Value::String(folder_name));
                object.insert("chatName".to_string(), Value::String(file_stem(&path)));
            }
            item
        })
        .collect();
    let group_chats: Vec<Value> =
        list_files(&data_dir.join("group chats"), &[".jsonl", ".json"], true)
            .into_iter()
            .map(|path| {
                let mut item = scan_item("groupChats", &data_dir, &path);
                if let Some(object) = item.as_object_mut() {
                    object.insert("groupName".to_string(), Value::String(file_stem(&path)));
                    object.insert("members".to_string(), json!([]));
                }
                item
            })
            .collect();
    let presets: Vec<Value> = list_files(&data_dir.join("presets"), &[".json"], false)
        .into_iter()
        .map(|path| {
            let mut item = scan_item("presets", &data_dir, &path);
            if let Some(object) = item.as_object_mut() {
                let name = file_stem(&path).to_ascii_lowercase();
                object.insert(
                    "isBuiltin".to_string(),
                    Value::Bool(matches!(
                        name.as_str(),
                        "default"
                            | "deterministic"
                            | "neutral"
                            | "universal-creative"
                            | "universal-light"
                            | "universal-super-creative"
                    )),
                );
            }
            item
        })
        .collect();
    let mut lorebook_files = list_files(&data_dir.join("worlds"), &[".json"], false);
    lorebook_files.extend(list_files(&data_dir.join("world-info"), &[".json"], false));
    lorebook_files.sort();
    lorebook_files.dedup();
    let lorebooks: Vec<Value> = lorebook_files
        .into_iter()
        .map(|path| scan_item("lorebooks", &data_dir, &path))
        .collect();
    let backgrounds: Vec<Value> = list_files(
        &data_dir.join("backgrounds"),
        &[".jpg", ".jpeg", ".png", ".gif", ".webp", ".avif"],
        true,
    )
    .into_iter()
    .map(|path| scan_item("backgrounds", &data_dir, &path))
    .collect();
    let mut persona_files = list_files(&data_dir.join("personas"), &[".json", ".txt"], false);
    persona_files.extend(list_files(
        &data_dir.join("User Avatars"),
        &[".json", ".txt"],
        false,
    ));
    persona_files.sort();
    persona_files.dedup();
    let personas: Vec<Value> = persona_files
        .into_iter()
        .map(|path| {
            let mut item = scan_item("personas", &data_dir, &path);
            if let Some(object) = item.as_object_mut() {
                object.insert("description".to_string(), Value::String(String::new()));
            }
            item
        })
        .collect();

    Ok(json!({
        "success": true,
        "dataDir": data_dir.to_string_lossy(),
        "characters": characters,
        "chats": chats,
        "groupChats": group_chats,
        "presets": presets,
        "lorebooks": lorebooks,
        "backgrounds": backgrounds,
        "personas": personas,
    }))
}

fn import_st_chat_text(
    state: &AppState,
    text: &str,
    chat_name: String,
    inherited: Option<Value>,
) -> AppResult<Value> {
    let mut character_name = String::new();
    let mut parsed_rows = Vec::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let parsed = match parse_json_text(line) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if character_name.is_empty() {
            if let Some(name) = parsed.get("character_name").and_then(Value::as_str) {
                character_name = name.to_string();
            }
        }
        parsed_rows.push(parsed);
    }
    let mut chat = ensure_object(inherited.unwrap_or_else(|| json!({})))?;
    chat.remove("id");
    chat.insert("name".to_string(), Value::String(chat_name));
    chat.entry("mode".to_string())
        .or_insert(Value::String("chat".to_string()));
    chat.entry("characterIds".to_string())
        .or_insert_with(|| json!([]));
    chat.entry("metadata".to_string())
        .or_insert_with(|| json!({}));
    if !character_name.is_empty() {
        chat.entry("importedCharacterName".to_string())
            .or_insert(Value::String(character_name));
    }
    let chat_record = state.storage.create("chats", Value::Object(chat))?;
    let chat_id = chat_record
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let mut imported = 0usize;
    for row in parsed_rows {
        if row
            .get("is_system")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let content = row
            .get("mes")
            .or_else(|| row.get("content"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if content.trim().is_empty() {
            continue;
        }
        let role = if row.get("is_user").and_then(Value::as_bool).unwrap_or(false) {
            "user"
        } else {
            "assistant"
        };
        state.storage.create(
            "messages",
            json!({
                "chatId": chat_id,
                "role": role,
                "content": content,
                "characterId": Value::Null,
                "extra": {},
                "activeSwipeIndex": 0,
                "swipes": [{ "content": content }]
            }),
        )?;
        imported += 1;
    }
    Ok(
        json!({ "success": true, "chatId": chat_id, "chat": chat_record, "messagesImported": imported }),
    )
}

pub(super) fn import_st_chat(state: &AppState, body: Value) -> AppResult<Value> {
    let uploaded = decode_uploaded_file_value(
        body.get("file")
            .ok_or_else(|| AppError::invalid_input("file is required"))?,
    )?;
    let text = String::from_utf8(uploaded.bytes)
        .map_err(|_| AppError::invalid_input("Chat import file must be UTF-8 JSONL"))?;
    let chat_name = Path::new(&uploaded.name)
        .file_stem()
        .map(|name| name.to_string_lossy().replace('_', " "))
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "Imported Chat".to_string());
    import_st_chat_text(state, &text, chat_name, None)
}

pub(super) fn import_st_chat_into_group(state: &AppState, body: Value) -> AppResult<Value> {
    let target_chat_id = required_string(&body, "chatId")?;
    let target = get_required(state, "chats", target_chat_id)?;
    let uploaded = decode_uploaded_file_value(
        body.get("file")
            .ok_or_else(|| AppError::invalid_input("file is required"))?,
    )?;
    let text = String::from_utf8(uploaded.bytes)
        .map_err(|_| AppError::invalid_input("Chat import file must be UTF-8 JSONL"))?;
    let mut inherited = target.clone();
    if let Some(object) = inherited.as_object_mut() {
        let group_id = object
            .get("groupId")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .unwrap_or_else(new_id);
        object.insert("groupId".to_string(), Value::String(group_id.clone()));
        state
            .storage
            .patch("chats", target_chat_id, json!({ "groupId": group_id }))?;
    }
    let branch_name = Path::new(&uploaded.name)
        .file_stem()
        .map(|name| name.to_string_lossy().replace('_', " "))
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "Imported".to_string());
    import_st_chat_text(state, &text, branch_name, Some(inherited))
}

fn import_persona_payload(
    state: &AppState,
    payload: Value,
    fallback_name: &str,
) -> AppResult<Value> {
    let mut object = ensure_object(payload).unwrap_or_default();
    object
        .entry("name".to_string())
        .or_insert(Value::String(fallback_name.to_string()));
    if !object.contains_key("description") {
        if let Some(persona) = object
            .get("persona")
            .or_else(|| object.get("content"))
            .and_then(Value::as_str)
        {
            object.insert(
                "description".to_string(),
                Value::String(persona.to_string()),
            );
        }
    }
    state
        .storage
        .create("personas", with_entity_defaults("personas", Value::Object(object)))
        .map(|record| json!({ "success": true, "id": record.get("id").cloned().unwrap_or(Value::Null), "name": record.get("name").cloned().unwrap_or(Value::Null), "persona": record }))
}

fn import_persona_file(state: &AppState, path: &Path) -> AppResult<Value> {
    let raw = fs::read_to_string(path)?;
    let fallback_name = file_stem(path);
    let payload = parse_json_text(&raw)
        .unwrap_or_else(|_| json!({ "name": fallback_name, "description": raw }));
    import_persona_payload(state, payload, &fallback_name)
}

fn copy_background_file(state: &AppState, path: &Path) -> AppResult<Value> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .ok_or_else(|| AppError::invalid_input("Background file is missing a filename"))?;
    let target = state.backgrounds.root().join(&name);
    let mut final_target = target.clone();
    if final_target.exists() {
        let stem = Path::new(&name)
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_else(|| "background".to_string());
        let ext = Path::new(&name)
            .extension()
            .map(|ext| format!(".{}", ext.to_string_lossy()))
            .unwrap_or_default();
        for index in 1..10_000 {
            let candidate = state
                .backgrounds
                .root()
                .join(format!("{stem}-{index}{ext}"));
            if !candidate.exists() {
                final_target = candidate;
                break;
            }
        }
    }
    fs::copy(path, &final_target)?;
    Ok(json!({ "success": true, "path": final_target.to_string_lossy() }))
}

pub(super) fn run_st_bulk_import(state: &AppState, body: Value) -> AppResult<Value> {
    let root = body
        .get("folderPath")
        .and_then(Value::as_str)
        .filter(|path| !path.trim().is_empty())
        .ok_or_else(|| AppError::invalid_input("folderPath is required"))?;
    let root = PathBuf::from(root);
    let data_dir = resolve_st_data_dir(&root)
        .ok_or_else(|| AppError::invalid_input("Could not find SillyTavern data directory"))?;
    let options = body.get("options").cloned().unwrap_or_else(|| json!({}));
    let mut imported = json!({
        "characters": 0,
        "chats": 0,
        "groupChats": 0,
        "presets": 0,
        "lorebooks": 0,
        "backgrounds": 0,
        "personas": 0
    });
    let mut errors: Vec<Value> = Vec::new();
    let tag_mode = options
        .get("characterTagImportMode")
        .and_then(Value::as_str)
        .unwrap_or("all");
    let import_embedded = bool_option(options.get("importEmbeddedLorebook")).unwrap_or(true);

    let bump = |imported: &mut Value, key: &str| {
        if let Some(value) = imported.get_mut(key) {
            *value = json!(value.as_i64().unwrap_or(0) + 1);
        }
    };

    for id in selected_ids(&options, "characters") {
        let path = path_from_id(&data_dir, "characters", &id)?;
        let filename = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        let result = fs::read(&path)
            .map_err(AppError::from)
            .and_then(|bytes| parse_character_file(&filename, &bytes))
            .and_then(|payload| {
                import_st_character_payload(
                    state,
                    payload,
                    Some(filename.clone()),
                    &json!({ "tagImportMode": tag_mode, "importEmbeddedLorebook": import_embedded }),
                )
            });
        match result {
            Ok(_) => bump(&mut imported, "characters"),
            Err(error) => errors.push(Value::String(format!(
                "{}: {}",
                path.to_string_lossy(),
                error.message
            ))),
        }
    }

    for id in selected_ids(&options, "lorebooks") {
        let path = path_from_id(&data_dir, "lorebooks", &id)?;
        let result = fs::read(&path)
            .map_err(AppError::from)
            .and_then(|bytes| parse_object(&bytes))
            .and_then(|payload| {
                create_lorebook_from_payload(state, &payload, &file_stem(&path), None)
            });
        match result {
            Ok(_) => bump(&mut imported, "lorebooks"),
            Err(error) => errors.push(Value::String(format!(
                "{}: {}",
                path.to_string_lossy(),
                error.message
            ))),
        }
    }

    for id in selected_ids(&options, "presets") {
        let path = path_from_id(&data_dir, "presets", &id)?;
        let result = fs::read(&path)
            .map_err(AppError::from)
            .and_then(|bytes| parse_object(&bytes))
            .and_then(|payload| {
                state
                    .storage
                    .create("prompts", with_entity_defaults("prompts", payload))
            });
        match result {
            Ok(_) => bump(&mut imported, "presets"),
            Err(error) => errors.push(Value::String(format!(
                "{}: {}",
                path.to_string_lossy(),
                error.message
            ))),
        }
    }

    for id in selected_ids(&options, "personas") {
        let path = path_from_id(&data_dir, "personas", &id)?;
        match import_persona_file(state, &path) {
            Ok(_) => bump(&mut imported, "personas"),
            Err(error) => errors.push(Value::String(format!(
                "{}: {}",
                path.to_string_lossy(),
                error.message
            ))),
        }
    }

    for id in selected_ids(&options, "backgrounds") {
        let path = path_from_id(&data_dir, "backgrounds", &id)?;
        match copy_background_file(state, &path) {
            Ok(_) => bump(&mut imported, "backgrounds"),
            Err(error) => errors.push(Value::String(format!(
                "{}: {}",
                path.to_string_lossy(),
                error.message
            ))),
        }
    }

    for id in selected_ids(&options, "chats") {
        let path = path_from_id(&data_dir, "chats", &id)?;
        let result = fs::read_to_string(&path)
            .map_err(AppError::from)
            .and_then(|text| {
                import_st_chat_text(state, &text, file_stem(&path).replace('_', " "), None)
            });
        match result {
            Ok(_) => bump(&mut imported, "chats"),
            Err(error) => errors.push(Value::String(format!(
                "{}: {}",
                path.to_string_lossy(),
                error.message
            ))),
        }
    }

    for id in selected_ids(&options, "groupChats") {
        let path = path_from_id(&data_dir, "groupChats", &id)?;
        let result = fs::read_to_string(&path)
            .map_err(AppError::from)
            .and_then(|text| {
                import_st_chat_text(state, &text, file_stem(&path).replace('_', " "), None)
            });
        match result {
            Ok(_) => bump(&mut imported, "groupChats"),
            Err(error) => errors.push(Value::String(format!(
                "{}: {}",
                path.to_string_lossy(),
                error.message
            ))),
        }
    }

    let imported_total = imported
        .as_object()
        .map(|object| object.values().filter_map(Value::as_i64).sum::<i64>())
        .unwrap_or(0);
    Ok(json!({
        "success": imported_total > 0 || errors.is_empty(),
        "imported": imported,
        "errors": errors
    }))
}
