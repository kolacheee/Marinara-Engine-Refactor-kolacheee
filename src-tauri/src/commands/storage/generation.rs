use super::chats::{chat_messages, message_swipes, messages_for_chat};
use super::llm::llm_connection_from_value;
use super::shared::*;
use super::*;

pub(crate) async fn generate_events(state: &AppState, body: Value) -> AppResult<Vec<Value>> {
    let started_at = now_millis();
    let chat_id = body
        .get("chatId")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::invalid_input("chatId is required"))?;
    let message = body
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();

    if !message.is_empty()
        && !body
            .get("impersonate")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && body
            .get("regenerateMessageId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .is_empty()
    {
        chat_messages(
            state,
            "POST",
            chat_id,
            json!({ "role": "user", "content": message }),
            &HashMap::new(),
        )?;
    }

    let connection = resolve_generation_connection(state, chat_id, &body)?;
    let mut prompt_messages = request_messages_from_body(&body).unwrap_or_else(|| {
        messages_for_chat(state, chat_id)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|message| llm_message_from_value(&message))
            .collect()
    });
    if let Some(guide) = body
        .get("generationGuide")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        prompt_messages.push(marinara_llm::LlmMessage {
            role: "user".to_string(),
            content: guide.to_string(),
        });
    }
    if prompt_messages.is_empty() {
        return Err(AppError::invalid_input(
            "Cannot generate without prompt messages",
        ));
    }

    let request = marinara_llm::LlmRequest {
        connection: llm_connection_from_value(&connection)?,
        messages: prompt_messages,
        parameters: generation_parameters(&connection, &body),
    };
    let mut events = vec![json!({ "type": "phase", "data": "Calling model..." })];
    let content = marinara_llm::complete(request).await?;
    if generation_abort_requested_since(state, chat_id, started_at)? {
        return Ok(vec![json!({ "type": "done", "data": { "aborted": true } })]);
    }

    let saved = save_generated_message(state, chat_id, &body, &connection, &content)?;
    events.push(json!({ "type": "token", "data": content }));
    if let Some(saved) = saved {
        events.push(json!({ "type": "assistant_message", "data": saved }));
    }
    events.push(json!({ "type": "done", "data": null }));
    Ok(events)
}

pub(crate) fn abort_generation(state: &AppState, body: Value) -> AppResult<Value> {
    let chat_id = required_string(&body, "chatId")?;
    let key = generation_abort_key(chat_id);
    state.storage.upsert_with_id(
        "app-settings",
        &key,
        json!({ "id": key, "value": now_millis().to_string(), "chatId": chat_id }),
    )?;
    Ok(json!({ "aborted": true, "status": "abort_requested", "chatId": chat_id }))
}

pub(crate) async fn test_connection(state: &AppState, id: &str) -> AppResult<Value> {
    let started = std::time::Instant::now();
    let connection = get_required(state, "connections", id)?;
    let request = marinara_llm::LlmRequest {
        connection: llm_connection_from_value(&connection)?,
        messages: vec![marinara_llm::LlmMessage {
            role: "user".to_string(),
            content: "Reply with exactly: ok".to_string(),
        }],
        parameters: json!({ "maxTokens": 16, "temperature": 0 }),
    };
    let response = marinara_llm::complete(request).await?;
    Ok(json!({
        "success": true,
        "message": response,
        "latencyMs": started.elapsed().as_millis(),
        "modelName": connection.get("model").and_then(Value::as_str)
    }))
}

pub(crate) async fn test_message(state: &AppState, id: &str) -> AppResult<Value> {
    let started = std::time::Instant::now();
    let connection = get_required(state, "connections", id)?;
    let request = marinara_llm::LlmRequest {
        connection: llm_connection_from_value(&connection)?,
        messages: vec![marinara_llm::LlmMessage {
            role: "user".to_string(),
            content: "hi".to_string(),
        }],
        parameters: json!({ "maxTokens": 64, "temperature": 0.7 }),
    };
    let response = marinara_llm::complete(request).await?;
    Ok(json!({
        "success": true,
        "response": response,
        "latencyMs": started.elapsed().as_millis()
    }))
}

pub(crate) fn resolve_generation_connection(
    state: &AppState,
    chat_id: &str,
    body: &Value,
) -> AppResult<Value> {
    if let Some(connection_id) = body
        .get("connectionId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
    {
        return get_required(state, "connections", connection_id);
    }
    let chat = get_required(state, "chats", chat_id)?;
    if let Some(connection_id) = chat
        .get("connectionId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
    {
        return get_required(state, "connections", connection_id);
    }
    let connections = state.storage.list("connections")?;
    if let Some(default) = connections
        .iter()
        .find(|connection| {
            connection
                .get("isDefault")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .cloned()
    {
        return Ok(default);
    }
    connections
        .into_iter()
        .next()
        .ok_or_else(|| AppError::invalid_input("No LLM connection is configured"))
}

fn request_messages_from_body(body: &Value) -> Option<Vec<marinara_llm::LlmMessage>> {
    let messages = body.get("messages")?.as_array()?;
    let mapped = messages
        .iter()
        .filter_map(llm_message_from_value)
        .collect::<Vec<_>>();
    if mapped.is_empty() {
        None
    } else {
        Some(mapped)
    }
}

fn llm_message_from_value(message: &Value) -> Option<marinara_llm::LlmMessage> {
    let content = message.get("content").and_then(Value::as_str)?.trim();
    if content.is_empty() {
        return None;
    }
    let role = match message.get("role").and_then(Value::as_str).unwrap_or("user") {
        "system" => "system",
        "assistant" => "assistant",
        "tool" => "tool",
        _ => "user",
    };
    Some(marinara_llm::LlmMessage {
        role: role.to_string(),
        content: content.to_string(),
    })
}

fn generation_parameters(connection: &Value, body: &Value) -> Value {
    let mut base = connection
        .get("defaultParameters")
        .and_then(Value::as_str)
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    if let Some(next) = body.get("parameters").and_then(Value::as_object) {
        for (key, value) in next {
            base.insert(key.clone(), value.clone());
        }
    }
    Value::Object(base)
}

fn save_generated_message(
    state: &AppState,
    chat_id: &str,
    body: &Value,
    connection: &Value,
    content: &str,
) -> AppResult<Option<Value>> {
    if body
        .get("impersonate")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(None);
    }
    if let Some(message_id) = body
        .get("regenerateMessageId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
    {
        return message_swipes(
            state,
            "POST",
            chat_id,
            message_id,
            json!({ "content": content }),
        )
        .map(Some);
    }
    chat_messages(
        state,
        "POST",
        chat_id,
        json!({
            "role": "assistant",
            "content": content,
            "generationInfo": {
                "connectionId": connection.get("id").cloned().unwrap_or(Value::Null),
                "model": connection.get("model").cloned().unwrap_or(Value::Null)
            }
        }),
        &HashMap::new(),
    )
    .map(Some)
}

fn generation_abort_requested_since(
    state: &AppState,
    chat_id: &str,
    started_at: u128,
) -> AppResult<bool> {
    let key = generation_abort_key(chat_id);
    Ok(state
        .storage
        .get("app-settings", &key)?
        .and_then(|record| record.get("value").and_then(Value::as_str).map(ToOwned::to_owned))
        .and_then(|value| value.parse::<u128>().ok())
        .is_some_and(|aborted_at| aborted_at >= started_at))
}

fn generation_abort_key(chat_id: &str) -> String {
    format!("generation-abort-{chat_id}")
}
