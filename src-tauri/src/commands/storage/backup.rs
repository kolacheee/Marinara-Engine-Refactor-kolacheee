use super::shared::*;
use super::*;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;

pub(crate) fn backup_snapshot(state: &AppState) -> AppResult<Value> {
    let collections = backup_collections(state)?;
    Ok(json!({
        "type": "marinara_profile",
        "version": 1,
        "exportedAt": now_iso(),
        "runtime": "tauri",
        "data": {
            "characters": state.storage.list("characters")?,
            "personas": state.storage.list("personas")?,
            "lorebooks": profile_lorebooks(state)?,
            "presets": profile_presets(state)?,
            "agents": state.storage.list("agents")?,
            "themes": state.storage.list("themes")?,
            "connections": state.storage.list("connections")?,
            "chats": state.storage.list("chats")?,
            "messages": state.storage.list("messages")?,
            "fileStorage": {
                "version": 1,
                "tables": collections,
                "files": []
            }
        }
    }))
}

fn backup_collections(state: &AppState) -> AppResult<Map<String, Value>> {
    let mut collections = Map::new();
    for collection in BACKUP_COLLECTIONS {
        collections.insert(
            (*collection).to_string(),
            Value::Array(state.storage.list(collection)?),
        );
    }
    Ok(collections)
}

pub(crate) fn backup_call(
    state: &AppState,
    method: &str,
    rest: &[&str],
    route: &ParsedPath,
    body: Value,
) -> AppResult<Value> {
    match (method, rest) {
        ("GET", []) => list_backups(state),
        ("POST", []) => create_backup_folder(state),
        ("GET", ["export-profile"]) => {
            export_profile(state, route.query.get("format").map(String::as_str))
        }
        ("POST", ["download"]) => download_backup_zip(state),
        ("POST", ["import-profile"]) => import_profile(state, body),
        ("DELETE", [name]) => delete_backup(state, name),
        _ => Err(AppError::new(
            "route_not_found",
            format!("Unknown backup route: {method} /{}", rest.join("/")),
        )),
    }
}

fn export_profile(state: &AppState, format: Option<&str>) -> AppResult<Value> {
    match format {
        Some("compatible") | Some("compatible-png") => compatible_profile_zip(state),
        _ => backup_snapshot(state),
    }
}

fn import_profile(state: &AppState, body: Value) -> AppResult<Value> {
    if body.get("type").and_then(Value::as_str) == Some("marinara_profile") {
        let data = body
            .get("data")
            .and_then(Value::as_object)
            .ok_or_else(|| AppError::invalid_input("Profile export is missing data"))?;
        if let Some(tables) = data
            .get("fileStorage")
            .and_then(|value| value.get("tables"))
            .and_then(Value::as_object)
        {
            return import_collection_map(state, tables);
        }
        return import_profile_arrays(state, data);
    }

    if let Some(collections) = body.get("collections").and_then(Value::as_object) {
        return import_collection_map(state, collections);
    }

    Err(AppError::invalid_input("Invalid profile export"))
}

fn import_collection_map(state: &AppState, collections: &Map<String, Value>) -> AppResult<Value> {
    let mut imported = Map::new();
    for (collection, rows) in collections {
        let collection = normalize_backup_collection_name(collection);
        if !BACKUP_COLLECTIONS.contains(&collection.as_str()) {
            continue;
        }
        let rows = rows
            .as_array()
            .ok_or_else(|| AppError::invalid_input(format!("{collection} must be an array")))?
            .clone();
        let count = rows.len();
        state.storage.replace_all(&collection, rows)?;
        imported.insert(collection, json!(count));
    }
    Ok(json!({ "success": true, "imported": imported }))
}

fn import_profile_arrays(state: &AppState, data: &Map<String, Value>) -> AppResult<Value> {
    let pairs = [
        ("characters", "characters"),
        ("personas", "personas"),
        ("agents", "agents"),
        ("themes", "themes"),
        ("connections", "connections"),
        ("chats", "chats"),
        ("messages", "messages"),
        ("presets", "prompts"),
    ];
    let mut imported = Map::new();
    for (profile_key, collection) in pairs {
        if let Some(rows) = data.get(profile_key).and_then(Value::as_array) {
            state.storage.replace_all(collection, rows.clone())?;
            imported.insert(collection.to_string(), json!(rows.len()));
        }
    }
    if let Some(lorebooks) = data.get("lorebooks").and_then(Value::as_array) {
        let mut books = Vec::new();
        let mut entries = Vec::new();
        let mut folders = Vec::new();
        for item in lorebooks {
            let mut book = item.clone();
            if let Some(object) = book.as_object_mut() {
                if let Some(Value::Array(item_entries)) = object.remove("entries") {
                    entries.extend(item_entries);
                }
                if let Some(Value::Array(item_folders)) = object.remove("folders") {
                    folders.extend(item_folders);
                }
            }
            books.push(book);
        }
        state.storage.replace_all("lorebooks", books.clone())?;
        state
            .storage
            .replace_all("lorebook-entries", entries.clone())?;
        state
            .storage
            .replace_all("lorebook-folders", folders.clone())?;
        imported.insert("lorebooks".to_string(), json!(books.len()));
        imported.insert("lorebook-entries".to_string(), json!(entries.len()));
        imported.insert("lorebook-folders".to_string(), json!(folders.len()));
    }
    Ok(json!({ "success": true, "imported": imported }))
}

fn normalize_backup_collection_name(name: &str) -> String {
    match name {
        "api_connections" => "connections",
        "prompt_presets" => "prompts",
        "prompt_groups" => "prompt-groups",
        "prompt_sections" => "prompt-sections",
        "prompt_variables" => "prompt-variables",
        "agent_configs" => "agents",
        "custom_themes" => "themes",
        "custom_tools" => "custom-tools",
        "lorebook_entries" => "lorebook-entries",
        "lorebook_folders" => "lorebook-folders",
        "chat_folders" => "chat-folders",
        "connection_folders" => "connection-folders",
        "character_groups" => "character-groups",
        "persona_groups" => "persona-groups",
        "app_settings" => "app-settings",
        other => other,
    }
    .to_string()
}

fn profile_lorebooks(state: &AppState) -> AppResult<Vec<Value>> {
    let mut books = state.storage.list("lorebooks")?;
    for book in &mut books {
        let Some(id) = book
            .get("id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
        else {
            continue;
        };
        if let Some(object) = book.as_object_mut() {
            object.insert(
                "entries".to_string(),
                list_collection(state, "lorebook-entries", Some(("lorebookId", &id)))?,
            );
            object.insert(
                "folders".to_string(),
                list_collection(state, "lorebook-folders", Some(("lorebookId", &id)))?,
            );
        }
    }
    Ok(books)
}

fn profile_presets(state: &AppState) -> AppResult<Vec<Value>> {
    let mut presets = state.storage.list("prompts")?;
    for preset in &mut presets {
        let Some(id) = preset
            .get("id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
        else {
            continue;
        };
        if let Some(object) = preset.as_object_mut() {
            object.insert(
                "groups".to_string(),
                list_collection(state, "prompt-groups", Some(("presetId", &id)))?,
            );
            object.insert(
                "sections".to_string(),
                list_collection(state, "prompt-sections", Some(("presetId", &id)))?,
            );
            object.insert(
                "variables".to_string(),
                list_collection(state, "prompt-variables", Some(("presetId", &id)))?,
            );
        }
    }
    Ok(presets)
}

fn list_backups(state: &AppState) -> AppResult<Value> {
    let root = backups_root(state);
    if !root.exists() {
        return Ok(json!([]));
    }
    let mut backups = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !is_safe_backup_name(&name) {
            continue;
        }
        let metadata = entry.metadata()?;
        backups.push(json!({
            "name": name,
            "createdAt": metadata.created().ok().map(system_time_iso).unwrap_or_else(now_iso),
            "path": path.to_string_lossy()
        }));
    }
    backups.sort_by(|a, b| {
        b.get("createdAt")
            .and_then(Value::as_str)
            .cmp(&a.get("createdAt").and_then(Value::as_str))
    });
    Ok(Value::Array(backups))
}

fn create_backup_folder(state: &AppState) -> AppResult<Value> {
    let name = backup_name();
    let dir = backups_root(state).join(&name);
    fs::create_dir_all(&dir)?;
    fs::write(
        dir.join("marinara-profile.json"),
        serde_json::to_vec_pretty(&backup_snapshot(state)?)?,
    )?;
    let collections = dir.join("collections");
    fs::create_dir_all(&collections)?;
    for collection in BACKUP_COLLECTIONS {
        fs::write(
            collections.join(format!("{collection}.json")),
            serde_json::to_vec_pretty(&state.storage.list(collection)?)?,
        )?;
    }
    Ok(json!({
        "success": true,
        "name": name,
        "createdAt": now_iso(),
        "path": dir.to_string_lossy()
    }))
}

fn delete_backup(state: &AppState, name: &str) -> AppResult<Value> {
    if !is_safe_backup_name(name) {
        return Err(AppError::invalid_input("Invalid backup name"));
    }
    let path = backups_root(state).join(name);
    if !path.exists() {
        return Err(AppError::not_found("Backup not found"));
    }
    fs::remove_dir_all(path)?;
    Ok(json!({ "success": true }))
}

fn download_backup_zip(state: &AppState) -> AppResult<Value> {
    let filename = format!("{}.zip", backup_name());
    let bytes = build_backup_zip(state)?;
    Ok(binary_download(bytes, "application/zip", &filename))
}

fn compatible_profile_zip(state: &AppState) -> AppResult<Value> {
    let mut zip = ZipBuilder::new();
    for (index, character) in state.storage.list("characters")?.into_iter().enumerate() {
        let data = character_data_value(&character);
        let name = data
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("character");
        zip.add_json(
            &format!(
                "characters/{}.json",
                safe_export_name(name, &format!("character-{}", index + 1))
            ),
            &json!({ "spec": "chara_card_v2", "spec_version": "2.0", "data": data }),
        )?;
    }
    for (index, persona) in state.storage.list("personas")?.into_iter().enumerate() {
        let name = persona
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("persona");
        zip.add_json(
            &format!(
                "personas/{}.json",
                safe_export_name(name, &format!("persona-{}", index + 1))
            ),
            &persona,
        )?;
    }
    for (index, lorebook) in profile_lorebooks(state)?.into_iter().enumerate() {
        let name = lorebook
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("lorebook");
        zip.add_json(
            &format!(
                "lorebooks/{}.json",
                safe_export_name(name, &format!("lorebook-{}", index + 1))
            ),
            &compatible_lorebook(&lorebook),
        )?;
    }
    Ok(binary_download(
        zip.finish()?,
        "application/zip",
        "marinara-compatible-export.zip",
    ))
}

fn build_backup_zip(state: &AppState) -> AppResult<Vec<u8>> {
    let mut zip = ZipBuilder::new();
    zip.add_json("marinara-profile.json", &backup_snapshot(state)?)?;
    zip.add_text(
        "RESTORE.txt",
        "Marinara Engine backup\n\nImport marinara-profile.json through Settings -> Import Profile (JSON). The remaining files are included for manual recovery.\n",
    )?;
    zip.add_json(
        "collections.json",
        &Value::Object(backup_collections(state)?),
    )?;
    add_directory_to_zip(&mut zip, &state.data_dir, &state.data_dir, "backups")?;
    zip.finish()
}

struct ZipBuilder {
    writer: zip::ZipWriter<Cursor<Vec<u8>>>,
    options: SimpleFileOptions,
}

impl ZipBuilder {
    fn new() -> Self {
        Self {
            writer: zip::ZipWriter::new(Cursor::new(Vec::new())),
            options: SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        }
    }

    fn add_text(&mut self, path: &str, text: &str) -> AppResult<()> {
        self.writer
            .start_file(path.replace('\\', "/"), self.options)
            .map_err(zip_error)?;
        self.writer.write_all(text.as_bytes())?;
        Ok(())
    }

    fn add_json(&mut self, path: &str, value: &Value) -> AppResult<()> {
        self.writer
            .start_file(path.replace('\\', "/"), self.options)
            .map_err(zip_error)?;
        self.writer.write_all(&serde_json::to_vec_pretty(value)?)?;
        Ok(())
    }

    fn add_bytes(&mut self, path: &str, bytes: &[u8]) -> AppResult<()> {
        self.writer
            .start_file(path.replace('\\', "/"), self.options)
            .map_err(zip_error)?;
        self.writer.write_all(bytes)?;
        Ok(())
    }

    fn finish(self) -> AppResult<Vec<u8>> {
        Ok(self.writer.finish().map_err(zip_error)?.into_inner())
    }
}

fn zip_error(error: zip::result::ZipError) -> AppError {
    AppError::new("zip_error", error.to_string())
}

fn add_directory_to_zip(
    zip: &mut ZipBuilder,
    root: &Path,
    current: &Path,
    skip_dir: &str,
) -> AppResult<()> {
    if !current.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if root == current && name == skip_dir {
                continue;
            }
            add_directory_to_zip(zip, root, &path, skip_dir)?;
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        zip.add_bytes(&format!("files/{relative}"), &fs::read(&path)?)?;
    }
    Ok(())
}

fn binary_download(bytes: Vec<u8>, content_type: &str, filename: &str) -> Value {
    json!({
        "base64": general_purpose::STANDARD.encode(bytes),
        "contentType": content_type,
        "filename": filename
    })
}

fn backups_root(state: &AppState) -> PathBuf {
    state.data_dir.join("backups")
}

fn backup_name() -> String {
    let timestamp = now_iso()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    format!("marinara-backup-{timestamp}")
}

fn is_safe_backup_name(name: &str) -> bool {
    name.starts_with("marinara-backup-")
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
}

fn system_time_iso(time: std::time::SystemTime) -> String {
    let _ = time;
    now_iso()
}

fn character_data_value(character: &Value) -> Value {
    match character.get("data") {
        Some(Value::String(raw)) => serde_json::from_str(raw).unwrap_or_else(|_| json!({})),
        Some(value) => value.clone(),
        None => character.clone(),
    }
}

fn compatible_lorebook(lorebook: &Value) -> Value {
    let mut entries = Map::new();
    for (index, entry) in lorebook
        .get("entries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .enumerate()
    {
        entries.insert(
            index.to_string(),
            json!({
                "uid": index,
                "key": super::prompts::value_string_array(entry.get("keys")),
                "keysecondary": super::prompts::value_string_array(entry.get("secondaryKeys")),
                "comment": entry.get("name").and_then(Value::as_str).unwrap_or("Entry"),
                "content": entry.get("content").and_then(Value::as_str).unwrap_or(""),
                "disable": entry.get("enabled").and_then(Value::as_bool) == Some(false),
                "constant": entry.get("constant").and_then(Value::as_bool).unwrap_or(false),
                "selective": entry.get("selective").and_then(Value::as_bool).unwrap_or(false),
                "order": entry.get("order").or_else(|| entry.get("sortOrder")).and_then(Value::as_i64).unwrap_or(100),
                "position": entry.get("position").cloned().unwrap_or_else(|| json!(0))
            }),
        );
    }
    json!({
        "name": lorebook.get("name").and_then(Value::as_str).unwrap_or("Lorebook"),
        "description": lorebook.get("description").cloned().unwrap_or(Value::Null),
        "entries": entries
    })
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
