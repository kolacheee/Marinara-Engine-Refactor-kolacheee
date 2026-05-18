use super::shared::*;
use super::*;

pub(crate) fn chat_presets_call(
    state: &AppState,
    method: &str,
    rest: &[&str],
    body: Value,
) -> AppResult<Value> {
    match (method, rest) {
        ("GET", []) => list_collection(state, "chat-presets", None),
        ("POST", []) => state.storage.create("chat-presets", body),
        ("GET", ["active", mode]) => {
            Ok(state
                .storage
                .list("chat-presets")?
                .into_iter()
                .find(|preset| {
                    preset.get("mode").and_then(Value::as_str) == Some(*mode)
                        && preset
                            .get("isActive")
                            .or_else(|| preset.get("active"))
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                })
                .or_else(|| find_by_field(state, "chat-presets", "mode", mode).ok().flatten())
                .unwrap_or(Value::Null))
        }
        ("POST", ["import"]) => state.storage.create("chat-presets", body),
        ("POST", [id, "duplicate"]) => duplicate_record(state, "chat-presets", id),
        ("POST", [id, "set-active"]) => {
            let selected = get_required(state, "chat-presets", id)?;
            let mode = selected.get("mode").and_then(Value::as_str).unwrap_or("");
            for preset in state.storage.list("chat-presets")? {
                let Some(preset_id) = preset.get("id").and_then(Value::as_str) else {
                    continue;
                };
                if preset.get("mode").and_then(Value::as_str).unwrap_or("") == mode {
                    let active = preset_id == *id;
                    state.storage.patch(
                        "chat-presets",
                        preset_id,
                        json!({ "isActive": active, "active": active }),
                    )?;
                }
            }
            get_required(state, "chat-presets", id)
        }
        ("PUT", [id, "settings"]) => {
            state
                .storage
                .patch("chat-presets", id, json!({ "settings": body }))
        }
        ("POST", [id, "apply", chat_id]) => {
            let preset = get_required(state, "chat-presets", id)?;
            let chat = state
                .storage
                .patch("chats", chat_id, json!({ "chatPresetId": id }))?;
            Ok(json!({ "preset": preset, "chat": chat }))
        }
        ("GET", [id, "export"]) => get_required(state, "chat-presets", id),
        ("GET", [id]) => get_required(state, "chat-presets", id),
        ("PATCH", [id]) => state.storage.patch("chat-presets", id, body),
        ("DELETE", [id]) => {
            let deleted = state.storage.delete("chat-presets", id)?;
            Ok(json!({ "deleted": deleted }))
        }
        _ => Err(AppError::new(
            "route_not_found",
            format!("Unknown chat-presets route: {method} /{}", rest.join("/")),
        )),
    }
}
