use super::*;
use std::path::Path;

pub(crate) fn update_character_avatar(
    state: &AppState,
    collection: &str,
    id: &str,
    body: Value,
) -> AppResult<Value> {
    let stored = persist_avatar(state, collection, id, body)?;
    state.storage.patch(
        collection,
        id,
        json!({
            "avatar": stored.data_url,
            "avatarPath": stored.data_url,
            "avatarFilePath": stored.absolute_path,
            "avatarFilename": stored.filename,
            "avatarUpdatedAt": now_iso()
        }),
    )
}

pub(crate) fn update_npc_avatar(state: &AppState, chat_id: &str, body: Value) -> AppResult<Value> {
    let name = body
        .get("name")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("npc")
        .to_string();
    let stored = persist_avatar(state, "npc", &format!("{chat_id}-{name}"), body)?;
    Ok(json!({
        "chatId": chat_id,
        "name": name,
        "avatarPath": stored.data_url,
        "avatarFilePath": stored.absolute_path,
        "avatarFilename": stored.filename
    }))
}

struct StoredAvatar {
    data_url: String,
    absolute_path: String,
    filename: String,
}

fn persist_avatar(
    state: &AppState,
    namespace: &str,
    id: &str,
    body: Value,
) -> AppResult<StoredAvatar> {
    let avatar = body
        .get("avatar")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::invalid_input("avatar is required"))?;
    let (mime, bytes) = decode_avatar_payload(avatar)?;
    let ext = extension_for_mime(&mime)
        .or_else(|| {
            body.get("filename")
                .and_then(Value::as_str)
                .and_then(extension_from_filename)
        })
        .unwrap_or("png");
    let filename = body
        .get("filename")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(safe_filename)
        .unwrap_or_else(|| format!("{}-{}.{}", safe_filename(id), now_millis(), ext));
    let dir = state.data_dir.join("avatars").join(safe_filename(namespace));
    fs::create_dir_all(&dir)?;
    let target = unique_avatar_path(&dir.join(&filename))?;
    fs::write(&target, &bytes)?;
    Ok(StoredAvatar {
        data_url: format!("data:{mime};base64,{}", general_purpose::STANDARD.encode(bytes)),
        absolute_path: target.to_string_lossy().to_string(),
        filename: target
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or(filename),
    })
}

fn decode_avatar_payload(avatar: &str) -> AppResult<(String, Vec<u8>)> {
    if let Some((header, payload)) = avatar.split_once(',') {
        if header.starts_with("data:") {
            let mime = header
                .trim_start_matches("data:")
                .split(';')
                .next()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("image/png")
                .to_string();
            let bytes = general_purpose::STANDARD
                .decode(payload)
                .map_err(|error| AppError::invalid_input(format!("Invalid avatar data: {error}")))?;
            return Ok((mime, bytes));
        }
    }
    let bytes = general_purpose::STANDARD
        .decode(avatar)
        .map_err(|error| AppError::invalid_input(format!("Invalid avatar data: {error}")))?;
    Ok(("image/png".to_string(), bytes))
}

fn extension_for_mime(mime: &str) -> Option<&'static str> {
    match mime.to_ascii_lowercase().as_str() {
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/webp" => Some("webp"),
        "image/gif" => Some("gif"),
        "image/avif" => Some("avif"),
        "image/png" => Some("png"),
        _ => None,
    }
}

fn extension_from_filename(filename: &str) -> Option<&'static str> {
    match Path::new(filename)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => Some("jpg"),
        "webp" => Some("webp"),
        "gif" => Some("gif"),
        "avif" => Some("avif"),
        "png" => Some("png"),
        _ => None,
    }
}

fn safe_filename(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string();
    if sanitized.is_empty() {
        new_id()
    } else {
        sanitized
    }
}

fn unique_avatar_path(target: &Path) -> AppResult<std::path::PathBuf> {
    if !target.exists() {
        return Ok(target.to_path_buf());
    }
    let parent = target.parent().unwrap_or_else(|| Path::new(""));
    let stem = target
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(new_id);
    let ext = target
        .extension()
        .map(|value| format!(".{}", value.to_string_lossy()))
        .unwrap_or_default();
    for index in 1..10_000 {
        let candidate = parent.join(format!("{stem}-{index}{ext}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(AppError::invalid_input("Could not allocate avatar filename"))
}
