use super::shared::*;
use super::*;

const MEMORY_CHUNK_SIZE: usize = 8;

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

fn chat_character_ids(chat: &Value) -> Vec<String> {
    string_array_from_value(chat.get("characterIds"))
}

fn message_content(message: &Value) -> String {
    message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
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

fn estimate_tokens(text: &str) -> usize {
    (text.chars().count() / 4).max(1)
}

fn compact_line(text: &str, max_chars: usize) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        normalized
    } else {
        let mut out = normalized
            .chars()
            .take(max_chars.saturating_sub(1))
            .collect::<String>();
        out = out.trim_end().to_string();
        out.push_str("...");
        out
    }
}

fn build_local_summary(messages: &[Value]) -> String {
    let mut lines = Vec::new();
    for message in messages {
        let content = message_content(message);
        if content.is_empty() {
            continue;
        }
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("message");
        lines.push(format!("- {role}: {}", compact_line(&content, 280)));
    }
    if lines.is_empty() {
        "No visible messages were available to summarize.".to_string()
    } else {
        lines.join("\n")
    }
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
            json!({
                "id": new_id(),
                "chatId": chat_id,
                "content": content,
                "messageCount": chunk.len(),
                "firstMessageAt": chunk.first().and_then(|message| message.get("createdAt")).cloned().unwrap_or(Value::Null),
                "lastMessageAt": chunk.last().and_then(|message| message.get("createdAt")).cloned().unwrap_or(Value::Null),
                "createdAt": now,
                "hasEmbedding": false,
                "embeddingStatus": "unavailable"
            })
        })
        .collect::<Vec<_>>();
    state
        .storage
        .patch("chats", chat_id, json!({ "memories": chunks }))?;
    Ok(json!({ "rebuilt": chunks.len(), "chunks": chunks }))
}

pub(crate) fn generate_summary(state: &AppState, chat_id: &str, body: Value) -> AppResult<Value> {
    let chat = get_required(state, "chats", chat_id)?;
    let meta = metadata_map(&chat);
    let context_size = body
        .get("contextSize")
        .and_then(Value::as_u64)
        .or_else(|| meta.get("summaryContextSize").and_then(Value::as_u64))
        .unwrap_or(50)
        .clamp(5, 200) as usize;
    let all_messages = messages_for_chat(state, chat_id)?;
    let start_id = body.get("rangeStartMessageId").and_then(Value::as_str);
    let end_id = body.get("rangeEndMessageId").and_then(Value::as_str);
    let mut range_start_index = None;
    let mut range_end_index = None;
    let selected = if let (Some(start_id), Some(end_id)) = (start_id, end_id) {
        let start = all_messages
            .iter()
            .position(|message| message.get("id").and_then(Value::as_str) == Some(start_id))
            .ok_or_else(|| AppError::invalid_input("Summary range start message was not found"))?;
        let end = all_messages
            .iter()
            .position(|message| message.get("id").and_then(Value::as_str) == Some(end_id))
            .ok_or_else(|| AppError::invalid_input("Summary range end message was not found"))?;
        let from = start.min(end);
        let to = start.max(end);
        if to - from + 1 > 200 {
            return Err(AppError::invalid_input(
                "Summary ranges cannot include more than 200 messages",
            ));
        }
        range_start_index = Some(from + 1);
        range_end_index = Some(to + 1);
        all_messages[from..=to].to_vec()
    } else {
        all_messages
            .iter()
            .rev()
            .take(context_size)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    };
    let selected = selected
        .into_iter()
        .filter(|message| !is_hidden_from_ai(message) && !message_content(message).is_empty())
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err(AppError::invalid_input(
            "No non-hidden messages available for summary generation",
        ));
    }
    let summary = build_local_summary(&selected);
    let now = now_iso();
    let entry = json!({
        "id": new_id(),
        "kind": "rolling",
        "origin": "manual",
        "title": if range_start_index.is_some() { "Summary range" } else { "Summary of recent messages" },
        "content": summary,
        "enabled": true,
        "sourceMode": if range_start_index.is_some() { "range" } else { "last" },
        "messageCount": selected.len(),
        "rangeStartIndex": range_start_index,
        "rangeEndIndex": range_end_index,
        "messageIds": selected.iter().filter_map(|message| message.get("id").and_then(Value::as_str)).collect::<Vec<_>>(),
        "promptTemplateId": body.get("promptTemplateId").cloned().unwrap_or(Value::Null),
        "tokenEstimate": estimate_tokens(&summary),
        "createdAt": now,
        "updatedAt": now
    });
    let mut entries = meta
        .get("summaryEntries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    entries.push(entry.clone());
    let compiled = entries
        .iter()
        .filter(|entry| {
            entry
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true)
        })
        .filter_map(|entry| entry.get("content").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut patch = Map::new();
    patch.insert("summary".to_string(), Value::String(compiled.clone()));
    patch.insert("summaryEntries".to_string(), Value::Array(entries.clone()));
    if range_start_index.is_none() && body.get("contextSize").is_some() {
        patch.insert("summaryContextSize".to_string(), json!(context_size));
    }
    merge_chat_metadata(state, chat_id, patch)?;
    Ok(json!({
        "summary": compiled,
        "entry": entry,
        "entries": entries,
        "messageIds": selected.iter().filter_map(|message| message.get("id").and_then(Value::as_str)).collect::<Vec<_>>()
    }))
}

pub(crate) fn backfill_summaries(state: &AppState, chat_id: &str, body: Value) -> AppResult<Value> {
    let chat = get_required(state, "chats", chat_id)?;
    let mut meta = metadata_map(&chat);
    let max_missing_days = body
        .get("maxMissingDays")
        .and_then(Value::as_u64)
        .unwrap_or(14)
        .clamp(1, 60) as usize;
    let mut day_summaries = meta
        .remove("daySummaries")
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    let messages = messages_for_chat(state, chat_id)?;
    let mut by_day: Map<String, Value> = Map::new();
    for message in messages
        .into_iter()
        .filter(|message| !is_hidden_from_ai(message) && !message_content(message).is_empty())
    {
        let key = message
            .get("createdAt")
            .and_then(Value::as_str)
            .and_then(|value| value.get(0..10))
            .unwrap_or("unknown")
            .to_string();
        by_day
            .entry(key)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .unwrap()
            .push(message);
    }
    let mut generated_days = Vec::new();
    let missing_days = by_day
        .into_iter()
        .filter(|(day, _)| !day_summaries.contains_key(day))
        .take(max_missing_days)
        .collect::<Vec<_>>();
    for (day, value) in missing_days {
        let day_messages = value.as_array().cloned().unwrap_or_default();
        let summary = build_local_summary(&day_messages);
        day_summaries.insert(day.clone(), json!({ "summary": summary, "keyDetails": [] }));
        generated_days.push(day);
    }
    let mut patch = Map::new();
    patch.insert("daySummaries".to_string(), Value::Object(day_summaries));
    merge_chat_metadata(state, chat_id, patch)?;
    Ok(json!({
        "generatedDays": generated_days,
        "consolidatedWeeks": [],
        "failedDays": [],
        "failedWeeks": [],
        "missingDayCount": generated_days.len(),
        "processedDayCount": generated_days.len(),
        "remainingMissingDayCount": 0
    }))
}

pub(crate) fn conversation_status(state: &AppState, chat_id: &str) -> AppResult<Value> {
    let chat = get_required(state, "chats", chat_id)?;
    let meta = metadata_map(&chat);
    let schedules = meta
        .get("characterSchedules")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut statuses = Map::new();
    for character_id in chat_character_ids(&chat) {
        let schedule = schedules.get(&character_id).cloned();
        statuses.insert(
            character_id,
            json!({
                "status": "online",
                "activity": if schedule.is_some() { "scheduled" } else { "unknown (no schedule)" },
                "schedule": schedule
            }),
        );
    }
    Ok(json!({ "statuses": statuses, "needsRefresh": false }))
}

pub(crate) fn autonomous_check(state: &AppState, body: Value) -> AppResult<Value> {
    let chat_id = required_string(&body, "chatId")?;
    let chat = get_required(state, "chats", chat_id)?;
    let meta = metadata_map(&chat);
    if !meta
        .get("autonomousMessages")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(
            json!({ "shouldTrigger": false, "characterIds": [], "reason": "disabled", "inactivityMs": 0 }),
        );
    }
    if body.get("userStatus").and_then(Value::as_str) == Some("dnd") {
        return Ok(
            json!({ "shouldTrigger": false, "characterIds": [], "reason": "user_dnd", "inactivityMs": 0 }),
        );
    }
    if meta.get("sceneStatus").and_then(Value::as_str) == Some("active") {
        return Ok(
            json!({ "shouldTrigger": false, "characterIds": [], "reason": "scene_active", "inactivityMs": 0 }),
        );
    }
    let messages = messages_for_chat(state, chat_id)?;
    let last = messages.last();
    let character_ids = chat_character_ids(&chat);
    if last
        .and_then(|message| message.get("role"))
        .and_then(Value::as_str)
        == Some("user")
        && !character_ids.is_empty()
    {
        return Ok(json!({
            "shouldTrigger": true,
            "characterIds": [character_ids[0].clone()],
            "reason": "user_inactivity",
            "inactivityMs": 0
        }));
    }
    Ok(
        json!({ "shouldTrigger": false, "characterIds": [], "reason": "waiting", "inactivityMs": 0 }),
    )
}

pub(crate) fn busy_delay(state: &AppState, body: Value) -> AppResult<Value> {
    let chat_id = required_string(&body, "chatId")?;
    let character_id = required_string(&body, "characterId")?;
    let chat = get_required(state, "chats", chat_id)?;
    let meta = metadata_map(&chat);
    let schedule = meta
        .get("characterSchedules")
        .and_then(Value::as_object)
        .and_then(|schedules| schedules.get(character_id));
    let delay_minutes = schedule
        .and_then(|schedule| {
            schedule
                .get("idleResponseDelayMinutes")
                .or_else(|| schedule.get("dndResponseDelayMinutes"))
        })
        .and_then(Value::as_u64)
        .unwrap_or(0);
    Ok(json!({
        "delayMs": delay_minutes * 60_000,
        "status": "online",
        "activity": if schedule.is_some() { "scheduled" } else { "unknown" }
    }))
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
            if state.storage.delete("chats", id)? {
                deleted += 1;
            }
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
