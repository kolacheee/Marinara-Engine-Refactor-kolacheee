use super::chats::messages_for_chat;
use super::shared::*;
use super::*;

fn parse_settings(value: Option<&Value>) -> Map<String, Value> {
    match value {
        Some(Value::Object(object)) => object.clone(),
        Some(Value::String(raw)) => serde_json::from_str::<Value>(raw)
            .ok()
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default(),
        _ => Map::new(),
    }
}

fn find_agent_config(state: &AppState, agent_type: &str) -> AppResult<Option<Value>> {
    if let Some(agent) = find_by_field(state, "agents", "type", agent_type)? {
        return Ok(Some(agent));
    }
    find_by_field(state, "agents", "agentType", agent_type)
}

fn get_or_create_agent_config(state: &AppState, agent_type: &str) -> AppResult<Value> {
    if let Some(agent) = find_agent_config(state, agent_type)? {
        return Ok(agent);
    }
    state.storage.create(
        "agents",
        json!({
            "type": agent_type,
            "name": agent_type,
            "enabled": true,
            "settings": {}
        }),
    )
}

fn agent_config_id(state: &AppState, agent_type: &str, create: bool) -> AppResult<Option<String>> {
    let agent = if create {
        Some(get_or_create_agent_config(state, agent_type)?)
    } else {
        find_agent_config(state, agent_type)?
    };
    Ok(agent.and_then(|agent| agent.get("id").and_then(Value::as_str).map(str::to_string)))
}

fn run_agent_type(run: &Value) -> Option<&str> {
    run.get("agentType")
        .or_else(|| run.get("type"))
        .and_then(Value::as_str)
}

fn run_message_id(run: &Value) -> Option<&str> {
    run.get("messageId").and_then(Value::as_str)
}

pub(crate) fn toggle_agent_type(state: &AppState, agent_type: &str) -> AppResult<Value> {
    if let Some(agent) = find_agent_config(state, agent_type)? {
        let id = agent
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or(agent_type);
        let enabled = !agent
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        state
            .storage
            .patch("agents", id, json!({ "enabled": enabled }))
    } else {
        state
            .storage
            .create("agents", json!({ "type": agent_type, "enabled": true }))
    }
}

pub(crate) fn patch_agent_type(
    state: &AppState,
    agent_type: &str,
    body: Value,
) -> AppResult<Value> {
    if let Some(agent) = find_agent_config(state, agent_type)? {
        let id = agent
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or(agent_type);
        state.storage.patch("agents", id, body)
    } else {
        let mut object = ensure_object(body)?;
        object.insert("type".to_string(), Value::String(agent_type.to_string()));
        state.storage.create("agents", Value::Object(object))
    }
}

pub(crate) fn agent_cadence_status(
    state: &AppState,
    agent_type: &str,
    chat_id: &str,
) -> AppResult<Value> {
    let config = find_agent_config(state, agent_type)?;
    let settings = parse_settings(config.as_ref().and_then(|agent| agent.get("settings")));
    let run_interval = settings
        .get("runInterval")
        .and_then(Value::as_i64)
        .or_else(|| {
            settings
                .get("runInterval")
                .and_then(Value::as_str)
                .and_then(|value| value.parse().ok())
        })
        .unwrap_or(1)
        .clamp(1, 100);
    let messages = messages_for_chat(state, chat_id)?;
    let runs = state
        .storage
        .list_where("agent-runs", &{
            let mut filters = Map::new();
            filters.insert("chatId".to_string(), Value::String(chat_id.to_string()));
            filters
        })?
        .into_iter()
        .filter(|run| run_agent_type(run) == Some(agent_type))
        .collect::<Vec<_>>();
    let last_run = runs
        .iter()
        .filter(|run| !run.get("error").and_then(Value::as_bool).unwrap_or(false))
        .max_by(|a, b| {
            let a_time = a.get("createdAt").and_then(Value::as_str).unwrap_or("");
            let b_time = b.get("createdAt").and_then(Value::as_str).unwrap_or("");
            a_time.cmp(b_time)
        });
    let mut assistant_messages_since_last_run = None;
    let mut last_run_message_found = None;
    if let Some(run) = last_run {
        if let Some(message_id) = run_message_id(run) {
            if let Some(index) = messages
                .iter()
                .position(|message| message.get("id").and_then(Value::as_str) == Some(message_id))
            {
                last_run_message_found = Some(true);
                let count = messages[index + 1..]
                    .iter()
                    .filter(|message| {
                        message.get("role").and_then(Value::as_str) == Some("assistant")
                    })
                    .count() as i64;
                assistant_messages_since_last_run = Some(count);
            } else {
                last_run_message_found = Some(false);
                assistant_messages_since_last_run = Some(run_interval);
            }
        }
    }
    let remaining = if last_run.is_none() || run_interval <= 1 {
        0
    } else {
        (run_interval - (assistant_messages_since_last_run.unwrap_or(0) + 1)).max(0)
    };
    Ok(json!({
        "agentType": agent_type,
        "runInterval": run_interval,
        "lastSuccessfulRun": last_run.map(|run| json!({
            "messageId": run.get("messageId").cloned().unwrap_or(Value::Null),
            "createdAt": run.get("createdAt").cloned().unwrap_or(Value::Null)
        })),
        "assistantMessagesSinceLastRun": assistant_messages_since_last_run,
        "remainingAssistantMessages": remaining,
        "runsNextAssistantMessage": remaining == 0,
        "lastRunMessageFound": last_run_message_found
    }))
}

pub(crate) fn agent_memory(
    state: &AppState,
    method: &str,
    agent_type: &str,
    chat_id: &str,
    body: Value,
) -> AppResult<Value> {
    match method {
        "GET" => {
            let Some(agent_config_id) = agent_config_id(state, agent_type, false)? else {
                return Err(AppError::not_found("Agent is not configured"));
            };
            Ok(json!({
                "agentConfigId": agent_config_id,
                "memory": read_agent_memory(state, &agent_config_id, chat_id)?
            }))
        }
        "PATCH" => {
            let agent_config_id = agent_config_id(state, agent_type, true)?
                .ok_or_else(|| AppError::not_found("Agent is not configured"))?;
            let patch = body
                .get("patch")
                .and_then(Value::as_object)
                .cloned()
                .ok_or_else(|| {
                    AppError::invalid_input("Body must be { patch: { key: value, ... } }")
                })?;
            for (key, value) in patch {
                set_agent_memory_value(state, &agent_config_id, chat_id, &key, value)?;
            }
            Ok(json!({
                "agentConfigId": agent_config_id,
                "memory": read_agent_memory(state, &agent_config_id, chat_id)?
            }))
        }
        "DELETE" => {
            if let Some(agent_config_id) = agent_config_id(state, agent_type, false)? {
                clear_agent_memory(state, &agent_config_id, chat_id)?;
            }
            Ok(json!({ "deleted": true }))
        }
        _ => Err(AppError::new(
            "method_not_allowed",
            "Unsupported agent memory method",
        )),
    }
}

fn read_agent_memory(
    state: &AppState,
    agent_config_id: &str,
    chat_id: &str,
) -> AppResult<Map<String, Value>> {
    let mut filters = Map::new();
    filters.insert(
        "agentConfigId".to_string(),
        Value::String(agent_config_id.to_string()),
    );
    filters.insert("chatId".to_string(), Value::String(chat_id.to_string()));
    let mut memory = Map::new();
    for row in state.storage.list_where("agent-memory", &filters)? {
        let Some(key) = row.get("key").and_then(Value::as_str) else {
            continue;
        };
        let value = row.get("value").cloned().unwrap_or(Value::Null);
        let parsed = match value {
            Value::String(raw) => serde_json::from_str::<Value>(&raw).unwrap_or(Value::String(raw)),
            other => other,
        };
        memory.insert(key.to_string(), parsed);
    }
    Ok(memory)
}

fn set_agent_memory_value(
    state: &AppState,
    agent_config_id: &str,
    chat_id: &str,
    key: &str,
    value: Value,
) -> AppResult<()> {
    let mut filters = Map::new();
    filters.insert(
        "agentConfigId".to_string(),
        Value::String(agent_config_id.to_string()),
    );
    filters.insert("chatId".to_string(), Value::String(chat_id.to_string()));
    filters.insert("key".to_string(), Value::String(key.to_string()));
    let stored_value = match value {
        Value::String(raw) => Value::String(raw),
        other => Value::String(serde_json::to_string(&other)?),
    };
    if let Some(existing) = state
        .storage
        .list_where("agent-memory", &filters)?
        .into_iter()
        .next()
    {
        let id = existing
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::invalid_input("Agent memory row is missing id"))?;
        state
            .storage
            .patch("agent-memory", id, json!({ "value": stored_value }))?;
    } else {
        state.storage.create(
            "agent-memory",
            json!({
                "agentConfigId": agent_config_id,
                "chatId": chat_id,
                "key": key,
                "value": stored_value
            }),
        )?;
    }
    Ok(())
}

fn clear_agent_memory(state: &AppState, agent_config_id: &str, chat_id: &str) -> AppResult<()> {
    let mut filters = Map::new();
    filters.insert(
        "agentConfigId".to_string(),
        Value::String(agent_config_id.to_string()),
    );
    filters.insert("chatId".to_string(), Value::String(chat_id.to_string()));
    for row in state.storage.list_where("agent-memory", &filters)? {
        if let Some(id) = row.get("id").and_then(Value::as_str) {
            state.storage.delete("agent-memory", id)?;
        }
    }
    Ok(())
}

pub(crate) fn echo_messages(state: &AppState, method: &str, chat_id: &str) -> AppResult<Value> {
    let mut filters = Map::new();
    filters.insert("chatId".to_string(), Value::String(chat_id.to_string()));
    let rows = state.storage.list_where("agent-runs", &filters)?;
    match method {
        "GET" => Ok(Value::Array(
            rows.into_iter()
                .filter(|run| run.get("resultType").and_then(Value::as_str) == Some("echo_message"))
                .collect(),
        )),
        "DELETE" => {
            let mut deleted = 0;
            for row in rows
                .into_iter()
                .filter(|run| run.get("resultType").and_then(Value::as_str) == Some("echo_message"))
            {
                if let Some(id) = row.get("id").and_then(Value::as_str) {
                    if state.storage.delete("agent-runs", id)? {
                        deleted += 1;
                    }
                }
            }
            Ok(json!({ "deleted": deleted }))
        }
        _ => Err(AppError::new(
            "method_not_allowed",
            "Unsupported echo messages method",
        )),
    }
}

pub(crate) fn retry_agents(state: &AppState, body: Value) -> AppResult<Value> {
    let chat_id = required_string(&body, "chatId")?;
    let agent_types = string_array_from_value(body.get("agentTypes"));
    let for_message_id = body
        .get("options")
        .and_then(|options| options.get("forMessageId"))
        .and_then(Value::as_str);
    let target_message = for_message_id.and_then(|id| {
        messages_for_chat(state, chat_id)
            .ok()?
            .into_iter()
            .find(|message| message.get("id").and_then(Value::as_str) == Some(id))
    });
    let injections = target_message
        .as_ref()
        .and_then(|message| message.get("extra"))
        .and_then(|extra| match extra {
            Value::Object(object) => Some(Value::Object(object.clone())),
            Value::String(raw) => serde_json::from_str::<Value>(raw).ok(),
            _ => None,
        })
        .and_then(|extra| {
            extra
                .get("contextInjections")
                .and_then(Value::as_array)
                .cloned()
        })
        .unwrap_or_default();
    let mut results = Vec::new();
    for agent_type in agent_types {
        let cached = injections.iter().find(|entry| {
            entry.get("agentType").and_then(Value::as_str) == Some(agent_type.as_str())
        });
        let result = json!({
            "agentType": agent_type,
            "messageId": for_message_id,
            "result": cached.cloned().unwrap_or_else(|| json!({ "text": "" })),
            "reusedCachedInjection": cached.is_some()
        });
        state.storage.create(
            "agent-runs",
            json!({
                "chatId": chat_id,
                "agentType": result.get("agentType").cloned().unwrap_or(Value::Null),
                "messageId": for_message_id,
                "resultType": "context_injection",
                "resultData": result.get("result").cloned().unwrap_or(Value::Null)
            }),
        )?;
        results.push(result);
    }
    Ok(json!({ "results": results }))
}
