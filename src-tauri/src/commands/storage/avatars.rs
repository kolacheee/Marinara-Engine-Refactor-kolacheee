use super::media_uploads::{persist_image_upload, remove_managed_record_file, safe_filename};
use super::*;

pub(crate) fn update_character_avatar(
    state: &AppState,
    collection: &str,
    id: &str,
    body: Value,
) -> AppResult<Value> {
    let previous = shared::get_required(state, collection, id)?;
    let stored = persist_image_upload(
        state,
        &format!("avatars/{}", safe_filename(collection)),
        id,
        &body,
        "avatar",
    )?;
    let updated = state.storage.patch(
        collection,
        id,
        json!({
            "avatar": stored.data_url,
            "avatarPath": stored.data_url,
            "avatarFilePath": stored.absolute_path,
            "avatarFilename": stored.filename,
            "avatarUpdatedAt": now_iso()
        }),
    )?;
    remove_avatar_file(state, collection, &previous);
    Ok(updated)
}

pub(crate) fn remove_avatar_file(
    state: &AppState,
    collection: &str,
    record: &Value,
) {
    remove_managed_record_file(
        state,
        &format!("avatars/{}", safe_filename(collection)),
        record,
        "avatarFilePath",
        "avatarFilename",
    )
}

pub(crate) fn update_npc_avatar(state: &AppState, chat_id: &str, body: Value) -> AppResult<Value> {
    let name = body
        .get("name")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("npc")
        .to_string();
    let stored = persist_image_upload(
        state,
        "avatars/npc",
        &format!("{chat_id}-{name}"),
        &body,
        "avatar",
    )?;
    Ok(json!({
        "chatId": chat_id,
        "name": name,
        "avatarPath": stored.data_url,
        "avatarFilePath": stored.absolute_path,
        "avatarFilename": stored.filename
    }))
}
