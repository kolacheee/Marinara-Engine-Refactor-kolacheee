use super::shared::*;
use super::*;

const MEMORY_CHUNK_SIZE: usize = 5;
const MEMORY_EMBEDDING_DIMS: usize = 256;

pub(crate) fn messages_for_chat(state: &AppState, chat_id: &str) -> AppResult<Vec<Value>> {
    let mut filters = Map::new();
    filters.insert("chatId".to_string(), Value::String(chat_id.to_string()));
    let mut rows = state.storage.list_where("messages", &filters)?;
    rows.sort_by(|a, b| {
        let a_time = a.get("createdAt").and_then(Value::as_str).unwrap_or("");
        let b_time = b.get("createdAt").and_then(Value::as_str).unwrap_or("");
        a_time.cmp(b_time)
    });
    Ok(rows)
}

fn message_content(message: &Value) -> String {
    message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

fn lexical_memory_embedding(text: &str) -> Vec<f64> {
    let mut vector = vec![0.0_f64; MEMORY_EMBEDDING_DIMS];
    for token in text
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| token.len() > 1)
    {
        let mut hash = 2166136261_u32;
        for byte in token.to_ascii_lowercase().bytes() {
            hash ^= byte as u32;
            hash = hash.wrapping_mul(16777619);
        }
        let index = (hash as usize) % MEMORY_EMBEDDING_DIMS;
        vector[index] += 1.0;
    }
    let magnitude = vector.iter().map(|value| value * value).sum::<f64>().sqrt();
    if magnitude > 0.0 {
        for value in &mut vector {
            *value /= magnitude;
        }
    }
    vector
}

fn is_hidden_from_ai(message: &Value) -> bool {
    let extra = match message.get("extra") {
        Some(Value::Object(object)) => Some(object.clone()),
        Some(Value::String(raw)) => serde_json::from_str::<Value>(raw)
            .ok()
            .and_then(|value| value.as_object().cloned()),
        _ => None,
    };
    extra
        .as_ref()
        .and_then(|object| object.get("hiddenFromAi"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn merge_chat_metadata(
    state: &AppState,
    chat_id: &str,
    patch: Map<String, Value>,
) -> AppResult<Value> {
    let mut chat = get_required(state, "chats", chat_id)?;
    let mut metadata = metadata_map(&chat);
    for (key, value) in patch {
        metadata.insert(key, value);
    }
    chat.as_object_mut()
        .ok_or_else(|| AppError::invalid_input("Chat is not an object"))?
        .insert("metadata".to_string(), Value::Object(metadata));
    state.storage.patch("chats", chat_id, chat)
}

pub(crate) fn chat_messages(
    state: &AppState,
    method: &str,
    chat_id: &str,
    body: Value,
    query: &HashMap<String, String>,
) -> AppResult<Value> {
    match method {
        "GET" => {
            let mut rows = messages_for_chat(state, chat_id)?;
            if let Some(limit) = query
                .get("limit")
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|limit| *limit > 0)
            {
                if rows.len() > limit {
                    rows = rows.split_off(rows.len() - limit);
                }
            }
            Ok(Value::Array(rows))
        }
        "POST" => {
            let mut object = ensure_object(body)?;
            object.insert("chatId".to_string(), Value::String(chat_id.to_string()));
            object
                .entry("role".to_string())
                .or_insert_with(|| Value::String("user".to_string()));
            object
                .entry("content".to_string())
                .or_insert_with(|| Value::String(String::new()));
            object
                .entry("extra".to_string())
                .or_insert_with(|| json!({}));
            object
                .entry("activeSwipeIndex".to_string())
                .or_insert_with(|| json!(0));
            let content = object
                .get("content")
                .cloned()
                .unwrap_or_else(|| Value::String(String::new()));
            object
                .entry("swipes".to_string())
                .or_insert_with(|| json!([{ "content": content }]));
            let record = state.storage.create("messages", Value::Object(object))?;
            touch_chat(state, chat_id)?;
            Ok(record)
        }
        _ => Err(AppError::new(
            "method_not_allowed",
            "Unsupported messages method",
        )),
    }
}

pub(crate) fn chat_message_item(
    state: &AppState,
    method: &str,
    chat_id: &str,
    message_id: &str,
    body: Value,
) -> AppResult<Value> {
    match method {
        "GET" => get_required(state, "messages", message_id),
        "PATCH" => {
            let updated = state.storage.patch("messages", message_id, body)?;
            touch_chat(state, chat_id)?;
            Ok(updated)
        }
        "DELETE" => {
            let deleted = state.storage.delete("messages", message_id)?;
            touch_chat(state, chat_id)?;
            Ok(json!({ "deleted": deleted }))
        }
        _ => Err(AppError::new(
            "method_not_allowed",
            "Unsupported message method",
        )),
    }
}

pub(crate) fn patch_message_extra(
    state: &AppState,
    chat_id: &str,
    message_id: &str,
    body: Value,
) -> AppResult<Value> {
    let mut message = get_required(state, "messages", message_id)?;
    let patch = ensure_object(body)?;
    {
        let object = message
            .as_object_mut()
            .ok_or_else(|| AppError::invalid_input("Message is not an object"))?;
        let extra = object
            .entry("extra".to_string())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(|| AppError::invalid_input("Message extra is not an object"))?;
        for (key, value) in patch {
            extra.insert(key, value);
        }
    }
    let updated = state.storage.patch("messages", message_id, message)?;
    touch_chat(state, chat_id)?;
    Ok(updated)
}

pub(crate) fn message_swipes(
    state: &AppState,
    _method: &str,
    _chat_id: &str,
    message_id: &str,
    body: Value,
) -> AppResult<Value> {
    let mut message = get_required(state, "messages", message_id)?;
    if body.is_null() {
        return Ok(message.get("swipes").cloned().unwrap_or_else(|| json!([])));
    }
    let content = body
        .get("content")
        .cloned()
        .unwrap_or_else(|| Value::String(String::new()));
    let object = message
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_input("Message is not an object"))?;
    let swipes = object
        .entry("swipes".to_string())
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| AppError::invalid_input("Message swipes is not an array"))?;
    swipes.push(json!({ "content": content, "createdAt": now_iso() }));
    let active_index = swipes.len().saturating_sub(1);
    object.insert("activeSwipeIndex".to_string(), json!(active_index));
    let updated = state.storage.patch("messages", message_id, message)?;
    Ok(updated)
}

pub(crate) fn set_active_swipe(
    state: &AppState,
    _chat_id: &str,
    message_id: &str,
    body: Value,
) -> AppResult<Value> {
    let index = body.get("index").and_then(Value::as_i64).unwrap_or(0);
    state
        .storage
        .patch("messages", message_id, json!({ "activeSwipeIndex": index }))
}

pub(crate) fn delete_swipe(
    state: &AppState,
    _chat_id: &str,
    message_id: &str,
    index: &str,
) -> AppResult<Value> {
    let index = index
        .parse::<usize>()
        .map_err(|_| AppError::invalid_input("Invalid swipe index"))?;
    let mut message = get_required(state, "messages", message_id)?;
    let object = message
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_input("Message is not an object"))?;
    if let Some(swipes) = object.get_mut("swipes").and_then(Value::as_array_mut) {
        if index < swipes.len() {
            swipes.remove(index);
        }
    }
    object.insert("activeSwipeIndex".to_string(), json!(0));
    state.storage.patch("messages", message_id, message)
}

pub(crate) fn bulk_delete_messages(
    state: &AppState,
    chat_id: &str,
    body: Value,
) -> AppResult<Value> {
    let ids = body
        .get("messageIds")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut deleted = 0;
    for id in ids.iter().filter_map(Value::as_str) {
        if state.storage.delete("messages", id)? {
            deleted += 1;
        }
    }
    touch_chat(state, chat_id)?;
    Ok(json!({ "deleted": deleted }))
}

pub(crate) fn bulk_hide_messages(state: &AppState, chat_id: &str, body: Value) -> AppResult<Value> {
    let ids = body
        .get("messageIds")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let hidden = body.get("hidden").and_then(Value::as_bool).unwrap_or(true);
    for id in ids.iter().filter_map(Value::as_str) {
        patch_message_extra(state, chat_id, id, json!({ "hiddenFromAi": hidden }))?;
    }
    Ok(json!({ "updated": true }))
}

pub(crate) fn patch_chat_object_field(
    state: &AppState,
    chat_id: &str,
    field: &str,
    body: Value,
) -> AppResult<Value> {
    let mut chat = get_required(state, "chats", chat_id)?;
    let patch = ensure_object(body)?;
    {
        let object = chat
            .as_object_mut()
            .ok_or_else(|| AppError::invalid_input("Chat is not an object"))?;
        let target = object
            .entry(field.to_string())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(|| AppError::invalid_input(format!("Chat {field} is not an object")))?;
        for (key, value) in patch {
            target.insert(key, value);
        }
    }
    state.storage.patch("chats", chat_id, chat)
}

pub(crate) fn mark_autonomous_unread(
    state: &AppState,
    chat_id: &str,
    body: Value,
) -> AppResult<Value> {
    get_required(state, "chats", chat_id)?;
    let mut patch = Map::new();
    let count = body
        .get("count")
        .and_then(Value::as_i64)
        .unwrap_or(1)
        .max(1);
    let character_id = body
        .get("characterId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let mut character_ids = Vec::new();
    if let Some(id) = character_id {
        character_ids.push(Value::String(id));
    }
    patch.insert("autonomousUnreadCount".to_string(), json!(count));
    patch.insert(
        "autonomousUnreadCharacterIds".to_string(),
        Value::Array(character_ids),
    );
    patch.insert("autonomousUnreadAt".to_string(), Value::String(now_iso()));
    merge_chat_metadata(state, chat_id, patch)
}

pub(crate) fn clear_autonomous_unread(state: &AppState, chat_id: &str) -> AppResult<Value> {
    let mut patch = Map::new();
    patch.insert("autonomousUnreadCount".to_string(), json!(0));
    patch.insert("autonomousUnreadCharacterIds".to_string(), json!([]));
    patch.insert("autonomousUnreadAt".to_string(), Value::Null);
    merge_chat_metadata(state, chat_id, patch)
}

pub(crate) fn chat_array_field(state: &AppState, chat_id: &str, field: &str) -> AppResult<Value> {
    let chat = get_required(state, "chats", chat_id)?;
    Ok(chat.get(field).cloned().unwrap_or_else(|| json!([])))
}

pub(crate) fn set_chat_array_field(
    state: &AppState,
    chat_id: &str,
    field: &str,
    values: Vec<Value>,
) -> AppResult<Value> {
    state
        .storage
        .patch("chats", chat_id, json!({ field: values }))
}

pub(crate) fn delete_chat_array_item(
    state: &AppState,
    chat_id: &str,
    field: &str,
    item_id: &str,
) -> AppResult<Value> {
    let chat = get_required(state, "chats", chat_id)?;
    let values = chat
        .get(field)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|item| item.get("id").and_then(Value::as_str) != Some(item_id))
        .collect::<Vec<_>>();
    set_chat_array_field(state, chat_id, field, values)
}

pub(crate) fn refresh_chat_memories(state: &AppState, chat_id: &str) -> AppResult<Value> {
    get_required(state, "chats", chat_id)?;
    let visible_messages = messages_for_chat(state, chat_id)?
        .into_iter()
        .filter(|message| !is_hidden_from_ai(message) && !message_content(message).is_empty())
        .collect::<Vec<_>>();
    let now = now_iso();
    let chunks = visible_messages
        .chunks(MEMORY_CHUNK_SIZE)
        .map(|chunk| {
            let content = chunk
                .iter()
                .map(|message| {
                    let role = message.get("role").and_then(Value::as_str).unwrap_or("message");
                    format!("{role}: {}", message_content(message))
                })
                .collect::<Vec<_>>()
                .join("\n");
            let embedding = lexical_memory_embedding(&content);
            json!({
                "id": new_id(),
                "chatId": chat_id,
                "content": content,
                "embedding": embedding,
                "messageCount": chunk.len(),
                "firstMessageAt": chunk.first().and_then(|message| message.get("createdAt")).cloned().unwrap_or(Value::Null),
                "lastMessageAt": chunk.last().and_then(|message| message.get("createdAt")).cloned().unwrap_or(Value::Null),
                "createdAt": now,
                "hasEmbedding": true,
                "embeddingStatus": "vectorized"
            })
        })
        .collect::<Vec<_>>();
    state
        .storage
        .patch("chats", chat_id, json!({ "memories": chunks }))?;
    Ok(json!({ "rebuilt": chunks.len(), "chunks": chunks }))
}

pub(crate) fn export_chat_memories(state: &AppState, chat_id: &str) -> AppResult<Value> {
    let chat = get_required(state, "chats", chat_id)?;
    let memories = chat_array_field(state, chat_id, "memories")?;
    let memory_count = memories.as_array().map(Vec::len).unwrap_or(0);
    Ok(json!({
        "type": "marinara_memory_recall",
        "version": 1,
        "exportedAt": now_iso(),
        "data": {
            "sourceChat": {
                "id": chat_id,
                "name": chat.get("name").and_then(Value::as_str).unwrap_or("Untitled Chat"),
                "mode": chat.get("mode").and_then(Value::as_str).unwrap_or("conversation"),
                "memoryCount": memory_count
            },
            "chunks": memories
        }
    }))
}

pub(crate) fn import_chat_memories(state: &AppState, chat_id: &str, body: Value) -> AppResult<Value> {
    get_required(state, "chats", chat_id)?;
    let incoming = body
        .get("data")
        .and_then(|data| data.get("chunks"))
        .or_else(|| body.get("chunks"))
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::invalid_input("Memory Recall import must contain a data.chunks array"))?;
    let mut memories = chat_array_field(state, chat_id, "memories")?
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut seen = memories
        .iter()
        .filter_map(|memory| {
            memory
                .get("content")
                .and_then(Value::as_str)
                .map(|content| content.trim().to_string())
        })
        .collect::<std::collections::HashSet<_>>();
    let now = now_iso();
    let mut imported = 0usize;
    let mut skipped = 0usize;
    for value in incoming {
        let Some(content) = value.get("content").and_then(Value::as_str).map(str::trim) else {
            skipped += 1;
            continue;
        };
        if content.is_empty() || !seen.insert(content.to_string()) {
            skipped += 1;
            continue;
        }
        let mut memory = value.as_object().cloned().unwrap_or_default();
        memory.insert(
            "id".to_string(),
            memory
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.trim().is_empty())
                .map(|id| Value::String(id.to_string()))
                .unwrap_or_else(|| Value::String(new_id())),
        );
        memory.insert("chatId".to_string(), Value::String(chat_id.to_string()));
        memory.insert("content".to_string(), Value::String(content.to_string()));
        memory
            .entry("createdAt".to_string())
            .or_insert_with(|| Value::String(now.clone()));
        memory
            .entry("messageCount".to_string())
            .or_insert_with(|| json!(1));
        let has_embedding = memory
            .get("embedding")
            .and_then(Value::as_array)
            .is_some_and(|items| items.iter().any(Value::is_number));
        if !has_embedding {
            memory.insert(
                "embedding".to_string(),
                Value::Array(
                    lexical_memory_embedding(content)
                        .into_iter()
                        .map(|value| json!(value))
                        .collect(),
                ),
            );
        }
        memory.insert("hasEmbedding".to_string(), json!(true));
        memory.insert("embeddingStatus".to_string(), json!("vectorized"));
        memories.push(Value::Object(memory));
        imported += 1;
    }
    set_chat_array_field(state, chat_id, "memories", memories)?;
    Ok(json!({ "imported": imported, "skipped": skipped }))
}

pub(crate) fn touch_chat(state: &AppState, chat_id: &str) -> AppResult<()> {
    if state.storage.get("chats", chat_id)?.is_some() {
        state
            .storage
            .patch("chats", chat_id, json!({ "lastMessageAt": now_iso() }))?;
    }
    Ok(())
}

pub(crate) fn delete_chat_group(state: &AppState, group_id: &str) -> AppResult<Value> {
    let chats = match list_collection(state, "chats", Some(("groupId", group_id)))? {
        Value::Array(rows) => rows,
        _ => Vec::new(),
    };
    let mut deleted = 0;
    for chat in chats {
        if let Some(id) = chat.get("id").and_then(Value::as_str) {
            delete_chat_with_messages(state, id)?;
            deleted += 1;
        }
    }
    Ok(json!({ "deleted": deleted }))
}

pub(crate) fn branch_chat(state: &AppState, chat_id: &str, body: Value) -> AppResult<Value> {
    let mut chat = get_required(state, "chats", chat_id)?;
    let new_chat_id = new_id();
    let object = chat
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_input("Chat is not an object"))?;
    let base_name = object
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Chat")
        .to_string();
    let group_id = object
        .get("groupId")
        .and_then(Value::as_str)
        .unwrap_or(chat_id)
        .to_string();
    object.insert("id".to_string(), Value::String(new_chat_id.clone()));
    object.insert(
        "name".to_string(),
        Value::String(format!("{base_name} Branch")),
    );
    object.insert("groupId".to_string(), Value::String(group_id));
    let new_chat = state.storage.create("chats", chat)?;
    let up_to = body.get("upToMessageId").and_then(Value::as_str);
    for mut message in messages_for_chat(state, chat_id)? {
        let stop = up_to.is_some_and(|id| message.get("id").and_then(Value::as_str) == Some(id));
        if let Some(obj) = message.as_object_mut() {
            obj.remove("id");
            obj.insert("chatId".to_string(), Value::String(new_chat_id.clone()));
        }
        state.storage.create("messages", message)?;
        if stop {
            break;
        }
    }
    Ok(new_chat)
}

pub(crate) fn peek_prompt(state: &AppState, chat_id: &str) -> AppResult<Value> {
    let messages: Vec<Value> = messages_for_chat(state, chat_id)?
        .into_iter()
        .map(|message| {
            json!({
                "role": message.get("role").and_then(Value::as_str).unwrap_or("user"),
                "content": message.get("content").and_then(Value::as_str).unwrap_or("")
            })
        })
        .collect();
    Ok(json!({ "messages": messages, "parameters": {}, "generationInfo": Value::Null }))
}

pub(crate) fn delete_chat_with_messages(state: &AppState, chat_id: &str) -> AppResult<()> {
    for message in messages_for_chat(state, chat_id)? {
        if let Some(id) = message.get("id").and_then(Value::as_str) {
            state.storage.delete("messages", id)?;
        }
    }
    state.storage.delete("chats", chat_id)?;
    Ok(())
}
