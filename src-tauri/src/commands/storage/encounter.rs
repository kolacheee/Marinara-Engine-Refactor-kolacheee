use super::shared::*;
use super::*;

fn combatant_name(value: &Value, fallback: &str) -> String {
    value
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn fallback_party_member(name: &str, is_player: bool) -> Value {
    json!({
        "name": name,
        "hp": 24,
        "maxHp": 24,
        "attacks": [{ "name": "Attack", "type": "single-target", "description": "A basic attack.", "power": 1.0, "cooldown": 0 }],
        "items": ["Healing Potion x1"],
        "statuses": [],
        "isPlayer": is_player
    })
}

fn fallback_enemy(index: usize) -> Value {
    json!({
        "name": format!("Enemy {}", index + 1),
        "hp": 18,
        "maxHp": 18,
        "attacks": [{ "name": "Strike", "type": "single-target", "description": "A direct attack.", "power": 1.0, "cooldown": 0 }],
        "statuses": [],
        "description": "A hostile combatant.",
        "sprite": "enemy"
    })
}

fn init_state(state: &AppState, body: &Value) -> AppResult<Value> {
    let chat_id = required_string(body, "chatId")?;
    let chat = get_required(state, "chats", chat_id)?;
    let meta = metadata_map(&chat);
    let mut party = Vec::new();
    let cards = meta
        .get("gameCharacterCards")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(first) = cards.first() {
        let name = combatant_name(first, "Player");
        let hp = first
            .get("rpgStats")
            .and_then(|rpg| rpg.get("hp"))
            .and_then(|hp| hp.get("max"))
            .and_then(Value::as_i64)
            .unwrap_or(24);
        party.push(json!({
            "name": name,
            "hp": hp,
            "maxHp": hp,
            "attacks": [{ "name": "Attack", "type": "single-target", "description": "A basic attack.", "power": 1.0, "cooldown": 0 }],
            "items": ["Healing Potion x1"],
            "statuses": [],
            "isPlayer": true
        }));
    }
    for card in cards.iter().skip(1) {
        party.push(fallback_party_member(&combatant_name(card, "Ally"), false));
    }
    if party.is_empty() {
        party.push(fallback_party_member("Player", true));
    }
    let enemy_count = body
        .get("enemyCount")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .max(1)
        .min(6) as usize;
    let enemies = (0..enemy_count).map(fallback_enemy).collect::<Vec<_>>();
    let map_name = meta
        .get("gameMap")
        .and_then(|map| map.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("the current area");
    Ok(json!({
        "combatState": {
            "party": party,
            "enemies": enemies,
            "environment": map_name,
            "styleNotes": {
                "environmentType": "plains",
                "atmosphere": "tense",
                "timeOfDay": "day",
                "weather": "clear"
            },
            "itemEffects": [{
                "name": "Healing Potion",
                "target": "ally",
                "type": "heal",
                "description": "Restores a moderate amount of health.",
                "power": 0.3,
                "consumes": true
            }],
            "dialogueCues": [],
            "mechanics": [],
            "visuals": { "isBossFight": false, "enemyImagePrompts": [] }
        }
    }))
}

fn hp(value: &Value) -> i64 {
    value.get("hp").and_then(Value::as_i64).unwrap_or(0)
}

fn resolve_action(body: &Value) -> AppResult<Value> {
    let action = body
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("Attack");
    let combat = body
        .get("combatStats")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let mut party = combat
        .get("party")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut enemies = combat
        .get("enemies")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if party.is_empty() || enemies.is_empty() {
        return Err(AppError::invalid_input(
            "combatStats.party and combatStats.enemies are required",
        ));
    }

    let target_index = enemies.iter().position(|enemy| hp(enemy) > 0).unwrap_or(0);
    let player_name = combatant_name(&party[0], "Player");
    let enemy_name = combatant_name(&enemies[target_index], "Enemy");
    let damage = if action.to_ascii_lowercase().contains("defend") {
        0
    } else {
        5 + (now_millis() % 8) as i64
    };
    let enemy_remaining = (hp(&enemies[target_index]) - damage).max(0);
    if let Some(enemy) = enemies[target_index].as_object_mut() {
        enemy.insert("hp".to_string(), json!(enemy_remaining));
    }

    let mut enemy_actions = Vec::new();
    let mut party_damage = 0;
    if enemy_remaining > 0 {
        party_damage = 3 + (now_millis() % 6) as i64;
        if action.to_ascii_lowercase().contains("defend") {
            party_damage = (party_damage as f64 * 0.45) as i64;
        }
        let player_remaining = (hp(&party[0]) - party_damage).max(0);
        if let Some(player) = party[0].as_object_mut() {
            player.insert("hp".to_string(), json!(player_remaining));
        }
        enemy_actions.push(
            json!({ "enemyName": enemy_name, "action": "counterattacks", "target": player_name }),
        );
    }

    let victory = enemies.iter().all(|enemy| hp(enemy) <= 0);
    let defeat = party.iter().all(|member| hp(member) <= 0);
    let narrative = if victory {
        format!("{player_name}'s action ends the fight.")
    } else if defeat {
        format!("{player_name} falls as the encounter turns against the party.")
    } else if damage > 0 {
        format!("{player_name} uses {action}, dealing {damage} damage. The enemy answers for {party_damage} damage.")
    } else {
        format!("{player_name} takes a defensive stance and reduces the incoming blow.")
    };
    let result_value = if victory {
        Value::String("victory".to_string())
    } else if defeat {
        Value::String("defeat".to_string())
    } else {
        Value::Null
    };
    Ok(json!({
        "result": {
            "combatStats": { "party": party, "enemies": enemies },
            "playerActions": body.get("playerActions").cloned().unwrap_or_else(|| json!({ "attacks": [], "items": [] })),
            "enemyActions": enemy_actions,
            "partyActions": [],
            "narrative": narrative,
            "combatEnd": victory || defeat,
            "result": result_value
        }
    }))
}

fn summary(state: &AppState, body: &Value) -> AppResult<Value> {
    let chat_id = required_string(body, "chatId")?;
    let result = body
        .get("result")
        .and_then(Value::as_str)
        .unwrap_or("interrupted");
    let entries = body
        .get("encounterLog")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut lines = vec![format!("Combat concluded: {result}.")];
    for entry in entries.iter().take(8) {
        if let Some(action) = entry.get("action").and_then(Value::as_str) {
            lines.push(format!("- {action}"));
        }
        if let Some(outcome) = entry.get("result").and_then(Value::as_str) {
            lines.push(format!("  {outcome}"));
        }
    }
    let text = lines.join("\n");
    let message = state.storage.create(
        "messages",
        json!({
            "chatId": chat_id,
            "role": "assistant",
            "characterId": Value::Null,
            "content": text,
            "extra": {},
            "swipes": [{ "content": text }],
            "activeSwipeIndex": 0
        }),
    )?;
    Ok(json!({ "summary": text, "messageId": message.get("id").cloned().unwrap_or(Value::Null) }))
}

pub(crate) fn encounter_call(state: &AppState, rest: &[&str], body: Value) -> AppResult<Value> {
    match rest {
        ["init"] => init_state(state, &body),
        ["action"] => resolve_action(&body),
        ["summary"] => summary(state, &body),
        _ => Err(AppError::new(
            "route_not_found",
            format!("Unknown encounter route: /{}", rest.join("/")),
        )),
    }
}
