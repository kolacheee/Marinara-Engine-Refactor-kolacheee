use super::shared::*;
use super::*;

use super::chats::{chat_messages, delete_chat_with_messages, messages_for_chat};
use super::generation::resolve_generation_connection;
use super::llm::{llm_connection_from_value, resolve_llm_connection_for_request};

pub(crate) fn patch_metadata_map(
    state: &AppState,
    chat_id: &str,
    metadata: Map<String, Value>,
) -> AppResult<Value> {
    state.storage.patch(
        "chats",
        chat_id,
        json!({ "metadata": Value::Object(metadata) }),
    )
}

pub(crate) fn clean_scene_pointers(metadata: &mut Map<String, Value>) {
    metadata.insert("activeSceneChatId".to_string(), Value::Null);
    metadata.insert("sceneBusyCharIds".to_string(), Value::Null);
}

pub(crate) fn safe_title(value: &str, fallback: &str) -> String {
    let title = value
        .trim()
        .replace('\r', " ")
        .replace('\n', " ")
        .replace('\t', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let title = if title.is_empty() {
        fallback.trim()
    } else {
        &title
    };
    let clipped: String = title.chars().take(60).collect();
    if clipped.starts_with("Scene:") {
        clipped
    } else {
        format!("Scene: {clipped}")
    }
}

pub(crate) fn fallback_scene_plan(
    state: &AppState,
    chat_id: &str,
    prompt: &str,
) -> AppResult<Value> {
    let chat = get_required(state, "chats", chat_id)?;
    let character_ids = string_array_from_value(chat.get("characterIds"));
    let history = messages_for_chat(state, chat_id)?
        .into_iter()
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .filter_map(|message| {
            let role = message
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("user");
            let content = message
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            (!content.is_empty()).then(|| format!("{role}: {content}"))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let premise = if prompt.trim().is_empty() {
        history
            .lines()
            .last()
            .unwrap_or("A focused roleplay scene continues from the current conversation.")
            .to_string()
    } else {
        prompt.trim().to_string()
    };
    Ok(json!({
        "name": safe_title(&premise, "New Scene"),
        "description": format!("The scene opens around this premise: {premise}"),
        "scenario": if history.is_empty() {
            premise.clone()
        } else {
            format!("Use the recent conversation as continuity and develop this premise: {premise}\n\nRecent context:\n{history}")
        },
        "firstMessage": format!("The moment settles into focus. {premise}"),
        "background": Value::Null,
        "characterIds": character_ids,
        "systemPrompt": "Write immersive roleplay prose with consistent point of view, clear character agency, and continuity from the originating conversation.",
        "rating": "sfw",
        "relationshipHistory": if history.is_empty() { "" } else { history.as_str() },
        "participationGuide": "Play the scene naturally and respond as your character would."
    }))
}

pub(crate) fn parse_json_object_from_text(raw: &str) -> Option<Value> {
    let cleaned = raw
        .replace("```json", "")
        .replace("```JSON", "")
        .replace("```", "");
    let first = cleaned.find('{')?;
    let last = cleaned.rfind('}')?;
    serde_json::from_str::<Value>(&cleaned[first..=last]).ok()
}

pub(crate) fn plan_field_string(parsed: &Value, key: &str, fallback: &Value) -> String {
    parsed
        .get(key)
        .and_then(Value::as_str)
        .or_else(|| fallback.get(key).and_then(Value::as_str))
        .unwrap_or("")
        .to_string()
}

pub(crate) fn sanitize_scene_plan(
    parsed: &Value,
    fallback: &Value,
    allowed_character_ids: &[String],
) -> Value {
    let mut plan = ensure_object(fallback.clone()).unwrap_or_default();
    let fallback_name = fallback
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Scene");
    plan.insert(
        "name".to_string(),
        Value::String(safe_title(
            parsed
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(fallback_name),
            "New Scene",
        )),
    );
    for key in [
        "description",
        "scenario",
        "firstMessage",
        "systemPrompt",
        "relationshipHistory",
        "participationGuide",
    ] {
        let value = plan_field_string(parsed, key, fallback);
        if !value.trim().is_empty() {
            plan.insert(key.to_string(), Value::String(value));
        }
    }
    let background = parsed
        .get("background")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty() && *value != "null")
        .map(|value| Value::String(value.to_string()))
        .unwrap_or(Value::Null);
    plan.insert("background".to_string(), background);

    let requested_ids = string_array_from_value(parsed.get("characterIds"));
    let character_ids = if requested_ids.is_empty() {
        string_array_from_value(fallback.get("characterIds"))
    } else if allowed_character_ids.is_empty() {
        requested_ids
    } else {
        requested_ids
            .into_iter()
            .filter(|id| allowed_character_ids.iter().any(|allowed| allowed == id))
            .collect()
    };
    plan.insert("characterIds".to_string(), json!(character_ids));
    plan.insert(
        "rating".to_string(),
        Value::String(
            if parsed.get("rating").and_then(Value::as_str) == Some("nsfw") {
                "nsfw".to_string()
            } else {
                "sfw".to_string()
            },
        ),
    );
    Value::Object(plan)
}

pub(crate) async fn scene_plan(state: &AppState, body: Value) -> AppResult<Value> {
    let chat_id = required_string(&body, "chatId")?;
    let prompt = body
        .get("prompt")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let fallback = fallback_scene_plan(state, chat_id, prompt)?;
    let chat = get_required(state, "chats", chat_id)?;
    let allowed_character_ids = string_array_from_value(chat.get("characterIds"));

    let connection = match resolve_generation_connection(state, chat_id, &body) {
        Ok(connection) => connection,
        Err(error) => {
            return Ok(json!({
                "plan": fallback,
                "error": format!("Used local scene planning because no LLM connection was available: {}", error.message)
            }));
        }
    };

    let history = messages_for_chat(state, chat_id)?
        .into_iter()
        .rev()
        .take(20)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .filter_map(|message| {
            let role = message
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("user");
            let content = message
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            (!content.is_empty()).then(|| format!("{role}: {content}"))
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let request_text = if prompt.is_empty() {
        "Plan a complete roleplay scene that naturally follows the recent conversation.".to_string()
    } else {
        format!("Plan a complete roleplay scene based on this request: {prompt}")
    };
    let messages = vec![
        marinara_llm::LlmMessage {
            role: "system".to_string(),
            content: [
                "You are a scene planner for Marinara roleplay.",
                "Return only one JSON object with fields name, description, scenario, firstMessage, background, characterIds, systemPrompt, rating, relationshipHistory, and participationGuide.",
                "The name must start with Scene:. The rating must be sfw or nsfw. Use only character IDs from the provided list.",
            ]
            .join("\n"),
        },
        marinara_llm::LlmMessage {
            role: "user".to_string(),
            content: format!(
                "Available character IDs: {}\n\nRecent conversation:\n{}\n\n{}",
                allowed_character_ids.join(", "),
                history,
                request_text
            ),
        },
    ];
    let request = marinara_llm::LlmRequest {
        connection: llm_connection_from_value(&connection)?,
        messages,
        parameters: json!({ "temperature": 0.9, "maxTokens": 4096 }),
    };

    match marinara_llm::complete(request).await {
        Ok(raw) => {
            if let Some(parsed) = parse_json_object_from_text(&raw) {
                Ok(
                    json!({ "plan": sanitize_scene_plan(&parsed, &fallback, &allowed_character_ids) }),
                )
            } else {
                Ok(json!({
                    "plan": fallback,
                    "error": "The model did not return valid scene-plan JSON, so Marinara used a local fallback plan."
                }))
            }
        }
        Err(error) => Ok(json!({
            "plan": fallback,
            "error": format!("Scene planning used a local fallback after the LLM request failed: {}", error.message)
        })),
    }
}

pub(crate) fn scene_create(state: &AppState, body: Value) -> AppResult<Value> {
    let origin_chat_id = required_string(&body, "originChatId")?;
    let origin_chat = get_required(state, "chats", origin_chat_id)?;
    let plan = body
        .get("plan")
        .cloned()
        .ok_or_else(|| AppError::invalid_input("plan is required"))?;
    let origin_character_ids = string_array_from_value(origin_chat.get("characterIds"));
    let planned_character_ids = string_array_from_value(plan.get("characterIds"));
    let character_ids = if planned_character_ids.is_empty() {
        origin_character_ids
    } else {
        planned_character_ids
    };
    let scene_name = plan
        .get("name")
        .and_then(Value::as_str)
        .map(|name| safe_title(name, "New Scene"))
        .unwrap_or_else(|| "Scene: New Scene".to_string());
    let description = plan
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("A new scene begins.")
        .to_string();
    let first_message = plan
        .get("firstMessage")
        .and_then(Value::as_str)
        .unwrap_or("The scene begins.")
        .to_string();
    let connection_id = body
        .get("connectionId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            origin_chat
                .get("connectionId")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        });
    let mut metadata = Map::new();
    metadata.insert(
        "sceneOriginChatId".to_string(),
        Value::String(origin_chat_id.to_string()),
    );
    metadata.insert(
        "sceneInitiatorCharId".to_string(),
        body.get("initiatorCharId").cloned().unwrap_or(Value::Null),
    );
    metadata.insert(
        "sceneDescription".to_string(),
        Value::String(description.clone()),
    );
    metadata.insert(
        "sceneScenario".to_string(),
        plan.get("scenario").cloned().unwrap_or(Value::Null),
    );
    metadata.insert(
        "sceneBackground".to_string(),
        plan.get("background").cloned().unwrap_or(Value::Null),
    );
    metadata.insert(
        "sceneSystemPrompt".to_string(),
        plan.get("systemPrompt").cloned().unwrap_or(Value::Null),
    );
    metadata.insert(
        "sceneRelationshipHistory".to_string(),
        plan.get("relationshipHistory")
            .cloned()
            .unwrap_or(Value::Null),
    );
    metadata.insert(
        "sceneRating".to_string(),
        Value::String(
            plan.get("rating")
                .and_then(Value::as_str)
                .unwrap_or("sfw")
                .to_string(),
        ),
    );
    metadata.insert(
        "sceneStatus".to_string(),
        Value::String("active".to_string()),
    );
    metadata.insert("enableMemoryRecall".to_string(), Value::Bool(true));
    if let Some(background) = plan.get("background").and_then(Value::as_str) {
        metadata.insert(
            "background".to_string(),
            Value::String(background.to_string()),
        );
    }

    let scene_chat = state.storage.create(
        "chats",
        json!({
            "name": scene_name,
            "mode": "roleplay",
            "characterIds": character_ids,
            "groupId": origin_chat.get("groupId").cloned().unwrap_or(Value::Null),
            "personaId": origin_chat.get("personaId").cloned().unwrap_or(Value::Null),
            "promptPresetId": origin_chat.get("promptPresetId").cloned().unwrap_or(Value::Null),
            "connectionId": connection_id,
            "connectedChatId": origin_chat_id,
            "metadata": metadata,
        }),
    )?;
    let scene_chat_id = scene_chat
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::new("storage_error", "Created scene chat has no id"))?;

    let mut origin_meta = metadata_map(&origin_chat);
    origin_meta.insert(
        "activeSceneChatId".to_string(),
        Value::String(scene_chat_id.to_string()),
    );
    origin_meta.insert(
        "sceneBusyCharIds".to_string(),
        json!(string_array_from_value(scene_chat.get("characterIds"))),
    );
    patch_metadata_map(state, origin_chat_id, origin_meta)?;
    state.storage.patch(
        "chats",
        origin_chat_id,
        json!({ "connectedChatId": scene_chat_id }),
    )?;

    if let Some(guide) = plan
        .get("participationGuide")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        chat_messages(
            state,
            "POST",
            scene_chat_id,
            json!({ "role": "narrator", "content": guide, "characterId": Value::Null }),
            &HashMap::new(),
        )?;
    }
    let first_character_id = body
        .get("initiatorCharId")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| {
            string_array_from_value(scene_chat.get("characterIds"))
                .into_iter()
                .next()
        });
    let opening_content = [description.as_str(), "", first_message.as_str()].join("\n");
    chat_messages(
        state,
        "POST",
        scene_chat_id,
        json!({
            "role": "assistant",
            "content": opening_content,
            "characterId": first_character_id,
        }),
        &HashMap::new(),
    )?;

    Ok(json!({
        "chatId": scene_chat_id,
        "chatName": scene_chat.get("name").cloned().unwrap_or_else(|| Value::String("Scene".to_string())),
        "description": description,
        "background": plan.get("background").cloned().unwrap_or(Value::Null)
    }))
}

pub(crate) async fn summarize_scene(
    state: &AppState,
    scene_chat_id: &str,
    body: &Value,
) -> AppResult<String> {
    let messages = messages_for_chat(state, scene_chat_id)?;
    let transcript = messages
        .iter()
        .filter_map(|message| {
            let role = message
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("user");
            let content = message
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            (!content.is_empty()).then(|| format!("{role}: {content}"))
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let fallback = if transcript.is_empty() {
        "The scene ended before any substantial roleplay occurred.".to_string()
    } else {
        let clipped: String = transcript.chars().take(1200).collect();
        format!("Scene summary: {clipped}")
    };
    let connection = match resolve_generation_connection(state, scene_chat_id, body) {
        Ok(connection) => connection,
        Err(_) => return Ok(fallback),
    };
    let request = marinara_llm::LlmRequest {
        connection: llm_connection_from_value(&connection)?,
        messages: vec![
            marinara_llm::LlmMessage {
                role: "system".to_string(),
                content: "Summarize the completed roleplay scene in concise third-person prose. Return only the summary.".to_string(),
            },
            marinara_llm::LlmMessage {
                role: "user".to_string(),
                content: transcript,
            },
        ],
        parameters: json!({ "temperature": 0.7, "maxTokens": 800 }),
    };
    match marinara_llm::complete(request).await {
        Ok(summary) if !summary.trim().is_empty() => Ok(summary.trim().to_string()),
        _ => Ok(fallback),
    }
}

pub(crate) async fn scene_conclude(state: &AppState, body: Value) -> AppResult<Value> {
    let scene_chat_id = required_string(&body, "sceneChatId")?;
    let scene_chat = get_required(state, "chats", scene_chat_id)?;
    let mut scene_meta = metadata_map(&scene_chat);
    let origin_chat_id = scene_meta
        .get("sceneOriginChatId")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| AppError::invalid_input("Not a scene chat"))?;
    let summary = summarize_scene(state, scene_chat_id, &body).await?;

    chat_messages(
        state,
        "POST",
        &origin_chat_id,
        json!({ "role": "narrator", "content": format!("The scene concluded.\n\n{summary}") }),
        &HashMap::new(),
    )?;
    scene_meta.insert(
        "sceneStatus".to_string(),
        Value::String("concluded".to_string()),
    );
    patch_metadata_map(state, scene_chat_id, scene_meta)?;
    if let Ok(origin_chat) = get_required(state, "chats", &origin_chat_id) {
        let mut origin_meta = metadata_map(&origin_chat);
        clean_scene_pointers(&mut origin_meta);
        patch_metadata_map(state, &origin_chat_id, origin_meta)?;
        state.storage.patch(
            "chats",
            &origin_chat_id,
            json!({ "connectedChatId": Value::Null }),
        )?;
    }
    state.storage.patch(
        "chats",
        scene_chat_id,
        json!({ "connectedChatId": Value::Null }),
    )?;
    Ok(json!({ "summary": summary, "originChatId": origin_chat_id }))
}

pub(crate) fn scene_abandon(state: &AppState, body: Value) -> AppResult<Value> {
    let scene_chat_id = required_string(&body, "sceneChatId")?;
    let scene_chat = get_required(state, "chats", scene_chat_id)?;
    let scene_meta = metadata_map(&scene_chat);
    let origin_chat_id = scene_meta
        .get("sceneOriginChatId")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| AppError::invalid_input("Not a scene chat"))?;
    if let Ok(origin_chat) = get_required(state, "chats", &origin_chat_id) {
        let mut origin_meta = metadata_map(&origin_chat);
        clean_scene_pointers(&mut origin_meta);
        patch_metadata_map(state, &origin_chat_id, origin_meta)?;
        state.storage.patch(
            "chats",
            &origin_chat_id,
            json!({ "connectedChatId": Value::Null }),
        )?;
    }
    delete_chat_with_messages(state, scene_chat_id)?;
    Ok(json!({ "originChatId": origin_chat_id }))
}

pub(crate) fn fork_metadata(scene_meta: &Map<String, Value>) -> Map<String, Value> {
    const EXCLUDED: &[&str] = &[
        "sceneOriginChatId",
        "sceneInitiatorCharId",
        "sceneDescription",
        "sceneScenario",
        "sceneSystemPrompt",
        "sceneRating",
        "sceneStatus",
        "sceneConversationContext",
        "sceneRelationshipHistory",
        "sceneBackground",
        "activeSceneChatId",
        "sceneBusyCharIds",
    ];
    scene_meta
        .iter()
        .filter(|(key, _)| !EXCLUDED.contains(&key.as_str()) && !key.starts_with("scene"))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

pub(crate) fn scene_fork(state: &AppState, body: Value) -> AppResult<Value> {
    let scene_chat_id = required_string(&body, "sceneChatId")?;
    let mode = body.get("mode").and_then(Value::as_str).unwrap_or("clone");
    if !matches!(mode, "clone" | "convert") {
        return Err(AppError::invalid_input("mode must be clone or convert"));
    }
    let scene_chat = get_required(state, "chats", scene_chat_id)?;
    let scene_meta = metadata_map(&scene_chat);
    let origin_chat_id = scene_meta
        .get("sceneOriginChatId")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let base_name = scene_chat
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Scene");
    let fork_chat = state.storage.create(
        "chats",
        json!({
            "name": format!("{base_name} {}", if mode == "clone" { "Clone" } else { "Converted" }),
            "mode": "roleplay",
            "characterIds": scene_chat.get("characterIds").cloned().unwrap_or_else(|| json!([])),
            "groupId": scene_chat.get("groupId").cloned().unwrap_or(Value::Null),
            "personaId": scene_chat.get("personaId").cloned().unwrap_or(Value::Null),
            "promptPresetId": scene_chat.get("promptPresetId").cloned().unwrap_or(Value::Null),
            "connectionId": scene_chat.get("connectionId").cloned().unwrap_or(Value::Null),
            "metadata": Value::Object(fork_metadata(&scene_meta)),
        }),
    )?;
    let fork_chat_id = fork_chat
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::new("storage_error", "Created fork chat has no id"))?
        .to_string();
    let up_to = body.get("upToMessageId").and_then(Value::as_str);
    let include_guide = body
        .get("includeParticipationGuide")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let mut skipped_guide = false;
    for mut message in messages_for_chat(state, scene_chat_id)? {
        let stop = up_to.is_some_and(|id| message.get("id").and_then(Value::as_str) == Some(id));
        if !include_guide
            && !skipped_guide
            && message.get("role").and_then(Value::as_str) == Some("narrator")
        {
            skipped_guide = true;
            if stop {
                break;
            }
            continue;
        }
        if let Some(object) = message.as_object_mut() {
            object.remove("id");
            object.insert("chatId".to_string(), Value::String(fork_chat_id.clone()));
        }
        state.storage.create("messages", message)?;
        if stop {
            break;
        }
    }
    if mode == "convert" {
        if let Some(origin_id) = &origin_chat_id {
            if let Ok(origin_chat) = get_required(state, "chats", origin_id) {
                let mut origin_meta = metadata_map(&origin_chat);
                clean_scene_pointers(&mut origin_meta);
                patch_metadata_map(state, origin_id, origin_meta)?;
                state.storage.patch(
                    "chats",
                    origin_id,
                    json!({ "connectedChatId": Value::Null }),
                )?;
            }
        }
        delete_chat_with_messages(state, scene_chat_id)?;
    }
    Ok(json!({ "chatId": fork_chat_id, "originChatId": origin_chat_id, "mode": mode }))
}

pub(crate) fn default_scene_analysis() -> Value {
    json!({
        "background": Value::Null,
        "music": Value::Null,
        "ambient": Value::Null,
        "weather": Value::Null,
        "timeOfDay": Value::Null,
        "musicGenre": Value::Null,
        "musicIntensity": Value::Null,
        "locationKind": Value::Null,
        "spotifyTrack": Value::Null,
        "reputationChanges": [],
        "segmentEffects": [],
        "directions": [],
        "illustration": Value::Null,
        "generatedIllustration": Value::Null,
        "generatedNpcAvatars": []
    })
}

pub(crate) fn sanitize_scene_analysis(parsed: &Value) -> Value {
    let mut out = ensure_object(default_scene_analysis()).unwrap_or_default();
    for key in [
        "background",
        "music",
        "ambient",
        "weather",
        "timeOfDay",
        "musicGenre",
        "musicIntensity",
        "locationKind",
        "spotifyTrack",
        "illustration",
    ] {
        if let Some(value) = parsed.get(key) {
            out.insert(key.to_string(), value.clone());
        }
    }
    for key in ["reputationChanges", "segmentEffects", "directions"] {
        if let Some(value) = parsed.get(key).filter(|value| value.is_array()) {
            out.insert(key.to_string(), value.clone());
        }
    }
    Value::Object(out)
}

pub(crate) async fn scene_analyze(state: &AppState, body: Value) -> AppResult<Value> {
    let narration = required_string(&body, "narration")?;
    let chat_id = body.get("chatId").and_then(Value::as_str).unwrap_or("");
    let connection = if chat_id.is_empty() {
        resolve_llm_connection_for_request(state, &body).ok()
    } else {
        resolve_generation_connection(state, chat_id, &body).ok()
    };
    let Some(connection) = connection else {
        return Ok(default_scene_analysis());
    };
    let prompt = format!(
        "Analyze this roleplay scene narration and return only compact JSON with optional keys background, music, ambient, weather, timeOfDay, musicGenre, musicIntensity, locationKind, spotifyTrack, reputationChanges, segmentEffects, directions, illustration. Narration:\n\n{narration}"
    );
    let request = marinara_llm::LlmRequest {
        connection: llm_connection_from_value(&connection)?,
        messages: vec![marinara_llm::LlmMessage {
            role: "user".to_string(),
            content: prompt,
        }],
        parameters: json!({ "maxTokens": 800, "temperature": 0.2 }),
    };
    let raw = match marinara_llm::complete(request).await {
        Ok(raw) => raw,
        Err(_) => return Ok(default_scene_analysis()),
    };
    Ok(parse_json_object_from_text(&raw)
        .map(|parsed| sanitize_scene_analysis(&parsed))
        .unwrap_or_else(default_scene_analysis))
}
