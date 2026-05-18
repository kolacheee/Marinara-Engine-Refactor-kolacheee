use super::chats::patch_chat_object_field;
use super::generation::resolve_generation_connection;
use super::images::{generate_image_with_connection, prompt_override};
use super::integrations::{game_spotify_candidates, game_spotify_play};
use super::llm::llm_connection_from_value;
use super::shared::*;
use super::*;

mod mechanics;
mod setup;

use mechanics::{
    advance_time_tuple, append_journal_entry, build_game_card, clamp, combat_round, dice_result,
    format_time, game_time_from_meta, generate_loot, generate_weather, journal_from_meta, rand_range,
    skill_check, value_i64, ENCOUNTER_TYPES,
};
use setup::{game_setup, regenerate_session_lorebook, update_campaign_progression};

pub(crate) fn default_game_map() -> Value {
    json!({
        "id": new_id(),
        "type": "grid",
        "name": "Starting Area",
        "description": "The party's current area.",
        "width": 3,
        "height": 3,
        "cells": [
            { "x": 1, "y": 1, "emoji": "📍", "label": "Start", "discovered": true, "terrain": "safe", "description": "The party's starting point." }
        ],
        "partyPosition": { "x": 1, "y": 1 }
    })
}

pub(crate) fn game_summary(session_number: i64) -> Value {
    json!({
        "sessionNumber": session_number,
        "summary": format!("Session {session_number} concluded."),
        "resumePoint": "Resume from the latest saved game state.",
        "partyDynamics": "Party dynamics were preserved from local journal state.",
        "partyState": "Session state saved locally.",
        "keyDiscoveries": [],
        "characterMoments": [],
        "littleDetails": [],
        "statsSnapshot": {},
        "npcUpdates": [],
        "nextSessionRequest": Value::Null,
        "timestamp": now_iso()
    })
}

pub(crate) fn chat_metadata_patch(
    state: &AppState,
    chat_id: &str,
    patch: Value,
) -> AppResult<Value> {
    patch_chat_object_field(state, chat_id, "metadata", patch)
}

pub(crate) fn chats_for_game(state: &AppState, game_id: &str) -> AppResult<Value> {
    let rows = state
        .storage
        .list("chats")?
        .into_iter()
        .filter(|chat| metadata_map(chat).get("gameId").and_then(Value::as_str) == Some(game_id))
        .collect::<Vec<_>>();
    Ok(Value::Array(rows))
}

fn generated_asset_slug(value: &str) -> String {
    let slug = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        format!("generated-{}", now_millis())
    } else {
        slug.chars().take(80).collect()
    }
}

fn image_review_id(kind: &str, key: &str) -> String {
    format!("{kind}:{}", generated_asset_slug(key))
}

fn image_size(body: &Value, bucket: &str, axis: &str, fallback: u64) -> u64 {
    body.get("imageSizes")
        .and_then(|sizes| sizes.get(bucket))
        .and_then(|size| size.get(axis))
        .and_then(Value::as_u64)
        .filter(|value| (128..=2048).contains(value))
        .unwrap_or(fallback)
}

fn prompt_override_by_id(body: &Value, id: &str) -> Option<String> {
    prompt_override(body, id)
}

fn scene_asset_prompt(kind: &str, label: &str, detail: &str, art_style: &str) -> String {
    let style = if art_style.trim().is_empty() {
        "polished fantasy visual novel art, cinematic lighting, high detail".to_string()
    } else {
        art_style.trim().to_string()
    };
    match kind {
        "background" => format!(
            "Wide establishing background of {label}. {detail}. {style}. No characters, no text, immersive environment art."
        ),
        "illustration" => format!(
            "Cinematic scene illustration: {label}. {detail}. {style}. Dynamic composition, no text, high detail."
        ),
        _ => format!(
            "Portrait of {label}. {detail}. {style}. Centered bust portrait, expressive face, clean readable silhouette, no text."
        ),
    }
}

fn asset_tag_from_path(path: &str) -> String {
    path.rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(path)
        .replace('/', ":")
        .replace('\\', ":")
}

fn image_ext(mime_type: &str) -> &'static str {
    if mime_type.contains("png") {
        "png"
    } else if mime_type.contains("webp") {
        "webp"
    } else {
        "jpg"
    }
}

fn upload_generated_asset(
    state: &AppState,
    category: &str,
    subcategory: &str,
    slug: &str,
    base64: &str,
    mime_type: &str,
) -> AppResult<String> {
    let uploaded = state.game_assets.write_upload(
        category,
        Some(subcategory),
        &json!({
            "name": format!("{slug}.{}", image_ext(mime_type)),
            "type": mime_type,
            "base64": base64
        }),
    )?;
    let path = uploaded
        .get("item")
        .and_then(|item| item.get("path"))
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::new("asset_write_failed", "Generated asset path missing"))?;
    Ok(asset_tag_from_path(path))
}

fn game_asset_preview(state: &AppState, body: &Value) -> AppResult<Value> {
    let chat_id = required_string(body, "chatId")?;
    let chat = get_required(state, "chats", &chat_id)?;
    let meta = metadata_map(&chat);
    let setup = meta
        .get("gameSetupConfig")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let art_style = body
        .get("artStylePrompt")
        .and_then(Value::as_str)
        .or_else(|| setup.get("artStylePrompt").and_then(Value::as_str))
        .unwrap_or("");
    let mut items = Vec::new();
    if let Some(background) = body.get("backgroundTag").and_then(Value::as_str) {
        let slug = generated_asset_slug(background);
        let id = image_review_id("background", &slug);
        items.push(json!({
            "id": id,
            "kind": "background",
            "title": format!("Background: {}", background),
            "prompt": prompt_override_by_id(body, &id).unwrap_or_else(|| scene_asset_prompt("background", background, background, art_style)),
            "width": image_size(body, "background", "width", 1280),
            "height": image_size(body, "background", "height", 720)
        }));
    }
    if let Some(illustration) = body.get("illustration").filter(|value| value.is_object()) {
        let label = illustration
            .get("reason")
            .and_then(Value::as_str)
            .or_else(|| illustration.get("slug").and_then(Value::as_str))
            .or_else(|| illustration.get("prompt").and_then(Value::as_str))
            .unwrap_or("Scene illustration");
        let prompt = illustration
            .get("prompt")
            .and_then(Value::as_str)
            .unwrap_or(label);
        let id = image_review_id("illustration", label);
        items.push(json!({
            "id": id,
            "kind": "illustration",
            "title": format!("Illustration: {}", label),
            "prompt": prompt_override_by_id(body, &id).unwrap_or_else(|| scene_asset_prompt("illustration", label, prompt, art_style)),
            "width": image_size(body, "background", "width", 1280),
            "height": image_size(body, "background", "height", 720)
        }));
    }
    if let Some(npcs) = body.get("npcsNeedingAvatars").and_then(Value::as_array) {
        for npc in npcs.iter().take(10) {
            let name = npc.get("name").and_then(Value::as_str).unwrap_or("NPC");
            let detail = npc
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("distinctive character portrait");
            let id = image_review_id("portrait", name);
            items.push(json!({
                "id": id,
                "kind": "portrait",
                "title": format!("Portrait: {}", name),
                "prompt": prompt_override_by_id(body, &id).unwrap_or_else(|| scene_asset_prompt("portrait", name, detail, art_style)),
                "width": image_size(body, "portrait", "width", 768),
                "height": image_size(body, "portrait", "height", 1024)
            }));
        }
    }
    Ok(json!({ "items": items }))
}

async fn generate_game_assets(state: &AppState, body: Value) -> AppResult<Value> {
    let chat_id = required_string(&body, "chatId")?;
    let chat = get_required(state, "chats", &chat_id)?;
    let meta = metadata_map(&chat);
    if !meta
        .get("enableSpriteGeneration")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(json!({
            "generatedBackground": Value::Null,
            "fallbackBackground": Value::Null,
            "generatedIllustration": Value::Null,
            "generatedNpcAvatars": []
        }));
    }

    let preview = game_asset_preview(state, &body)?;
    let items = preview
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut generated_background = Value::Null;
    let mut generated_illustration = Value::Null;
    let mut generated_npc_avatars = Vec::new();
    let image_connection_id = body
        .get("imageConnectionId")
        .and_then(Value::as_str)
        .or_else(|| meta.get("gameImageConnectionId").and_then(Value::as_str))
        .or_else(|| meta.get("imageConnectionId").and_then(Value::as_str))
        .or_else(|| {
            meta.get("gameSetupConfig")
                .and_then(|value| value.get("imageConnectionId"))
                .and_then(Value::as_str)
        })
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::invalid_input("Game image generation requires an image connection"))?
        .to_string();
    let image_connection = get_required(state, "connections", &image_connection_id)?;

    for item in items {
        let kind = item.get("kind").and_then(Value::as_str).unwrap_or("");
        let prompt = item.get("prompt").and_then(Value::as_str).unwrap_or("");
        let width = item.get("width").and_then(Value::as_u64).unwrap_or(1024);
        let height = item.get("height").and_then(Value::as_u64).unwrap_or(1024);
        let (base64, mime_type) =
            generate_image_with_connection(&image_connection, prompt, width, height).await?;
        match kind {
            "background" => {
                let key = body
                    .get("backgroundTag")
                    .and_then(Value::as_str)
                    .unwrap_or("generated-background");
                let tag = upload_generated_asset(
                    state,
                    "backgrounds",
                    "generated",
                    &generated_asset_slug(key),
                    &base64,
                    &mime_type,
                )?;
                generated_background = Value::String(tag.clone());
                chat_metadata_patch(state, &chat_id, json!({ "gameSceneBackground": tag }))?;
            }
            "illustration" => {
                let key = body
                    .get("illustration")
                    .and_then(|value| value.get("slug"))
                    .and_then(Value::as_str)
                    .or_else(|| item.get("title").and_then(Value::as_str))
                    .unwrap_or("scene-illustration");
                let tag = upload_generated_asset(
                    state,
                    "backgrounds",
                    "illustrations",
                    &generated_asset_slug(key),
                    &base64,
                    &mime_type,
                )?;
                let segment = body
                    .get("illustration")
                    .and_then(|value| value.get("segment"))
                    .cloned();
                generated_illustration = json!({
                    "tag": tag,
                    "segment": segment.unwrap_or(Value::Null)
                });
            }
            "portrait" => {
                let name = item
                    .get("title")
                    .and_then(Value::as_str)
                    .and_then(|title| title.strip_prefix("Portrait: "))
                    .unwrap_or("NPC")
                    .to_string();
                generated_npc_avatars.push(json!({
                    "name": name,
                    "avatarUrl": format!("data:{mime_type};base64,{base64}")
                }));
            }
            _ => {}
        }
    }

    if !generated_npc_avatars.is_empty() {
        let mut npcs = meta
            .get("gameNpcs")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for avatar in &generated_npc_avatars {
            let Some(name) = avatar.get("name").and_then(Value::as_str) else {
                continue;
            };
            let avatar_url = avatar.get("avatarUrl").cloned().unwrap_or(Value::Null);
            if let Some(existing) = npcs.iter_mut().find(|npc| {
                npc.get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|candidate| candidate.eq_ignore_ascii_case(name))
            }) {
                if let Some(object) = existing.as_object_mut() {
                    object.insert("avatarUrl".to_string(), avatar_url);
                }
            } else {
                npcs.push(json!({
                    "id": new_id(),
                    "name": name,
                    "description": "",
                    "location": "",
                    "reputation": 0,
                    "met": true,
                    "notes": [],
                    "avatarUrl": avatar_url
                }));
            }
        }
        chat_metadata_patch(state, &chat_id, json!({ "gameNpcs": npcs }))?;
    }

    Ok(json!({
        "generatedBackground": generated_background,
        "fallbackBackground": Value::Null,
        "generatedIllustration": generated_illustration,
        "generatedNpcAvatars": generated_npc_avatars
    }))
}

async fn party_turn(state: &AppState, body: Value) -> AppResult<Value> {
    let chat_id = required_string(&body, "chatId")?;
    let narration = required_string(&body, "narration")?;
    let chat = get_required(state, "chats", &chat_id)?;
    let meta = metadata_map(&chat);
    let cards = meta
        .get("gameCharacterCards")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let names = cards
        .iter()
        .filter_map(|card| card.get("name").and_then(Value::as_str))
        .filter(|name| !name.trim().is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let party_names = if names.is_empty() {
        "The party".to_string()
    } else {
        names.join(", ")
    };
    let content = match resolve_generation_connection(state, &chat_id, &body) {
        Ok(connection) => {
            let request = marinara_llm::LlmRequest {
                connection: llm_connection_from_value(&connection)?,
                messages: vec![
                    marinara_llm::LlmMessage {
                        role: "system".to_string(),
                        content: format!(
                            "You write short party banter for a game. Reply using lines like [Name] [dialogue] [neutral]: text. Party: {party_names}."
                        ),
                    },
                    marinara_llm::LlmMessage {
                        role: "user".to_string(),
                        content: format!(
                            "GM narration:\n{narration}\n\nPlayer action:\n{}\n\nWrite the party's immediate reactions.",
                            body.get("playerAction").and_then(Value::as_str).unwrap_or("")
                        ),
                    },
                ],
                parameters: json!({ "temperature": 0.9, "maxTokens": 1200 }),
            };
            marinara_llm::complete(request).await?
        }
        Err(_) => format!("[{party_names}] [dialogue] [neutral]: We take this in and prepare for what comes next."),
    };
    let clean = content
        .replace("[party-turn]", "")
        .trim()
        .to_string();
    state.storage.create(
        "messages",
        json!({
            "chatId": chat_id,
            "role": "assistant",
            "characterId": Value::Null,
            "content": format!("[party-turn]\n{clean}"),
            "extra": {},
            "swipes": [{ "content": format!("[party-turn]\n{clean}") }],
            "activeSwipeIndex": 0
        }),
    )?;
    Ok(json!({ "raw": clean }))
}

fn recent_spotify_tracks(body: &Value) -> Vec<String> {
    body.get("context")
        .and_then(|context| context.get("recentSpotifyTracks"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .filter(|uri| uri.starts_with("spotify:track:"))
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn game_spotify_query(body: &Value) -> String {
    let mut text = String::new();
    if let Some(narration) = body.get("narration").and_then(Value::as_str) {
        text.push_str(narration);
        text.push(' ');
    }
    if let Some(action) = body.get("playerAction").and_then(Value::as_str) {
        text.push_str(action);
    }
    let words = text
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|word| word.len() > 3)
        .take(8)
        .collect::<Vec<_>>();
    if words.is_empty() {
        "cinematic adventure soundtrack".to_string()
    } else {
        words.join(" ")
    }
}

async fn game_spotify_candidates_route(state: &AppState, body: Value) -> AppResult<Value> {
    let limit = body
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(50)
        .clamp(1, 50) as u32;
    let recent = recent_spotify_tracks(&body);
    match game_spotify_candidates(state, &game_spotify_query(&body), limit, &recent).await {
        Ok(value) => Ok(value),
        Err(error) if error.code == "not_found" || error.code == "invalid_input" => Ok(json!({
            "enabled": false,
            "tracks": [],
            "error": error.message
        })),
        Err(error) => Err(error),
    }
}

async fn game_spotify_play_route(state: &AppState, body: Value) -> AppResult<Value> {
    let track = body
        .get("track")
        .ok_or_else(|| AppError::invalid_input("track is required"))?;
    let device_id = body.get("deviceId").and_then(Value::as_str);
    game_spotify_play(state, track, device_id).await
}

fn deterministic_scene_analysis(body: &Value) -> Value {
    let context = body.get("context").and_then(Value::as_object);
    let pick_from = |key: &str| -> Value {
        context
            .and_then(|ctx| ctx.get(key))
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .cloned()
            .unwrap_or(Value::Null)
    };
    let spotify_track = context
        .and_then(|ctx| ctx.get("availableSpotifyTracks"))
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .cloned()
        .unwrap_or(Value::Null);
    json!({
        "background": pick_from("availableBackgrounds"),
        "music": Value::Null,
        "ambient": Value::Null,
        "sfx": [],
        "directions": [],
        "weather": context.and_then(|ctx| ctx.get("currentWeather")).cloned().unwrap_or(Value::Null),
        "timeOfDay": context.and_then(|ctx| ctx.get("currentTimeOfDay")).cloned().unwrap_or(Value::Null),
        "reputationChanges": [],
        "segmentEffects": [],
        "spotifyTrack": spotify_track,
        "illustration": Value::Null,
        "generatedIllustration": Value::Null,
        "generatedNpcAvatars": []
    })
}

async fn scene_wrap(state: &AppState, body: Value) -> AppResult<Value> {
    let chat_id = required_string(&body, "chatId")?;
    let narration = required_string(&body, "narration")?;
    let fallback = deterministic_scene_analysis(&body);
    let connection = match resolve_generation_connection(state, &chat_id, &body) {
        Ok(connection) => connection,
        Err(_) => return Ok(fallback),
    };
    let context = body.get("context").cloned().unwrap_or_else(|| json!({}));
    let request = marinara_llm::LlmRequest {
        connection: llm_connection_from_value(&connection)?,
        messages: vec![
            marinara_llm::LlmMessage {
                role: "system".to_string(),
                content: "Analyze the game narration for scene effects. Return strict JSON only with background, music, ambient, sfx, directions, weather, timeOfDay, reputationChanges, segmentEffects, spotifyTrack, illustration, generatedIllustration, generatedNpcAvatars.".to_string(),
            },
            marinara_llm::LlmMessage {
                role: "user".to_string(),
                content: format!("Context JSON:\n{context}\n\nNarration:\n{narration}"),
            },
        ],
        parameters: json!({ "temperature": 0.4, "maxTokens": 2000 }),
    };
    let raw = marinara_llm::complete(request).await?;
    let parsed = super::scene::parse_json_object_from_text(&raw).unwrap_or(fallback);
    Ok(parsed)
}

pub(crate) async fn game_call(
    state: &AppState,
    method: &str,
    rest: &[&str],
    body: Value,
) -> AppResult<Value> {
    match (method, rest) {
        ("POST", ["create"]) => {
            let game_id = new_id();
            let chat = if let Some(chat_id) = body
                .get("chatId")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
            {
                chat_metadata_patch(
                    state,
                    chat_id,
                    json!({ "gameId": game_id, "gameSessionNumber": 1, "gameSessionStatus": "setup", "gameSetupConfig": body.get("setupConfig").cloned().unwrap_or(Value::Null) }),
                )?
            } else {
                state.storage.create(
                    "chats",
                    json!({
                        "name": body.get("name").and_then(Value::as_str).unwrap_or("New Game"),
                        "mode": "game",
                        "characterIds": body.get("partyCharacterIds").cloned().unwrap_or_else(|| json!([])),
                        "connectionId": body.get("connectionId").cloned().unwrap_or(Value::Null),
                        "metadata": {
                            "gameId": game_id,
                            "gameSessionNumber": 1,
                            "gameSessionStatus": "setup",
                            "gameSetupConfig": body.get("setupConfig").cloned().unwrap_or(Value::Null),
                            "gameJournal": { "entries": [], "quests": [], "locations": [], "npcLog": [], "inventoryLog": [] }
                        }
                    }),
                )?
            };
            Ok(json!({ "sessionChat": chat, "gameId": game_id }))
        }
        ("POST", ["setup"]) | ("POST", ["setup", "apply-json"]) => game_setup(state, body).await,
        ("POST", ["start"]) => {
            let chat_id = required_string(&body, "chatId")?;
            chat_metadata_patch(
                state,
                chat_id,
                json!({ "gameSessionStatus": "active", "gameActiveState": "exploration" }),
            )?;
            Ok(json!({ "status": "active", "alreadyStarted": false }))
        }
        ("POST", ["session", "start"]) => {
            let game_id = required_string(&body, "gameId")?;
            let existing = match chats_for_game(state, game_id)? {
                Value::Array(rows) => rows,
                _ => Vec::new(),
            };
            let session_number = existing.len() as i64 + 1;
            let previous_meta = existing.last().map(metadata_map).unwrap_or_default();
            let chat = state.storage.create(
                "chats",
                json!({
                    "name": format!("Game Session {session_number}"),
                    "mode": "game",
                    "characterIds": [],
                    "connectionId": body.get("connectionId").cloned().unwrap_or(Value::Null),
                    "metadata": {
                        "gameId": game_id,
                        "gameSessionNumber": session_number,
                        "gameSessionStatus": "active",
                        "gameActiveState": "exploration",
                        "gamePreviousSessionSummaries": previous_meta.get("gamePreviousSessionSummaries").cloned().unwrap_or_else(|| json!([])),
                        "gameJournal": { "entries": [], "quests": [], "locations": [], "npcLog": [], "inventoryLog": [] }
                    }
                }),
            )?;
            Ok(json!({ "sessionChat": chat, "sessionNumber": session_number, "recap": "" }))
        }
        ("POST", ["session", "conclude"])
        | ("POST", ["session", "regenerate-conclusion"])
        | ("POST", ["session", "conclude", "apply-json"])
        | ("POST", ["session", "regenerate-conclusion", "apply-json"]) => {
            let chat_id = required_string(&body, "chatId")?;
            let chat = get_required(state, "chats", chat_id)?;
            let mut meta = metadata_map(&chat);
            let session_number = meta
                .get("gameSessionNumber")
                .and_then(Value::as_i64)
                .unwrap_or(1);
            let mut summary = body
                .get("summary")
                .cloned()
                .unwrap_or_else(|| game_summary(session_number));
            if summary.get("timestamp").is_none() {
                summary["timestamp"] = json!(now_iso());
            }
            let mut summaries = meta
                .remove("gamePreviousSessionSummaries")
                .and_then(|value| value.as_array().cloned())
                .unwrap_or_default();
            summaries.retain(|item| {
                item.get("sessionNumber").and_then(Value::as_i64) != Some(session_number)
            });
            summaries.push(summary.clone());
            chat_metadata_patch(
                state,
                chat_id,
                json!({ "gameSessionStatus": "concluded", "gamePreviousSessionSummaries": summaries }),
            )?;
            Ok(json!({ "summary": summary }))
        }
        ("POST", ["session", "regenerate-lorebook"]) => {
            regenerate_session_lorebook(state, body).await
        }
        ("POST", ["session", "update-campaign-progression"])
        | ("POST", ["session", "update-campaign-progression", "apply-json"]) => {
            update_campaign_progression(state, body).await
        }
        ("POST", ["party", "recruit"]) | ("POST", ["party", "card", "regenerate"]) => {
            let chat_id = required_string(&body, "chatId")?;
            let name = body
                .get("characterName")
                .and_then(Value::as_str)
                .unwrap_or("Character");
            let card = build_game_card(name);
            let chat = get_required(state, "chats", chat_id)?;
            let mut meta = metadata_map(&chat);
            let mut cards = meta
                .remove("gameCharacterCards")
                .and_then(|value| value.as_array().cloned())
                .unwrap_or_default();
            cards.retain(|item| item.get("name").and_then(Value::as_str) != Some(name));
            cards.push(card.clone());
            let updated =
                chat_metadata_patch(state, chat_id, json!({ "gameCharacterCards": cards }))?;
            Ok(
                json!({ "sessionChat": updated, "added": rest == ["party", "recruit"], "characterName": name, "cardCreated": true, "gameCard": card }),
            )
        }
        ("POST", ["party", "remove"]) => {
            let chat_id = required_string(&body, "chatId")?;
            let name = body
                .get("characterName")
                .and_then(Value::as_str)
                .unwrap_or("Character");
            let chat = get_required(state, "chats", chat_id)?;
            let mut meta = metadata_map(&chat);
            let mut cards = meta
                .remove("gameCharacterCards")
                .and_then(|value| value.as_array().cloned())
                .unwrap_or_default();
            let before = cards.len();
            cards.retain(|item| item.get("name").and_then(Value::as_str) != Some(name));
            let removed = before != cards.len();
            let updated =
                chat_metadata_patch(state, chat_id, json!({ "gameCharacterCards": cards }))?;
            Ok(json!({ "sessionChat": updated, "removed": removed, "characterName": name }))
        }
        ("POST", ["dice", "roll"]) => Ok(
            json!({ "result": dice_result(body.get("notation").and_then(Value::as_str).unwrap_or("1d20"))? }),
        ),
        ("POST", ["skill-check"]) => skill_check(state, &body),
        ("POST", ["morale"]) => Ok(
            json!({ "morale": { "tier": "steady", "value": 0 }, "events": body.get("events").cloned().unwrap_or_else(|| json!([])) }),
        ),
        ("POST", ["state", "transition"]) => {
            let chat_id = required_string(&body, "chatId")?;
            let chat = get_required(state, "chats", chat_id)?;
            let previous = metadata_map(&chat)
                .get("gameActiveState")
                .cloned()
                .unwrap_or_else(|| Value::String("exploration".to_string()));
            let new_state = body
                .get("newState")
                .cloned()
                .unwrap_or_else(|| Value::String("exploration".to_string()));
            chat_metadata_patch(
                state,
                chat_id,
                json!({ "gameActiveState": new_state.clone() }),
            )?;
            Ok(json!({ "previousState": previous, "newState": new_state }))
        }
        ("POST", ["map", "generate"]) => {
            let chat_id = required_string(&body, "chatId")?;
            let mut map = default_game_map();
            if let Some(object) = map.as_object_mut() {
                object.insert(
                    "name".to_string(),
                    Value::String(
                        body.get("locationType")
                            .and_then(Value::as_str)
                            .unwrap_or("Area")
                            .to_string(),
                    ),
                );
                object.insert(
                    "description".to_string(),
                    Value::String(
                        body.get("context")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                    ),
                );
            }
            let active_id = map.get("id").cloned().unwrap_or(Value::Null);
            chat_metadata_patch(
                state,
                chat_id,
                json!({ "gameMap": map.clone(), "gameMaps": [map.clone()], "activeGameMapId": active_id.clone() }),
            )?;
            Ok(json!({ "map": map.clone(), "maps": [map], "activeGameMapId": active_id }))
        }
        ("POST", ["map", "move"]) => {
            let chat_id = required_string(&body, "chatId")?;
            let chat = get_required(state, "chats", chat_id)?;
            let meta = metadata_map(&chat);
            let mut map = meta
                .get("gameMap")
                .cloned()
                .unwrap_or_else(default_game_map);
            let position = body
                .get("position")
                .cloned()
                .unwrap_or_else(|| json!({ "x": 1, "y": 1 }));
            if let Some(object) = map.as_object_mut() {
                object.insert("partyPosition".to_string(), position);
            }
            let active_id = body
                .get("mapId")
                .cloned()
                .or_else(|| map.get("id").cloned())
                .unwrap_or(Value::Null);
            chat_metadata_patch(
                state,
                chat_id,
                json!({ "gameMap": map.clone(), "gameMaps": [map.clone()], "activeGameMapId": active_id.clone() }),
            )?;
            Ok(json!({ "map": map.clone(), "maps": [map], "activeGameMapId": active_id }))
        }
        ("PUT", [chat_id, "notes"]) => {
            chat_metadata_patch(
                state,
                chat_id,
                json!({ "gamePlayerNotes": body.get("notes").cloned().unwrap_or_else(|| json!("")) }),
            )?;
            Ok(json!({ "ok": true }))
        }
        ("PUT", [chat_id, "widgets"]) => {
            chat_metadata_patch(
                state,
                chat_id,
                json!({ "gameWidgetState": body.get("widgets").cloned().unwrap_or_else(|| json!([])) }),
            )?;
            Ok(json!({ "ok": true }))
        }
        ("GET", [game_id, "sessions"]) => chats_for_game(state, game_id),
        ("GET", [chat_id, "journal"]) => {
            let chat = get_required(state, "chats", chat_id)?;
            let meta = metadata_map(&chat);
            Ok(json!({
                "journal": journal_from_meta(&meta),
                "recap": "",
                "playerNotes": meta.get("gamePlayerNotes").cloned().unwrap_or_else(|| json!(""))
            }))
        }
        ("GET", [chat_id, "checkpoints"]) => {
            list_collection(state, "game-checkpoints", Some(("chatId", *chat_id)))
        }
        ("POST", ["checkpoint"]) => {
            let chat_id = required_string(&body, "chatId")?;
            let snapshot = state.storage.create(
                "game-state-snapshots",
                json!({
                    "chatId": chat_id,
                    "messageId": Value::Null,
                    "gameState": get_required(state, "chats", chat_id)?.get("gameState").cloned().unwrap_or_else(|| json!({})),
                    "metadata": metadata_map(&get_required(state, "chats", chat_id)?)
                }),
            )?;
            let snapshot_id = snapshot
                .get("id")
                .cloned()
                .unwrap_or_else(|| json!(new_id()));
            state.storage.create("game-checkpoints", json!({
                "chatId": chat_id,
                "snapshotId": snapshot_id,
                "messageId": "",
                "label": body.get("label").and_then(Value::as_str).unwrap_or("Checkpoint"),
                "triggerType": body.get("triggerType").and_then(Value::as_str).unwrap_or("manual"),
                "location": Value::Null,
                "gameState": Value::Null,
                "weather": Value::Null,
                "timeOfDay": Value::Null,
                "turnNumber": Value::Null
            })).map(|record| json!({ "id": record.get("id").cloned().unwrap_or(Value::Null) }))
        }
        ("POST", ["checkpoint", "load"]) => {
            let chat_id = required_string(&body, "chatId")?;
            let checkpoint_id = required_string(&body, "checkpointId")?;
            let checkpoint = get_required(state, "game-checkpoints", checkpoint_id)?;
            if checkpoint.get("chatId").and_then(Value::as_str) != Some(chat_id) {
                return Err(AppError::invalid_input(
                    "Checkpoint does not belong to this chat",
                ));
            }
            let restore_msg = state.storage.create(
                "messages",
                json!({ "chatId": chat_id, "role": "system", "characterId": Value::Null, "content": format!("[Checkpoint restored: {}]", checkpoint.get("label").and_then(Value::as_str).unwrap_or("Checkpoint")) }),
            )?;
            Ok(
                json!({ "ok": true, "messageId": restore_msg.get("id").cloned().unwrap_or(Value::Null) }),
            )
        }
        ("DELETE", ["checkpoint", id]) => {
            let deleted = state.storage.delete("game-checkpoints", id)?;
            Ok(json!({ "ok": deleted }))
        }
        ("POST", ["combat", "round"]) => Ok(combat_round(&body)),
        ("POST", ["combat", "loot"]) => {
            let count = value_i64(body.get("enemyCount"), 1);
            let difficulty = body
                .get("difficulty")
                .and_then(Value::as_str)
                .unwrap_or("normal");
            Ok(json!({ "drops": generate_loot(count, difficulty) }))
        }
        ("POST", ["loot", "generate"]) => {
            let count = value_i64(body.get("count"), 1);
            let difficulty = body
                .get("difficulty")
                .and_then(Value::as_str)
                .unwrap_or("normal");
            Ok(json!({ "drops": generate_loot(count, difficulty) }))
        }
        ("POST", ["time", "advance"]) => {
            let chat_id = required_string(&body, "chatId")?;
            let action = body
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or("default");
            let chat = get_required(state, "chats", chat_id)?;
            let meta = metadata_map(&chat);
            let (day, hour, minute) = game_time_from_meta(&meta);
            let (day, hour, minute) = advance_time_tuple(day, hour, minute, action);
            let formatted = format_time(day, hour, minute);
            chat_metadata_patch(
                state,
                chat_id,
                json!({ "gameTime": { "day": day, "hour": hour, "minute": minute }, "gameTimeFormatted": formatted }),
            )?;
            Ok(
                json!({ "time": { "day": day, "hour": hour, "minute": minute }, "formatted": formatted }),
            )
        }
        ("POST", ["weather", "update"]) => {
            let chat_id = required_string(&body, "chatId")?;
            let action = body
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or("default");
            let location = body.get("location").and_then(Value::as_str).unwrap_or("");
            let season = body
                .get("season")
                .and_then(Value::as_str)
                .unwrap_or("summer");
            let threshold = match action {
                "travel" => 35,
                "rest_long" => 60,
                "explore" => 20,
                "rest_short" => 15,
                _ => 8,
            };
            let forced_type = body.get("type").and_then(Value::as_str);
            let changed = forced_type.is_some() || rand_range(100) <= threshold;
            let weather = if let Some(kind) = forced_type {
                json!({ "type": kind, "temperature": 20, "description": "", "wind": "calm", "visibility": "clear" })
            } else {
                generate_weather(location, season)
            };
            if changed {
                chat_metadata_patch(state, chat_id, json!({ "gameWeather": weather.clone() }))?;
            }
            Ok(json!({ "changed": changed, "weather": weather }))
        }
        ("POST", ["encounter", "roll"]) => {
            let action = body
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or("default");
            let difficulty = body
                .get("difficulty")
                .and_then(Value::as_str)
                .unwrap_or("normal");
            let location = body.get("location").and_then(Value::as_str).unwrap_or("");
            let base = match action {
                "travel" => 35,
                "map_move" => 30,
                "explore" => 25,
                "rest_long" => 20,
                "rest_short" => 10,
                _ => 15,
            };
            let diff = match difficulty {
                "casual" => -8,
                "hard" => 8,
                "brutal" => 15,
                _ => 0,
            };
            let danger = if location.to_ascii_lowercase().contains("town") {
                -15
            } else {
                0
            };
            let threshold = clamp(base + diff + danger, 5, 90);
            let roll = rand_range(100);
            let triggered = roll <= threshold;
            let encounter_type = if triggered {
                Some(ENCOUNTER_TYPES[(now_millis() as usize) % ENCOUNTER_TYPES.len()])
            } else {
                None
            };
            let hint = match encounter_type {
                Some("combat") => "Hostile creatures emerge from hiding.",
                Some("social") => "A traveler approaches with urgent news.",
                Some("trap") => "The path ahead shows signs of danger.",
                Some("puzzle") => "An unusual mechanism blocks progress.",
                Some("merchant") => "A wandering merchant appears nearby.",
                Some("event") => "Something unexpected changes the scene.",
                _ => "",
            };
            let party_size = value_i64(body.get("partySize"), 1);
            let enemy_count = if encounter_type == Some("combat") {
                (party_size.max(1)
                    + if difficulty == "brutal" {
                        2
                    } else if difficulty == "hard" {
                        1
                    } else {
                        0
                    })
                .max(1)
            } else {
                0
            };
            Ok(
                json!({ "encounter": { "triggered": triggered, "type": encounter_type, "difficulty": difficulty, "hint": hint, "roll": roll, "threshold": threshold }, "enemyCount": enemy_count }),
            )
        }
        ("POST", ["reputation", "update"]) => {
            let chat_id = required_string(&body, "chatId")?;
            let chat = get_required(state, "chats", chat_id)?;
            let mut meta = metadata_map(&chat);
            let mut npcs = meta
                .remove("gameNpcs")
                .and_then(|value| value.as_array().cloned())
                .unwrap_or_default();
            let actions = body
                .get("actions")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let mut changes = Vec::new();
            for action in actions {
                let npc_id = action.get("npcId").and_then(Value::as_str).unwrap_or("");
                let modifier = value_i64(action.get("modifier"), 0);
                if let Some(npc) = npcs
                    .iter_mut()
                    .find(|npc| npc.get("id").and_then(Value::as_str) == Some(npc_id))
                {
                    let current = value_i64(npc.get("reputation"), 0);
                    let next = clamp(current + modifier, -100, 100);
                    npc["reputation"] = json!(next);
                    changes.push(json!({ "npcId": npc_id, "from": current, "to": next, "action": action.get("action").cloned().unwrap_or(Value::Null) }));
                }
            }
            chat_metadata_patch(state, chat_id, json!({ "gameNpcs": npcs.clone() }))?;
            Ok(json!({ "npcs": npcs, "changes": changes }))
        }
        ("POST", ["journal", "entry"]) => append_journal_entry(state, &body),
        ("POST", ["generate-assets", "preview"]) => game_asset_preview(state, &body),
        ("POST", ["generate-assets"]) => generate_game_assets(state, body).await,
        ("POST", ["spotify", "candidates"]) => game_spotify_candidates_route(state, body).await,
        ("POST", ["spotify", "play"]) => game_spotify_play_route(state, body).await,
        ("POST", ["party-turn"]) => party_turn(state, body).await,
        ("POST", ["scene-wrap"]) => scene_wrap(state, body).await,
        _ => Err(AppError::new(
            "route_not_found",
            format!("Unknown game route: {method} /game/{}", rest.join("/")),
        )),
    }
}

