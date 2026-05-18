use super::*;
use super::super::chats::messages_for_chat;
use super::super::generation::resolve_generation_connection;
use super::super::llm::llm_connection_from_value;
use super::super::scene::parse_json_object_from_text;

fn fallback_game_blueprint(preferences: &str) -> Value {
    let overview = if preferences.trim().is_empty() {
        "A flexible local campaign ready for play.".to_string()
    } else {
        format!("A local campaign shaped around: {}", preferences.trim())
    };
    json!({
        "worldOverview": overview,
        "hudWidgets": [
            { "id": "party", "type": "party", "title": "Party", "enabled": true },
            { "id": "journal", "type": "journal", "title": "Journal", "enabled": true },
            { "id": "inventory", "type": "inventory", "title": "Inventory", "enabled": true }
        ],
        "introSequence": [
            "Frame the opening situation clearly.",
            "Invite the player to choose the first action."
        ],
        "visualTheme": {
            "palette": "default",
            "uiStyle": "classic",
            "moodDefault": "neutral"
        },
        "campaignPlan": {
            "questSeeds": [],
            "encounterPrinciples": [
                "Keep conflicts actionable.",
                "Let player choices alter the world state."
            ]
        }
    })
}

pub(super) async fn game_setup(state: &AppState, body: Value) -> AppResult<Value> {
    let chat_id = required_string(&body, "chatId")?.to_string();
    let preferences = body
        .get("preferences")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let config = body
        .get("setupConfig")
        .or_else(|| body.get("preferences"))
        .cloned()
        .unwrap_or(Value::Null);
    let enable_sprite_generation = body
        .get("enableSpriteGeneration")
        .or_else(|| config.get("enableSpriteGeneration"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let image_connection_id = body
        .get("imageConnectionId")
        .or_else(|| config.get("imageConnectionId"))
        .cloned()
        .unwrap_or(Value::Null);
    let mut blueprint = body
        .get("setup")
        .cloned()
        .unwrap_or_else(|| fallback_game_blueprint(&preferences));

    if body.get("setup").is_none() {
        if let Ok(connection) = resolve_generation_connection(state, &chat_id, &body) {
            let request = marinara_llm::LlmRequest {
                connection: llm_connection_from_value(&connection)?,
                messages: vec![
                    marinara_llm::LlmMessage {
                        role: "system".to_string(),
                        content: "Create a game-mode setup blueprint for a roleplay campaign. Return strict JSON only with worldOverview, hudWidgets, introSequence, visualTheme, and campaignPlan.".to_string(),
                    },
                    marinara_llm::LlmMessage {
                        role: "user".to_string(),
                        content: format!("Player preferences:\n{preferences}"),
                    },
                ],
                parameters: json!({ "temperature": 0.7, "maxTokens": 2200 }),
            };
            if let Ok(raw) = marinara_llm::complete(request).await {
                if let Some(parsed) = parse_json_object_from_text(&raw) {
                    blueprint = parsed;
                }
            }
        }
    }

    let world_overview = blueprint
        .get("worldOverview")
        .or_else(|| blueprint.get("overview"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            fallback_game_blueprint(&preferences)
                .get("worldOverview")
                .and_then(Value::as_str)
                .unwrap_or("Game setup saved.")
                .to_string()
        });
    chat_metadata_patch(
        state,
        &chat_id,
        json!({
            "gameSetupConfig": config,
            "gameSessionStatus": "ready",
            "gameBlueprint": blueprint,
            "gameMap": default_game_map(),
            "gameMaps": [default_game_map()],
            "enableSpriteGeneration": enable_sprite_generation,
            "gameImageConnectionId": image_connection_id,
            "gameTime": { "day": 1, "hour": 8, "minute": 0 },
            "gameJournal": { "entries": [], "quests": [], "locations": [], "npcLog": [], "inventoryLog": [] }
        }),
    )?;
    Ok(json!({ "setup": blueprint, "worldOverview": world_overview }))
}

pub(super) async fn regenerate_session_lorebook(
    state: &AppState,
    body: Value,
) -> AppResult<Value> {
    let chat_id = required_string(&body, "chatId")?.to_string();
    let session_number = body
        .get("sessionNumber")
        .and_then(Value::as_i64)
        .unwrap_or(1);
    let messages = messages_for_chat(state, &chat_id)?;
    let transcript = messages
        .iter()
        .rev()
        .take(80)
        .rev()
        .map(|message| {
            format!(
                "{}: {}",
                message.get("role").and_then(Value::as_str).unwrap_or("message"),
                message.get("content").and_then(Value::as_str).unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let entries = match resolve_generation_connection(state, &chat_id, &body) {
        Ok(connection) if !transcript.trim().is_empty() => {
            let request = marinara_llm::LlmRequest {
                connection: llm_connection_from_value(&connection)?,
                messages: vec![
                    marinara_llm::LlmMessage {
                        role: "system".to_string(),
                        content: "Extract durable campaign lore from the session transcript. Return strict JSON with an entries array; each entry has name, content, and keys array.".to_string(),
                    },
                    marinara_llm::LlmMessage {
                        role: "user".to_string(),
                        content: transcript.clone(),
                    },
                ],
                parameters: json!({ "temperature": 0.3, "maxTokens": 2500 }),
            };
            match marinara_llm::complete(request)
                .await
                .ok()
                .and_then(|raw| parse_json_object_from_text(&raw))
                .and_then(|parsed| parsed.get("entries").and_then(Value::as_array).cloned())
            {
                Some(entries) if !entries.is_empty() => entries,
                _ => fallback_session_lore_entries(session_number, &transcript),
            }
        }
        _ => fallback_session_lore_entries(session_number, &transcript),
    };
    let lorebook = state.storage.create(
        "lorebooks",
        json!({
            "name": format!("Game Session {session_number} Lore"),
            "description": "Generated from local game session state.",
            "category": "game",
            "chatId": chat_id,
            "enabled": true,
            "generatedBy": "game-session"
        }),
    )?;
    let lorebook_id = lorebook
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::new("storage_error", "Created lorebook is missing an id"))?
        .to_string();
    let mut entry_count = 0usize;
    for (index, entry) in entries.iter().enumerate() {
        let keys = entry
            .get("keys")
            .cloned()
            .unwrap_or_else(|| json!([format!("session {session_number}")]));
        state.storage.create(
            "lorebook-entries",
            json!({
                "lorebookId": lorebook_id,
                "name": entry.get("name").and_then(Value::as_str).unwrap_or("Session Lore"),
                "content": entry.get("content").and_then(Value::as_str).unwrap_or(""),
                "keys": keys,
                "secondaryKeys": [],
                "enabled": true,
                "constant": false,
                "selective": false,
                "order": index as i64,
                "sortOrder": index as i64,
                "position": 0,
                "role": "system",
                "excludeFromVectorization": false
            }),
        )?;
        entry_count += 1;
    }
    chat_metadata_patch(
        state,
        &chat_id,
        json!({ "gameSessionLorebookId": lorebook_id, "gameSessionLorebookEntryCount": entry_count }),
    )?;
    Ok(json!({
        "sessionNumber": session_number,
        "lorebookId": lorebook_id,
        "entryCount": entry_count
    }))
}

fn fallback_session_lore_entries(session_number: i64, transcript: &str) -> Vec<Value> {
    let summary = transcript
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(12)
        .collect::<Vec<_>>()
        .join("\n");
    if summary.trim().is_empty() {
        return vec![json!({
            "name": format!("Session {session_number} State"),
            "content": "No transcript was available; preserve the current campaign state from the chat metadata.",
            "keys": [format!("session {session_number}")]
        })];
    }
    vec![json!({
        "name": format!("Session {session_number} Recap"),
        "content": summary,
        "keys": [format!("session {session_number}"), "recap", "campaign"]
    })]
}

pub(super) async fn update_campaign_progression(
    state: &AppState,
    body: Value,
) -> AppResult<Value> {
    let chat_id = required_string(&body, "chatId")?.to_string();
    let session_number = body
        .get("sessionNumber")
        .and_then(Value::as_i64)
        .unwrap_or(1);
    let chat = get_required(state, "chats", &chat_id)?;
    let meta = metadata_map(&chat);
    let game_id = meta.get("gameId").cloned().unwrap_or(Value::Null);
    let transcript = messages_for_chat(state, &chat_id)?
        .iter()
        .rev()
        .take(80)
        .rev()
        .map(|message| message.get("content").and_then(Value::as_str).unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    let fallback = json!({
        "storyArc": if transcript.trim().is_empty() {
            Value::Null
        } else {
            Value::String(format!("Session {session_number} advanced the campaign."))
        },
        "plotTwists": [],
        "partyArcs": []
    });
    let progression = match resolve_generation_connection(state, &chat_id, &body) {
        Ok(connection) if !transcript.trim().is_empty() => {
            let request = marinara_llm::LlmRequest {
                connection: llm_connection_from_value(&connection)?,
                messages: vec![
                    marinara_llm::LlmMessage {
                        role: "system".to_string(),
                        content: "Update campaign progression from this game session. Return strict JSON with storyArc, plotTwists, and partyArcs.".to_string(),
                    },
                    marinara_llm::LlmMessage {
                        role: "user".to_string(),
                        content: transcript,
                    },
                ],
                parameters: json!({ "temperature": 0.4, "maxTokens": 1800 }),
            };
            marinara_llm::complete(request)
                .await
                .ok()
                .and_then(|raw| parse_json_object_from_text(&raw))
                .unwrap_or_else(|| fallback.clone())
        }
        _ => fallback,
    };
    let updated = chat_metadata_patch(
        state,
        &chat_id,
        json!({ "gameCampaignProgression": progression, "gameCampaignProgressionUpdatedAt": now_iso() }),
    )?;
    Ok(json!({
        "sessionChat": updated,
        "gameId": game_id,
        "campaignProgression": progression
    }))
}
