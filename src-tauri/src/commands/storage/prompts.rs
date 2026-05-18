use super::shared::*;
use super::*;
use marinara_security::is_allowed_outbound_url;

pub(crate) fn preset_full(state: &AppState, preset_id: &str) -> AppResult<Value> {
    Ok(json!({
        "preset": get_required(state, "prompts", preset_id)?,
        "sections": list_collection(state, "prompt-sections", Some(("presetId", preset_id)))?,
        "groups": list_collection(state, "prompt-groups", Some(("presetId", preset_id)))?,
        "choiceBlocks": list_collection(state, "prompt-variables", Some(("presetId", preset_id)))?,
    }))
}

pub(crate) fn prompt_nested_collection(nested: &str) -> &'static str {
    match nested {
        "groups" => "prompt-groups",
        "sections" => "prompt-sections",
        "variables" => "prompt-variables",
        _ => "prompt-items",
    }
}

pub(crate) fn prompt_nested_root(
    state: &AppState,
    method: &str,
    preset_id: &str,
    nested: &str,
    body: Value,
) -> AppResult<Value> {
    let collection = prompt_nested_collection(nested);
    match method {
        "GET" => list_collection(state, collection, Some(("presetId", preset_id))),
        "POST" => create_nested(state, collection, "presetId", preset_id, body),
        _ => Err(AppError::new(
            "method_not_allowed",
            "Unsupported prompt nested method",
        )),
    }
}

pub(crate) fn prompt_nested_item(
    state: &AppState,
    method: &str,
    preset_id: &str,
    nested: &str,
    nested_id: &str,
    body: Value,
) -> AppResult<Value> {
    nested_item(
        state,
        method,
        prompt_nested_collection(nested),
        "presetId",
        preset_id,
        nested_id,
        body,
    )
}

pub(crate) fn reorder_prompt_nested(
    state: &AppState,
    preset_id: &str,
    nested: &str,
    body: Value,
) -> AppResult<Value> {
    let ids_field = match nested {
        "groups" => "groupIds",
        "sections" => "sectionIds",
        "variables" => "variableIds",
        _ => "ids",
    };
    let collection = prompt_nested_collection(nested);
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
    let order_field = match nested {
        "groups" => "groupOrder",
        "sections" => "sectionOrder",
        "variables" => "variableOrder",
        _ => "itemOrder",
    };
    let _ = state.storage.patch(
        "prompts",
        preset_id,
        json!({ order_field: serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_string()) }),
    );
    list_collection(state, collection, Some(("presetId", preset_id)))
}

pub(crate) fn default_prompt(state: &AppState) -> AppResult<Value> {
    Ok(state
        .storage
        .list("prompts")?
        .into_iter()
        .find(|prompt| {
            prompt
                .get("isDefault")
                .or_else(|| prompt.get("default"))
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .unwrap_or(Value::Null))
}

pub(crate) fn set_default_prompt(state: &AppState, id: &str) -> AppResult<Value> {
    for prompt in state.storage.list("prompts")? {
        if let Some(prompt_id) = prompt.get("id").and_then(Value::as_str) {
            let is_selected = prompt_id == id;
            state.storage.patch(
                "prompts",
                prompt_id,
                json!({ "isDefault": is_selected, "default": is_selected }),
            )?;
        }
    }
    get_required(state, "prompts", id)
}

pub(crate) fn create_nested(
    state: &AppState,
    collection: &str,
    parent_field: &str,
    parent_id: &str,
    body: Value,
) -> AppResult<Value> {
    let mut object = ensure_object(body)?;
    object.insert(
        parent_field.to_string(),
        Value::String(parent_id.to_string()),
    );
    state.storage.create(collection, Value::Object(object))
}

pub(crate) fn nested_item(
    state: &AppState,
    method: &str,
    collection: &str,
    _parent_field: &str,
    _parent_id: &str,
    id: &str,
    body: Value,
) -> AppResult<Value> {
    collection_item_or_action(state, method, collection, id, None, body)
}

pub(crate) fn create_lorebook_entries_bulk(
    state: &AppState,
    lorebook_id: &str,
    body: Value,
) -> AppResult<Value> {
    let entries = body
        .get("entries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut created = Vec::new();
    for entry in entries {
        created.push(create_nested(
            state,
            "lorebook-entries",
            "lorebookId",
            lorebook_id,
            entry,
        )?);
    }
    Ok(Value::Array(created))
}

pub(crate) fn reorder_lorebook_entries(
    state: &AppState,
    lorebook_id: &str,
    body: Value,
) -> AppResult<Value> {
    let ids = string_array_from_value(body.get("entryIds"));
    let folder_present = body.get("folderId").is_some();
    let folder_id = body.get("folderId").cloned().unwrap_or(Value::Null);
    for (index, id) in ids.iter().enumerate() {
        if state.storage.get("lorebook-entries", id)?.is_some() {
            let mut patch = json!({ "order": index as i64, "sortOrder": index as i64 });
            if folder_present {
                patch["folderId"] = folder_id.clone();
            }
            state.storage.patch("lorebook-entries", id, patch)?;
        }
    }
    list_collection(state, "lorebook-entries", Some(("lorebookId", lorebook_id)))
}

pub(crate) fn reorder_lorebook_folders(
    state: &AppState,
    lorebook_id: &str,
    body: Value,
) -> AppResult<Value> {
    let ids = string_array_from_value(body.get("folderIds"));
    for (index, id) in ids.iter().enumerate() {
        if state.storage.get("lorebook-folders", id)?.is_some() {
            state.storage.patch(
                "lorebook-folders",
                id,
                json!({ "order": index as i64, "sortOrder": index as i64 }),
            )?;
        }
    }
    list_collection(state, "lorebook-folders", Some(("lorebookId", lorebook_id)))
}

pub(crate) fn transfer_lorebook_entries(
    state: &AppState,
    source_lorebook_id: &str,
    body: Value,
) -> AppResult<Value> {
    let target_lorebook_id = required_string(&body, "targetLorebookId")?;
    let operation = body
        .get("operation")
        .and_then(Value::as_str)
        .unwrap_or("copy");
    let entry_ids = string_array_from_value(body.get("entryIds"));
    let mut created = Vec::new();
    for id in &entry_ids {
        let Some(mut entry) = state.storage.get("lorebook-entries", id)? else {
            continue;
        };
        if entry.get("lorebookId").and_then(Value::as_str) != Some(source_lorebook_id) {
            continue;
        }
        if operation == "move" {
            created.push(state.storage.patch(
                "lorebook-entries",
                id,
                json!({ "lorebookId": target_lorebook_id }),
            )?);
        } else {
            if let Some(object) = entry.as_object_mut() {
                object.remove("id");
                object.insert(
                    "lorebookId".to_string(),
                    Value::String(target_lorebook_id.to_string()),
                );
            }
            created.push(state.storage.create("lorebook-entries", entry)?);
        }
    }
    Ok(json!({
        "operation": if operation == "move" { "move" } else { "copy" },
        "sourceLorebookId": source_lorebook_id,
        "targetLorebookId": target_lorebook_id,
        "requested": entry_ids.len(),
        "transferred": created.len(),
        "created": created
    }))
}

pub(crate) fn search_lorebook_entries(state: &AppState, query: &str) -> AppResult<Value> {
    let needle = query.trim().to_ascii_lowercase();
    if needle.len() < 2 {
        return Ok(json!([]));
    }
    let entries = state
        .storage
        .list("lorebook-entries")?
        .into_iter()
        .filter(|entry| {
            let haystack = [
                entry.get("name").and_then(Value::as_str).unwrap_or(""),
                entry.get("content").and_then(Value::as_str).unwrap_or(""),
                entry.get("comment").and_then(Value::as_str).unwrap_or(""),
            ]
            .join("\n")
            .to_ascii_lowercase();
            haystack.contains(&needle)
                || value_string_array(entry.get("keys"))
                    .iter()
                    .any(|key| key.to_ascii_lowercase().contains(&needle))
        })
        .collect::<Vec<_>>();
    Ok(Value::Array(entries))
}

pub(crate) fn scan_lorebooks(state: &AppState, chat_id: &str) -> AppResult<Value> {
    let messages = super::chats::messages_for_chat(state, chat_id)?;
    let context = messages
        .iter()
        .rev()
        .take(30)
        .map(|message| message.get("content").and_then(Value::as_str).unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
        .to_ascii_lowercase();
    let lorebooks = state.storage.list("lorebooks")?;
    let enabled_lorebook_ids = lorebooks
        .iter()
        .filter(|book| book.get("enabled").and_then(Value::as_bool).unwrap_or(true))
        .filter_map(|book| {
            book.get("id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .collect::<std::collections::HashSet<_>>();
    let mut active = Vec::new();
    for entry in state.storage.list("lorebook-entries")? {
        let lorebook_id = entry
            .get("lorebookId")
            .and_then(Value::as_str)
            .unwrap_or("");
        if !enabled_lorebook_ids.contains(lorebook_id)
            || !entry
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true)
        {
            continue;
        }
        let constant = entry
            .get("constant")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let keys = value_string_array(entry.get("keys"));
        let matched = constant
            || keys
                .iter()
                .filter(|key| !key.trim().is_empty())
                .any(|key| context.contains(&key.to_ascii_lowercase()));
        if matched {
            active.push(json!({
                "id": entry.get("id").cloned().unwrap_or(Value::Null),
                "name": entry.get("name").and_then(Value::as_str).unwrap_or("Entry"),
                "content": entry.get("content").and_then(Value::as_str).unwrap_or(""),
                "keys": keys,
                "lorebookId": lorebook_id,
                "order": entry.get("order").or_else(|| entry.get("sortOrder")).and_then(Value::as_i64).unwrap_or(0),
                "constant": constant
            }));
        }
    }
    let total_tokens = active
        .iter()
        .map(|entry| {
            entry
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .len()
                / 4
        })
        .sum::<usize>();
    let total_entries = active.len();
    Ok(json!({
        "entries": active,
        "budgetSkippedEntries": [],
        "totalTokens": total_tokens,
        "totalEntries": total_entries
    }))
}

pub(crate) async fn vectorize_lorebook(
    state: &AppState,
    lorebook_id: &str,
    body: Value,
) -> AppResult<Value> {
    let connection_id = required_string(&body, "connectionId")?;
    let mut connection = get_required(state, "connections", connection_id)?;
    let model = body
        .get("model")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| connection.get("embeddingModel").and_then(Value::as_str))
        .ok_or_else(|| AppError::invalid_input("Embedding model is required"))?
        .to_string();
    if let Some(object) = connection.as_object_mut() {
        object.insert("model".to_string(), Value::String(model.clone()));
    }
    let only_missing = body
        .get("onlyMissing")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let entries = match list_collection(
        state,
        "lorebook-entries",
        Some(("lorebookId", lorebook_id)),
    )? {
        Value::Array(rows) => rows,
        _ => Vec::new(),
    };
    let total = entries
        .iter()
        .filter(|entry| {
            !entry
                .get("excludeFromVectorization")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .count();
    let mut vectorized = 0usize;
    let mut skipped = 0usize;
    for entry in entries {
        if entry
            .get("excludeFromVectorization")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            skipped += 1;
            continue;
        }
        if only_missing
            && entry
                .get("embedding")
                .and_then(Value::as_array)
                .is_some_and(|embedding| !embedding.is_empty())
        {
            skipped += 1;
            continue;
        }
        let Some(entry_id) = entry.get("id").and_then(Value::as_str) else {
            skipped += 1;
            continue;
        };
        let text = lorebook_entry_embedding_text(&entry);
        if text.trim().is_empty() {
            skipped += 1;
            continue;
        }
        let embedding = embed_text(&connection, &model, &text).await?;
        state.storage.patch(
            "lorebook-entries",
            entry_id,
            json!({
                "embedding": embedding,
                "embeddingModel": model,
                "embeddingConnectionId": connection_id,
                "embeddingUpdatedAt": now_iso()
            }),
        )?;
        vectorized += 1;
    }
    Ok(json!({
        "success": true,
        "lorebookId": lorebook_id,
        "model": model,
        "total": total,
        "vectorized": vectorized,
        "skipped": skipped
    }))
}

pub(crate) fn value_string_array(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect(),
        Some(Value::String(raw)) => serde_json::from_str::<Vec<String>>(raw).unwrap_or_else(|_| {
            raw.split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        }),
        _ => Vec::new(),
    }
}

fn lorebook_entry_embedding_text(entry: &Value) -> String {
    let keys = value_string_array(entry.get("keys")).join(", ");
    [
        entry.get("name").and_then(Value::as_str).unwrap_or(""),
        keys.as_str(),
        entry.get("description").and_then(Value::as_str).unwrap_or(""),
        entry.get("content").and_then(Value::as_str).unwrap_or(""),
    ]
    .into_iter()
    .filter(|part| !part.trim().is_empty())
    .collect::<Vec<_>>()
    .join("\n")
}

async fn embed_text(connection: &Value, model: &str, text: &str) -> AppResult<Vec<f64>> {
    let provider = connection
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or("openai");
    match provider {
        "google" | "google_vertex" => embed_google(connection, model, text).await,
        "ollama" => embed_ollama(connection, model, text).await,
        _ => embed_openai_compatible(connection, model, text).await,
    }
}

async fn embed_openai_compatible(
    connection: &Value,
    model: &str,
    text: &str,
) -> AppResult<Vec<f64>> {
    let base = embedding_base_url(connection, "https://api.openai.com/v1");
    let url = format!("{base}/embeddings");
    ensure_embedding_url_allowed(&url)?;
    let mut request = reqwest::Client::new()
        .post(url)
        .json(&json!({ "model": model, "input": text }));
    if let Some(api_key) = connection
        .get("apiKey")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        request = request.bearer_auth(api_key.trim());
    }
    let response = request
        .send()
        .await
        .map_err(|error| AppError::new("embedding_network_error", error.to_string()))?;
    parse_embedding_response(response, |json| {
        json.get("data")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|item| item.get("embedding"))
            .and_then(json_embedding_array)
    })
    .await
}

async fn embed_google(connection: &Value, model: &str, text: &str) -> AppResult<Vec<f64>> {
    let api_key = connection.get("apiKey").and_then(Value::as_str).unwrap_or("");
    let base = embedding_base_url(connection, "https://generativelanguage.googleapis.com");
    let url = format!("{base}/v1beta/models/{model}:embedContent?key={api_key}");
    ensure_embedding_url_allowed(&url)?;
    let response = reqwest::Client::new()
        .post(url)
        .json(&json!({ "content": { "parts": [{ "text": text }] } }))
        .send()
        .await
        .map_err(|error| AppError::new("embedding_network_error", error.to_string()))?;
    parse_embedding_response(response, |json| {
        json.get("embedding")
            .and_then(|embedding| embedding.get("values"))
            .and_then(json_embedding_array)
    })
    .await
}

async fn embed_ollama(connection: &Value, model: &str, text: &str) -> AppResult<Vec<f64>> {
    let base = embedding_base_url(connection, "http://127.0.0.1:11434");
    let url = format!("{base}/api/embeddings");
    ensure_embedding_url_allowed(&url)?;
    let response = reqwest::Client::new()
        .post(url)
        .json(&json!({ "model": model, "prompt": text }))
        .send()
        .await
        .map_err(|error| AppError::new("embedding_network_error", error.to_string()))?;
    parse_embedding_response(response, |json| {
        json.get("embedding").and_then(json_embedding_array)
    })
    .await
}

async fn parse_embedding_response<F>(response: reqwest::Response, extractor: F) -> AppResult<Vec<f64>>
where
    F: Fn(&Value) -> Option<Vec<f64>>,
{
    let status = response.status();
    let json: Value = response
        .json()
        .await
        .map_err(|error| AppError::new("embedding_response_error", error.to_string()))?;
    if !status.is_success() {
        return Err(AppError::with_details(
            "embedding_provider_error",
            format!("Embedding provider returned HTTP {status}"),
            json,
        ));
    }
    extractor(&json).filter(|embedding| !embedding.is_empty()).ok_or_else(|| {
        AppError::with_details(
            "embedding_response_error",
            "Embedding response did not contain a numeric embedding",
            json,
        )
    })
}

fn json_embedding_array(value: &Value) -> Option<Vec<f64>> {
    Some(
        value
            .as_array()?
            .iter()
            .filter_map(Value::as_f64)
            .collect::<Vec<_>>(),
    )
}

fn embedding_base_url(connection: &Value, fallback: &str) -> String {
    connection
        .get("baseUrl")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .trim_end_matches('/')
        .to_string()
}

fn ensure_embedding_url_allowed(url: &str) -> AppResult<()> {
    if is_allowed_outbound_url(url, true) {
        Ok(())
    } else {
        Err(AppError::invalid_input(format!(
            "Outbound embedding URL is not allowed: {url}"
        )))
    }
}
