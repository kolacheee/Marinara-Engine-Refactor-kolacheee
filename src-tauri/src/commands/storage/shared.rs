use super::*;

pub(crate) struct ParsedPath {
    pub(crate) parts: Vec<String>,
    pub(crate) query: HashMap<String, String>,
}

impl ParsedPath {
    pub(crate) fn new(path: &str) -> Self {
        let (path_part, query_part) = path.split_once('?').unwrap_or((path, ""));
        let parts = path_part
            .trim_matches('/')
            .split('/')
            .filter(|part| !part.is_empty())
            .map(|part| part.to_string())
            .collect();
        let query = query_part
            .split('&')
            .filter_map(|pair| {
                let (key, value) = pair.split_once('=')?;
                Some((key.to_string(), value.to_string()))
            })
            .collect();
        Self { parts, query }
    }
}

pub(crate) fn collection_root(
    state: &AppState,
    method: &str,
    collection: &str,
    body: Value,
) -> AppResult<Value> {
    match method {
        "GET" => list_collection(state, collection, None),
        "POST" => state
            .storage
            .create(collection, with_entity_defaults(collection, body)),
        _ => Err(AppError::new(
            "method_not_allowed",
            format!("{method} is not allowed for /{collection}"),
        )),
    }
}

pub(crate) fn collection_item_or_action(
    state: &AppState,
    method: &str,
    collection: &str,
    id: &str,
    _action: Option<&str>,
    body: Value,
) -> AppResult<Value> {
    match method {
        "GET" => get_required(state, collection, id),
        "PATCH" => state.storage.patch(collection, id, body),
        "PUT" => state.storage.upsert_with_id(collection, id, body),
        "DELETE" => {
            let deleted = state.storage.delete(collection, id)?;
            Ok(json!({ "deleted": deleted }))
        }
        _ => Err(AppError::new(
            "method_not_allowed",
            format!("{method} is not allowed for /{collection}/{id}"),
        )),
    }
}

pub(crate) fn list_collection(
    state: &AppState,
    collection: &str,
    filter: Option<(&str, &str)>,
) -> AppResult<Value> {
    let mut rows = match filter {
        Some((key, value)) => {
            let mut filters = Map::new();
            filters.insert(key.to_string(), Value::String(value.to_string()));
            state.storage.list_where(collection, &filters)?
        }
        None => state.storage.list(collection)?,
    };
    rows.sort_by(|a, b| {
        let a_order = a
            .get("sortOrder")
            .or_else(|| a.get("order"))
            .and_then(Value::as_i64);
        let b_order = b
            .get("sortOrder")
            .or_else(|| b.get("order"))
            .and_then(Value::as_i64);
        match (a_order, b_order) {
            (Some(a_order), Some(b_order)) if a_order != b_order => a_order.cmp(&b_order),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            _ => {
                let a_time = a.get("createdAt").and_then(Value::as_str).unwrap_or("");
                let b_time = b.get("createdAt").and_then(Value::as_str).unwrap_or("");
                a_time.cmp(b_time)
            }
        }
    });
    Ok(Value::Array(rows))
}

pub(crate) fn reorder_collection(
    state: &AppState,
    collection: &str,
    ids_field: &str,
    body: Value,
) -> AppResult<Value> {
    let ids = string_array_from_value(body.get(ids_field));
    for (index, id) in ids.iter().enumerate() {
        if state.storage.get(collection, id)?.is_some() {
            state.storage.patch(
                collection,
                id,
                json!({ "sortOrder": index as i64, "order": index as i64 }),
            )?;
        }
    }
    list_collection(state, collection, None)
}

pub(crate) fn move_child_to_folder(
    state: &AppState,
    collection: &str,
    id_field: &str,
    folder_field: &str,
    body: Value,
) -> AppResult<Value> {
    let id = body
        .get(id_field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::invalid_input(format!("{id_field} is required")))?;
    let folder_id = body.get(folder_field).cloned().unwrap_or(Value::Null);
    let updated = state
        .storage
        .patch(collection, id, json!({ folder_field: folder_id }))?;
    Ok(json!({ "ok": true, "item": updated }))
}

pub(crate) fn reorder_children(
    state: &AppState,
    collection: &str,
    ids_field: &str,
    folder_field: Option<&str>,
    body: Value,
) -> AppResult<Value> {
    let ids = string_array_from_value(body.get(ids_field));
    let folder_value = folder_field.and_then(|field| body.get(field).cloned());
    for (index, id) in ids.iter().enumerate() {
        if state.storage.get(collection, id)?.is_none() {
            continue;
        }
        let mut patch = json!({ "sortOrder": index as i64, "order": index as i64 });
        if let (Some(field), Some(value)) = (folder_field, folder_value.clone()) {
            patch[field] = value;
        }
        state.storage.patch(collection, id, patch)?;
    }
    Ok(json!({ "ok": true, "orderedIds": ids }))
}

pub(crate) fn get_required(state: &AppState, collection: &str, id: &str) -> AppResult<Value> {
    state
        .storage
        .get(collection, id)?
        .ok_or_else(|| AppError::not_found(format!("{collection}/{id} was not found")))
}

pub(crate) fn handle_singleton(
    state: &AppState,
    method: &str,
    collection: &str,
    key: &str,
    body: Value,
) -> AppResult<Value> {
    match method {
        "GET" => Ok(state
            .storage
            .get(collection, key)?
            .unwrap_or_else(|| json!({ "id": key, "value": null }))),
        "PUT" | "PATCH" | "POST" => state.storage.upsert_with_id(collection, key, body),
        "DELETE" => {
            let deleted = state.storage.delete(collection, key)?;
            Ok(json!({ "deleted": deleted }))
        }
        _ => Err(AppError::new(
            "method_not_allowed",
            "Unsupported singleton method",
        )),
    }
}

pub(crate) fn with_entity_defaults(collection: &str, body: Value) -> Value {
    let mut object = ensure_object(body).unwrap_or_default();
    match collection {
        "chats" => {
            object
                .entry("metadata".to_string())
                .or_insert_with(|| json!({}));
            object
                .entry("gameState".to_string())
                .or_insert_with(|| json!({}));
            object
                .entry("characterIds".to_string())
                .or_insert_with(|| json!([]));
        }
        "connections" => {
            object
                .entry("enabled".to_string())
                .or_insert(Value::Bool(true));
        }
        "characters" => {
            if let Some(data) = object.get_mut("data") {
                if data.is_object() {
                    *data = Value::String(
                        serde_json::to_string(data).unwrap_or_else(|_| "{}".to_string()),
                    );
                }
            } else {
                let mut data = Map::new();
                let name = object
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or("New Character");
                data.insert("name".to_string(), Value::String(name.to_string()));
                data.insert("description".to_string(), Value::String(String::new()));
                data.insert("personality".to_string(), Value::String(String::new()));
                data.insert("scenario".to_string(), Value::String(String::new()));
                data.insert("first_mes".to_string(), Value::String(String::new()));
                data.insert("mes_example".to_string(), Value::String(String::new()));
                data.insert("creator_notes".to_string(), Value::String(String::new()));
                data.insert("system_prompt".to_string(), Value::String(String::new()));
                data.insert(
                    "post_history_instructions".to_string(),
                    Value::String(String::new()),
                );
                data.insert("tags".to_string(), json!([]));
                data.insert("creator".to_string(), Value::String(String::new()));
                data.insert(
                    "character_version".to_string(),
                    Value::String("1.0".to_string()),
                );
                data.insert("alternate_greetings".to_string(), json!([]));
                data.insert("extensions".to_string(), json!({ "altDescriptions": [] }));
                data.insert("character_book".to_string(), Value::Null);
                object.insert(
                    "data".to_string(),
                    Value::String(
                        serde_json::to_string(&Value::Object(data))
                            .unwrap_or_else(|_| "{}".to_string()),
                    ),
                );
            }
            object
                .entry("comment".to_string())
                .or_insert(Value::String(String::new()));
            object
                .entry("avatarPath".to_string())
                .or_insert(Value::Null);
        }
        "lorebooks" => {
            object
                .entry("description".to_string())
                .or_insert(Value::String(String::new()));
            object
                .entry("category".to_string())
                .or_insert(Value::String("uncategorized".to_string()));
            object.entry("imagePath".to_string()).or_insert(Value::Null);
            object.entry("scanDepth".to_string()).or_insert(json!(2));
            object
                .entry("tokenBudget".to_string())
                .or_insert(json!(2048));
            object
                .entry("recursiveScanning".to_string())
                .or_insert(Value::Bool(false));
            object
                .entry("maxRecursionDepth".to_string())
                .or_insert(json!(3));
            object
                .entry("characterId".to_string())
                .or_insert(Value::Null);
            object
                .entry("characterIds".to_string())
                .or_insert(json!([]));
            object.entry("personaId".to_string()).or_insert(Value::Null);
            object.entry("personaIds".to_string()).or_insert(json!([]));
            object.entry("chatId".to_string()).or_insert(Value::Null);
            object
                .entry("isGlobal".to_string())
                .or_insert(Value::Bool(false));
            object
                .entry("enabled".to_string())
                .or_insert(Value::Bool(true));
            object.entry("tags".to_string()).or_insert(json!([]));
            object
                .entry("generatedBy".to_string())
                .or_insert(Value::Null);
            object
                .entry("sourceAgentId".to_string())
                .or_insert(Value::Null);
        }
        "personas" => {
            object
                .entry("description".to_string())
                .or_insert(Value::String(String::new()));
            object
                .entry("comment".to_string())
                .or_insert(Value::String(String::new()));
            object
                .entry("personality".to_string())
                .or_insert(Value::String(String::new()));
            object
                .entry("scenario".to_string())
                .or_insert(Value::String(String::new()));
            object
                .entry("backstory".to_string())
                .or_insert(Value::String(String::new()));
            object
                .entry("appearance".to_string())
                .or_insert(Value::String(String::new()));
            object
                .entry("avatarPath".to_string())
                .or_insert(Value::Null);
            object
                .entry("isActive".to_string())
                .or_insert(Value::Bool(false));
            object
                .entry("tags".to_string())
                .or_insert(Value::String("[]".to_string()));
        }
        "prompts" => {
            object
                .entry("description".to_string())
                .or_insert(Value::String(String::new()));
            object
                .entry("parameters".to_string())
                .or_insert_with(|| json!({}));
            object
                .entry("isDefault".to_string())
                .or_insert(Value::Bool(false));
        }
        "agents" => {
            object
                .entry("enabled".to_string())
                .or_insert(Value::Bool(true));
        }
        _ => {}
    }
    Value::Object(object)
}

pub(crate) fn with_chat_defaults(body: Value) -> Value {
    with_entity_defaults("chats", body)
}

pub(crate) fn duplicate_record(state: &AppState, collection: &str, id: &str) -> AppResult<Value> {
    let mut record = get_required(state, collection, id)?;
    let object = record
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_input("Record is not an object"))?;
    object.remove("id");
    if let Some(name) = object
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string)
    {
        object.insert("name".to_string(), Value::String(format!("{name} Copy")));
    }
    state.storage.create(collection, record)
}

pub(crate) fn find_by_field(
    state: &AppState,
    collection: &str,
    field: &str,
    value: &str,
) -> AppResult<Option<Value>> {
    let mut filters = Map::new();
    filters.insert(field.to_string(), Value::String(value.to_string()));
    Ok(state
        .storage
        .list_where(collection, &filters)?
        .into_iter()
        .next())
}

pub(crate) fn decode_path(value: &str) -> String {
    value
        .replace("%2F", "/")
        .replace("%5C", "\\")
        .replace("%20", " ")
}

pub(crate) fn required_string<'a>(body: &'a Value, key: &str) -> AppResult<&'a str> {
    body.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::invalid_input(format!("{key} is required")))
}

pub(crate) fn string_array_from_value(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .filter(|item| !item.trim().is_empty())
            .map(ToOwned::to_owned)
            .collect(),
        Some(Value::String(raw)) => serde_json::from_str::<Vec<String>>(raw).unwrap_or_default(),
        _ => Vec::new(),
    }
}

pub(crate) const BACKUP_COLLECTIONS: &[&str] = &[
    "app-settings",
    "characters",
    "character-groups",
    "character-versions",
    "character-gallery",
    "personas",
    "persona-groups",
    "chats",
    "chat-folders",
    "gallery",
    "messages",
    "connections",
    "connection-folders",
    "prompts",
    "prompt-groups",
    "prompt-sections",
    "prompt-variables",
    "chat-presets",
    "lorebooks",
    "lorebook-entries",
    "lorebook-folders",
    "agents",
    "agent-runs",
    "custom-tools",
    "themes",
    "extensions",
    "regex-scripts",
    "sprites",
    "knowledge-sources",
    "background-metadata",
];

#[derive(Clone)]
pub(crate) struct UploadedFile {
    pub(crate) name: String,
    pub(crate) content_type: String,
    pub(crate) bytes: Vec<u8>,
}

pub(crate) fn decode_uploaded_file_value(file: &Value) -> AppResult<UploadedFile> {
    let name = file
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| AppError::invalid_input("uploaded file is missing a name"))?
        .to_string();
    let content_type = file
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("application/octet-stream")
        .to_string();
    let base64 = file
        .get("base64")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::invalid_input("uploaded file is missing base64 data"))?;
    let bytes = general_purpose::STANDARD
        .decode(base64)
        .map_err(|error| AppError::invalid_input(format!("Invalid upload encoding: {error}")))?;
    Ok(UploadedFile {
        name,
        content_type,
        bytes,
    })
}

pub(crate) fn decode_uploaded_file(body: &Value) -> AppResult<(String, String, Vec<u8>)> {
    let file = body
        .get("file")
        .ok_or_else(|| AppError::invalid_input("file is required"))?;
    let uploaded = decode_uploaded_file_value(file)?;
    Ok((uploaded.name, uploaded.content_type, uploaded.bytes))
}

pub(crate) fn decode_uploaded_files(body: &Value, field: &str) -> AppResult<Vec<UploadedFile>> {
    let Some(value) = body.get(field) else {
        return Ok(Vec::new());
    };
    match value {
        Value::Array(items) => items.iter().map(decode_uploaded_file_value).collect(),
        Value::Object(_) => decode_uploaded_file_value(value).map(|file| vec![file]),
        _ => Err(AppError::invalid_input(format!(
            "{field} must contain uploaded file objects"
        ))),
    }
}

pub(crate) fn upload_gallery_image(
    state: &AppState,
    collection: &str,
    parent_field: &str,
    parent_id: &str,
    body: Value,
) -> AppResult<Value> {
    let (name, content_type, bytes) = decode_uploaded_file(&body)?;
    let encoded = general_purpose::STANDARD.encode(bytes);
    let data_url = format!("data:{content_type};base64,{encoded}");
    let mut record = Map::new();
    record.insert(
        parent_field.to_string(),
        Value::String(parent_id.to_string()),
    );
    record.insert("filePath".to_string(), Value::String(name.clone()));
    record.insert("filename".to_string(), Value::String(name));
    record.insert("url".to_string(), Value::String(data_url));
    record.insert("prompt".to_string(), Value::Null);
    record.insert("provider".to_string(), Value::Null);
    record.insert("model".to_string(), Value::Null);
    record.insert("width".to_string(), Value::Null);
    record.insert("height".to_string(), Value::Null);
    state.storage.create(collection, Value::Object(record))
}

pub(crate) fn metadata_map(chat: &Value) -> Map<String, Value> {
    match chat.get("metadata") {
        Some(Value::Object(object)) => object.clone(),
        Some(Value::String(raw)) => serde_json::from_str::<Value>(raw)
            .ok()
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default(),
        _ => Map::new(),
    }
}
