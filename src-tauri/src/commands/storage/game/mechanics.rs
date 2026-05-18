use super::*;

const WEATHER_TYPES: &[&str] = &[
    "clear",
    "cloudy",
    "overcast",
    "rain",
    "heavy_rain",
    "storm",
    "snow",
    "fog",
    "wind",
];

pub(super) const ENCOUNTER_TYPES: &[&str] =
    &["combat", "social", "trap", "puzzle", "merchant", "event"];

pub(super) fn rand_range(max: i64) -> i64 {
    if max <= 0 {
        return 0;
    }
    ((now_millis() % max as u128) + 1) as i64
}

pub(super) fn value_i64(value: Option<&Value>, fallback: i64) -> i64 {
    value
        .and_then(Value::as_i64)
        .or_else(|| value.and_then(Value::as_f64).map(|n| n as i64))
        .unwrap_or(fallback)
}

pub(super) fn clamp(value: i64, min: i64, max: i64) -> i64 {
    value.max(min).min(max)
}

pub(super) fn dice_result(notation: &str) -> AppResult<Value> {
    let cleaned = notation.trim();
    let (count_raw, rest) = cleaned
        .split_once('d')
        .or_else(|| cleaned.split_once('D'))
        .ok_or_else(|| {
            AppError::invalid_input("Invalid dice notation. Use NdM, for example 2d6+3.")
        })?;
    let count = if count_raw.is_empty() {
        1
    } else {
        count_raw
            .parse::<i64>()
            .map_err(|_| AppError::invalid_input("Invalid dice count"))?
    };
    let split_at = rest.find(['+', '-']).unwrap_or(rest.len());
    let sides = rest[..split_at]
        .parse::<i64>()
        .map_err(|_| AppError::invalid_input("Invalid dice sides"))?;
    let modifier = if split_at < rest.len() {
        rest[split_at..]
            .parse::<i64>()
            .map_err(|_| AppError::invalid_input("Invalid dice modifier"))?
    } else {
        0
    };
    if count < 1 || sides < 1 {
        return Err(AppError::invalid_input(
            "Dice count and sides must be at least 1.",
        ));
    }
    let capped_count = count.min(100);
    let capped_sides = sides.min(1000);
    let rolls = (0..capped_count)
        .map(|_| rand_range(capped_sides))
        .collect::<Vec<_>>();
    let total: i64 = rolls.iter().sum::<i64>() + modifier;
    Ok(json!({ "notation": cleaned, "rolls": rolls, "modifier": modifier, "total": total }))
}

fn attr_modifier(score: i64) -> i64 {
    ((score - 10) as f64 / 2.0).floor() as i64
}

fn governing_attribute(skill: &str) -> &'static str {
    match skill
        .trim()
        .to_ascii_lowercase()
        .replace([' ', '-'], "_")
        .as_str()
    {
        "str" | "strength" | "athletics" => "str",
        "dex" | "dexterity" | "acrobatics" | "sleight_of_hand" | "stealth" => "dex",
        "con" | "constitution" | "endurance" => "con",
        "wis" | "wisdom" | "animal_handling" | "insight" | "medicine" | "perception"
        | "survival" => "wis",
        "cha" | "charisma" | "deception" | "intimidation" | "performance" | "persuasion" => "cha",
        _ => "int",
    }
}

fn map_attributes(attrs: Option<&Value>) -> Map<String, Value> {
    let mut mapped = Map::new();
    let Some(Value::Array(items)) = attrs else {
        return mapped;
    };
    for item in items {
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_ascii_lowercase();
        let key = match name.as_str() {
            "str" | "strength" => "str",
            "dex" | "dexterity" => "dex",
            "con" | "constitution" => "con",
            "int" | "intelligence" => "int",
            "wis" | "wisdom" => "wis",
            "cha" | "charisma" => "cha",
            _ => continue,
        };
        mapped.insert(key.to_string(), json!(value_i64(item.get("value"), 10)));
    }
    mapped
}

pub(super) fn skill_check(state: &AppState, body: &Value) -> AppResult<Value> {
    let skill = body.get("skill").and_then(Value::as_str).unwrap_or("skill");
    let dc = value_i64(body.get("dc"), 10);
    let chat_id = body.get("chatId").and_then(Value::as_str).unwrap_or("");
    let chat = if chat_id.is_empty() {
        None
    } else {
        state.storage.get("chats", chat_id)?
    };
    let meta = chat.as_ref().map(metadata_map).unwrap_or_default();
    let cards = meta
        .get("gameCharacterCards")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let attrs = cards
        .first()
        .and_then(|card| card.get("rpgStats"))
        .and_then(|rpg| rpg.get("attributes"));
    let mapped = map_attributes(attrs);
    let attr = governing_attribute(skill);
    let attr_score = mapped.get(attr).and_then(Value::as_i64).unwrap_or(10);
    let skill_modifier = value_i64(body.get("skillModifier"), 0);
    let modifier = skill_modifier + attr_modifier(attr_score);
    let pre_roll = value_i64(body.get("preRolledD20"), 0);
    let advantage = body
        .get("advantage")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let disadvantage = body
        .get("disadvantage")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let rolls = if (1..=20).contains(&pre_roll) {
        vec![pre_roll]
    } else if advantage ^ disadvantage {
        vec![rand_range(20), rand_range(20)]
    } else {
        vec![rand_range(20)]
    };
    let used_roll = if advantage && !disadvantage && rolls.len() == 2 {
        rolls[0].max(rolls[1])
    } else if disadvantage && !advantage && rolls.len() == 2 {
        rolls[0].min(rolls[1])
    } else {
        rolls[0]
    };
    let total = used_roll + modifier;
    let critical_success = used_roll == 20;
    let critical_failure = used_roll == 1;
    let success = if critical_success {
        true
    } else if critical_failure {
        false
    } else {
        total >= dc
    };
    Ok(json!({
        "result": {
            "skill": skill,
            "dc": dc,
            "rolls": rolls,
            "usedRoll": used_roll,
            "modifier": modifier,
            "total": total,
            "success": success,
            "criticalSuccess": critical_success,
            "criticalFailure": critical_failure,
            "rollMode": if advantage && !disadvantage { "advantage" } else if disadvantage && !advantage { "disadvantage" } else { "normal" }
        }
    }))
}

pub(super) fn game_time_from_meta(meta: &Map<String, Value>) -> (i64, i64, i64) {
    let time = meta.get("gameTime").and_then(Value::as_object);
    (
        time.and_then(|m| m.get("day"))
            .and_then(Value::as_i64)
            .unwrap_or(1),
        time.and_then(|m| m.get("hour"))
            .and_then(Value::as_i64)
            .unwrap_or(8),
        time.and_then(|m| m.get("minute"))
            .and_then(Value::as_i64)
            .unwrap_or(0),
    )
}

pub(super) fn advance_time_tuple(day: i64, hour: i64, minute: i64, action: &str) -> (i64, i64, i64) {
    let add = match action {
        "explore" => 30,
        "combat_round" => 5,
        "combat_end" => 15,
        "rest_short" => 60,
        "rest_long" => 480,
        "travel" => 120,
        "craft" => 45,
        "shop" => 20,
        "investigate" => 25,
        _ => 15,
    };
    let total = ((day.max(1) - 1) * 1440) + hour * 60 + minute + add;
    let next_day = total / 1440 + 1;
    let rem = total % 1440;
    (next_day, rem / 60, rem % 60)
}

fn time_of_day(hour: i64) -> &'static str {
    match hour {
        5..=6 => "dawn",
        7..=11 => "morning",
        12..=16 => "afternoon",
        17..=19 => "evening",
        20..=23 => "night",
        _ => "midnight",
    }
}

pub(super) fn format_time(day: i64, hour: i64, minute: i64) -> String {
    format!("Day {day}, {hour:02}:{minute:02} ({})", time_of_day(hour))
}

fn infer_biome(location: &str) -> &'static str {
    let lower = location.to_ascii_lowercase();
    if lower.contains("desert") || lower.contains("dune") || lower.contains("wasteland") {
        "desert"
    } else if lower.contains("mountain") || lower.contains("peak") || lower.contains("cliff") {
        "mountain"
    } else if lower.contains("coast") || lower.contains("harbor") || lower.contains("sea") {
        "coastal"
    } else if lower.contains("cave") || lower.contains("dungeon") || lower.contains("crypt") {
        "underground"
    } else if lower.contains("city") || lower.contains("town") || lower.contains("village") {
        "urban"
    } else {
        "temperate"
    }
}

pub(super) fn generate_weather(location: &str, season: &str) -> Value {
    let biome = infer_biome(location);
    let index = (now_millis() as usize + biome.len() + season.len()) % WEATHER_TYPES.len();
    let weather_type = if biome == "underground" {
        "clear"
    } else {
        WEATHER_TYPES[index]
    };
    let base = match biome {
        "desert" => 32,
        "mountain" => 8,
        "coastal" => 20,
        "underground" => 15,
        "urban" => 21,
        _ => 18,
    };
    let season_mod = match season {
        "winter" => -8,
        "autumn" => -3,
        "summer" => 5,
        _ => 0,
    };
    let wind = if matches!(weather_type, "storm" | "heavy_rain" | "wind") {
        "windy"
    } else {
        "calm"
    };
    let visibility = if matches!(weather_type, "fog" | "storm" | "heavy_rain") {
        "poor"
    } else {
        "clear"
    };
    json!({
        "type": weather_type,
        "temperature": base + season_mod + rand_range(7) - 4,
        "description": match weather_type {
            "rain" | "heavy_rain" => "Rain falls across the area.",
            "storm" => "Thunder rolls as the storm builds.",
            "snow" => "Snow drifts through the air.",
            "fog" => "Fog limits visibility.",
            "wind" => "Strong wind moves through the area.",
            _ => "The weather is clear enough to travel."
        },
        "wind": wind,
        "visibility": visibility
    })
}

pub(super) fn journal_from_meta(meta: &Map<String, Value>) -> Value {
    meta.get("gameJournal").cloned().unwrap_or_else(|| {
        json!({ "entries": [], "quests": [], "locations": [], "npcLog": [], "inventoryLog": [] })
    })
}

pub(super) fn append_journal_entry(state: &AppState, body: &Value) -> AppResult<Value> {
    let chat_id = required_string(body, "chatId")?;
    let chat = get_required(state, "chats", chat_id)?;
    let meta = metadata_map(&chat);
    let mut journal = journal_from_meta(&meta);
    let entry_type = body.get("type").and_then(Value::as_str).unwrap_or("event");
    let data = body.get("data").cloned().unwrap_or_else(|| json!({}));
    let title = data
        .get("title")
        .and_then(Value::as_str)
        .or_else(|| data.get("name").and_then(Value::as_str))
        .unwrap_or(match entry_type {
            "location" => "Location",
            "npc" => "NPC",
            "combat" => "Combat",
            "item" => "Item",
            "quest" => "Quest",
            "note" => "Note",
            _ => "Event",
        });
    let content = data
        .get("content")
        .and_then(Value::as_str)
        .or_else(|| data.get("description").and_then(Value::as_str))
        .unwrap_or("");
    if let Some(entries) = journal.get_mut("entries").and_then(Value::as_array_mut) {
        entries.push(json!({
            "timestamp": now_iso(),
            "type": entry_type,
            "title": title,
            "content": content,
            "readableType": data.get("readableType").cloned().unwrap_or(Value::Null),
            "sourceMessageId": data.get("sourceMessageId").cloned().unwrap_or(Value::Null),
            "sourceSegmentIndex": data.get("sourceSegmentIndex").cloned().unwrap_or(Value::Null)
        }));
    }
    if entry_type == "location" {
        if let Some(locations) = journal.get_mut("locations").and_then(Value::as_array_mut) {
            if !locations
                .iter()
                .any(|location| location.as_str() == Some(title))
            {
                locations.push(Value::String(title.to_string()));
            }
        }
    }
    chat_metadata_patch(state, chat_id, json!({ "gameJournal": journal.clone() }))?;
    Ok(json!({ "journal": journal }))
}

fn loot_drop(difficulty: &str) -> Value {
    let table = [
        (
            "Minor Healing Potion",
            "potion",
            "Restores a small amount of health.",
        ),
        (
            "Iron Shortsword",
            "weapon",
            "A reliable shortsword forged from solid iron.",
        ),
        (
            "Chainmail Shirt",
            "armor",
            "Interlocking metal rings provide solid defense.",
        ),
        (
            "Ancient Coin",
            "currency",
            "A weathered coin from a forgotten era.",
        ),
        (
            "Spell Scroll",
            "scroll",
            "A scroll containing a single-use spell.",
        ),
        ("Mysterious Key", "key", "An ornate key of unknown purpose."),
    ];
    let rarity = match difficulty {
        "hard" => ["common", "uncommon", "rare", "rare", "epic"][(now_millis() as usize) % 5],
        "brutal" => ["uncommon", "rare", "rare", "epic", "legendary"][(now_millis() as usize) % 5],
        "casual" => ["common", "common", "uncommon"][(now_millis() as usize) % 3],
        _ => ["common", "uncommon", "rare"][(now_millis() as usize) % 3],
    };
    let item = table[(now_millis() as usize) % table.len()];
    json!({
        "item": { "name": item.0, "description": item.2, "rarity": rarity, "type": item.1, "value": rand_range(20) * 5 },
        "quantity": if item.1 == "currency" { rand_range(10) } else { 1 }
    })
}

pub(super) fn generate_loot(count: i64, difficulty: &str) -> Value {
    Value::Array(
        (0..count.max(0).min(10))
            .map(|_| loot_drop(difficulty))
            .collect(),
    )
}

pub(super) fn combat_round(body: &Value) -> Value {
    let mut combatants = body
        .get("combatants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let round = value_i64(body.get("round"), 1);
    let mut initiative = Vec::new();
    for combatant in &combatants {
        let speed = value_i64(combatant.get("speed"), 10);
        let roll = rand_range(20);
        initiative.push(json!({
            "id": combatant.get("id").cloned().unwrap_or_else(|| json!(new_id())),
            "name": combatant.get("name").and_then(Value::as_str).unwrap_or("Combatant"),
            "roll": roll,
            "speed": speed,
            "total": roll + speed / 5
        }));
    }
    initiative.sort_by(|a, b| {
        b.get("total")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .cmp(&a.get("total").and_then(Value::as_i64).unwrap_or(0))
    });
    let player_action = body.get("playerAction").cloned().unwrap_or(Value::Null);
    let difficulty = body
        .get("difficulty")
        .and_then(Value::as_str)
        .unwrap_or("normal");
    let diff_mult = match difficulty {
        "casual" => 0.6,
        "hard" => 1.3,
        "brutal" => 1.6,
        _ => 1.0,
    };
    let mut actions = Vec::new();
    for entry in &initiative {
        let Some(attacker_id) = entry.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(attacker_index) = combatants
            .iter()
            .position(|c| c.get("id").and_then(Value::as_str) == Some(attacker_id))
        else {
            continue;
        };
        if value_i64(combatants[attacker_index].get("hp"), 0) <= 0 {
            continue;
        }
        let attacker_side = combatants[attacker_index]
            .get("side")
            .and_then(Value::as_str)
            .unwrap_or("enemy");
        let target_id = if attacker_side == "player" {
            player_action
                .get("targetId")
                .and_then(Value::as_str)
                .map(str::to_string)
        } else {
            None
        };
        let target_index = target_id
            .as_deref()
            .and_then(|id| {
                combatants
                    .iter()
                    .position(|c| c.get("id").and_then(Value::as_str) == Some(id))
            })
            .or_else(|| {
                combatants.iter().position(|c| {
                    c.get("side").and_then(Value::as_str).unwrap_or("enemy") != attacker_side
                        && value_i64(c.get("hp"), 0) > 0
                })
            });
        let Some(target_index) = target_index else {
            break;
        };
        if player_action.get("type").and_then(Value::as_str) == Some("defend")
            && attacker_side == "player"
        {
            continue;
        }
        let attack = value_i64(combatants[attacker_index].get("attack"), 10);
        let defense = value_i64(combatants[target_index].get("defense"), 8);
        let attack_roll = rand_range(20) + attack / 3;
        let defense_roll = rand_range(20) + defense / 3;
        let is_miss = attack_roll < defense_roll;
        let is_critical = !is_miss && (attack_roll - defense_roll >= 10 || attack_roll >= 20);
        let raw_damage = if is_miss {
            0
        } else {
            let level = value_i64(combatants[attacker_index].get("level"), 1);
            let mut damage = attack + rand_range(6).max(1) * (level / 2).max(1);
            if is_critical {
                damage = (damage as f64 * 1.5) as i64;
            }
            damage
        };
        let mitigated = (defense as f64 * 0.4) as i64;
        let final_damage = ((raw_damage - mitigated).max(0) as f64 * diff_mult) as i64;
        let remaining_hp = (value_i64(combatants[target_index].get("hp"), 0) - final_damage).max(0);
        if let Some(target) = combatants[target_index].as_object_mut() {
            target.insert("hp".to_string(), json!(remaining_hp));
        }
        actions.push(json!({
            "attackerId": attacker_id,
            "defenderId": combatants[target_index].get("id").cloned().unwrap_or(Value::Null),
            "attackRoll": attack_roll,
            "defenseRoll": defense_roll,
            "rawDamage": raw_damage,
            "mitigated": mitigated.min(raw_damage),
            "finalDamage": final_damage,
            "isCritical": is_critical,
            "isMiss": is_miss,
            "remainingHp": remaining_hp,
            "isKo": remaining_hp <= 0
        }));
    }
    json!({
        "result": { "round": round, "initiative": initiative, "actions": actions, "statusTicks": [], "reactions": [] },
        "combatants": combatants
    })
}

pub(super) fn build_game_card(character_name: &str) -> Value {
    json!({
        "name": character_name,
        "shortDescription": "",
        "class": "Adventurer",
        "abilities": ["Attack", "Assist"],
        "strengths": [],
        "weaknesses": [],
        "extra": {},
        "rpgStats": {
            "attributes": [
                { "name": "STR", "value": 10 },
                { "name": "DEX", "value": 10 },
                { "name": "CON", "value": 10 },
                { "name": "INT", "value": 10 },
                { "name": "WIS", "value": 10 },
                { "name": "CHA", "value": 10 }
            ],
            "hp": { "value": 20, "max": 20 }
        }
    })
}
