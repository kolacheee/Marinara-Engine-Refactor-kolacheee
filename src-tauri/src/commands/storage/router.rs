use super::agents::*;
use super::admin::*;
use super::avatars::*;
use super::backgrounds::*;
use super::backup::*;
use super::bot_browser::*;
use super::chat_presets::*;
use super::characters::*;
use super::chats::*;
use super::custom_tools::*;
use super::exports::*;
use super::fonts::*;
use super::game_assets::*;
use super::generation::*;
use super::http::*;
use super::images::*;
use super::imports::*;
use super::integrations::*;
use super::knowledge::*;
use super::llm::*;
use super::lorebook_images::*;
use super::prompts::*;
use super::shared::*;
use super::sprites::*;
use super::translation::*;
use super::*;

pub(crate) async fn stream_events(
    state: &AppState,
    path: String,
    body: Option<Value>,
) -> Result<Vec<Value>, AppError> {
    let route = ParsedPath::new(&path);
    let parts: Vec<&str> = route.parts.iter().map(String::as_str).collect();
    match parts.as_slice() {
        ["generate"] => generate_events(state, body.unwrap_or(Value::Null)).await,
        ["llm", "stream"] => llm_stream_events(state, body.unwrap_or(Value::Null)).await,
        ["import", rest @ ..] => import_stream_events(state, rest, body.unwrap_or(Value::Null)),
        _ => Err(AppError::new(
            "stream_not_supported",
            format!("Streaming is not supported for {path}"),
        )),
    }
}

pub(crate) async fn route_request(
    state: &AppState,
    method: &str,
    path: &str,
    body: Value,
) -> AppResult<Value> {
    let route = ParsedPath::new(path);
    let parts: Vec<&str> = route.parts.iter().map(String::as_str).collect();
    match parts.as_slice() {
        [] => Ok(json!({ "ok": true })),
        ["health"] => Ok(
            json!({ "ok": true, "runtime": "tauri", "dataDir": state.data_dir.to_string_lossy() }),
        ),
        ["backup", rest @ ..] => backup_call(state, method, rest, &route, body),
        ["updates", "check"] if method == "GET" => marinara_updates::check_updates(),
        ["updates", "apply"] if method == "POST" => Ok(json!({
            "applied": false,
            "status": "apply_unavailable",
            "message": "This desktop build can check update metadata. Applying updates is handled by the packaged desktop updater or release installer.",
            "applyUnavailableReason": "unsupported-install"
        })),
        ["llm", "complete"] if method == "POST" => llm_complete(state, body).await,
        ["llm", "models"] if method == "GET" => {
            llm_models(state, route.query.get("connectionId").map(String::as_str)).await
        }
        ["fonts", rest @ ..] => fonts_call(state, method, rest, body).await,
        ["tts", rest @ ..] => tts_call(state, method, rest, body).await,
        ["translate"] if method == "POST" => translate_text(state, body).await,
        ["backgrounds", rest @ ..] => backgrounds_call(state, method, rest, body),
        ["avatars", "npc", chat_id] if method == "POST" => update_npc_avatar(state, chat_id, body),
        ["generate", "abort"] if method == "POST" => abort_generation(state, body),
        ["gifs", "search"] if method == "GET" => gifs_search(&route).await,
        ["knowledge-sources", rest @ ..] => knowledge_sources_call(state, method, rest, body),
        ["bot-browser", rest @ ..] => bot_browser_call(state, method, rest, &route, body).await,
        ["import", rest @ ..] => import_call(state, rest, body),
        ["admin", "clear-all"] if method == "POST" => admin_clear_all(state),
        ["admin", "expunge"] if method == "POST" => admin_expunge(state, body),
        ["app-settings", key] => handle_singleton(state, method, "app-settings", key, body),
        ["characters", "avatar-generation", "preview"] if method == "POST" => {
            avatar_generation_preview(state, body)
        }
        ["characters", "avatar-generation"] if method == "POST" => {
            avatar_generation(state, body).await
        }
        ["images", "generate"] if method == "POST" => generate_image(state, body).await,
        ["characters", "personas", "list"] if method == "GET" => {
            list_collection(state, "personas", None)
        }
        ["characters", "personas", "export-bulk"] if method == "POST" => {
            export_records(state, "marinara_personas", "personas", body)
        }
        ["characters", "personas"] => collection_root(state, method, "personas", body),
        ["characters", "personas", id, "export"] if method == "GET" => export_record(
            state,
            "marinara_persona",
            "personas",
            id,
            route.query.get("format").map(String::as_str),
        ),
        ["characters", "personas", id] => {
            collection_item_or_action(state, method, "personas", id, None, body)
        }
        ["characters", "personas", id, "duplicate"] if method == "POST" => {
            duplicate_record(state, "personas", id)
        }
        ["characters", "personas", id, "activate"] if method == "PUT" => {
            activate_persona(state, id)
        }
        ["characters", "personas", id, "avatar"] if method == "POST" => {
            update_character_avatar(state, "personas", id, body)
        }
        ["characters", "groups", "list"] if method == "GET" => {
            list_collection(state, "character-groups", None)
        }
        ["characters", "groups"] => collection_root(state, method, "character-groups", body),
        ["characters", "groups", id] => {
            collection_item_or_action(state, method, "character-groups", id, None, body)
        }
        ["characters", "persona-groups", "list"] if method == "GET" => {
            list_collection(state, "persona-groups", None)
        }
        ["characters", "persona-groups"] => collection_root(state, method, "persona-groups", body),
        ["characters", "persona-groups", id] => {
            collection_item_or_action(state, method, "persona-groups", id, None, body)
        }
        ["characters", id, "duplicate"] if method == "POST" => {
            duplicate_record(state, "characters", id)
        }
        ["characters", id, "export"] if method == "GET" => export_record(
            state,
            "marinara_character",
            "characters",
            id,
            route.query.get("format").map(String::as_str),
        ),
        ["characters", id, "export-png"] if method == "GET" => export_character_png(state, id),
        ["characters", id, "embedded-lorebook", "import"] if method == "POST" => {
            import_character_embedded_lorebook(state, id)
        }
        ["characters", id, "avatar"] if method == "POST" => {
            update_character_avatar(state, "characters", id, body)
        }
        ["characters", id, "versions"] if method == "GET" => {
            list_collection(state, "character-versions", Some(("characterId", *id)))
        }
        ["characters", id, "versions", version_id] if method == "DELETE" => {
            let deleted = state.storage.delete("character-versions", version_id)?;
            Ok(json!({ "deleted": deleted, "characterId": id }))
        }
        ["characters", id, "versions", version_id, "restore"] if method == "POST" => {
            restore_character_version(state, id, version_id)
        }
        ["characters", id, "gallery"] if method == "GET" => {
            list_collection(state, "character-gallery", Some(("characterId", *id)))
        }
        ["characters", id, "gallery", "upload"] if method == "POST" => {
            upload_gallery_image(state, "character-gallery", "characterId", id, body)
        }
        ["characters", id, "gallery", image_id] if method == "DELETE" => {
            let deleted = state.storage.delete("character-gallery", image_id)?;
            Ok(json!({ "deleted": deleted, "characterId": id }))
        }
        ["characters", "export-bulk"] if method == "POST" => {
            export_records(state, "marinara_characters", "characters", body)
        }
        ["characters"] => collection_root(state, method, "characters", body),
        ["characters", id] => {
            collection_item_or_action(state, method, "characters", id, None, body)
        }
        ["chats"] => collection_root(state, method, "chats", with_chat_defaults(body)),
        ["chats", "group", group_id] if method == "GET" => {
            list_collection(state, "chats", Some(("groupId", *group_id)))
        }
        ["chats", "group", group_id] if method == "DELETE" => delete_chat_group(state, group_id),
        ["chats", chat_id, "messages"] => chat_messages(state, method, chat_id, body, &route.query),
        ["chats", chat_id, "gallery"] if method == "GET" => {
            list_collection(state, "gallery", Some(("chatId", *chat_id)))
        }
        ["chats", chat_id, "gallery", "upload"] if method == "POST" => {
            upload_gallery_image(state, "gallery", "chatId", chat_id, body)
        }
        ["chats", chat_id, "gallery", image_id] if method == "DELETE" => {
            let deleted = state.storage.delete("gallery", image_id)?;
            Ok(json!({ "deleted": deleted, "chatId": chat_id }))
        }
        ["chats", chat_id, "message-count"] if method == "GET" => {
            let messages = messages_for_chat(state, chat_id)?;
            Ok(json!({ "count": messages.len() }))
        }
        ["chats", chat_id, "messages", "bulk-delete"] if method == "POST" => {
            bulk_delete_messages(state, chat_id, body)
        }
        ["chats", chat_id, "messages", "bulk-hidden"] if method == "PATCH" => {
            bulk_hide_messages(state, chat_id, body)
        }
        ["chats", chat_id, "messages", message_id] => {
            chat_message_item(state, method, chat_id, message_id, body)
        }
        ["chats", chat_id, "messages", message_id, "extra"] if method == "PATCH" => {
            patch_message_extra(state, chat_id, message_id, body)
        }
        ["chats", chat_id, "messages", message_id, "swipes"] => {
            message_swipes(state, method, chat_id, message_id, body)
        }
        ["chats", chat_id, "messages", message_id, "swipes", index] if method == "DELETE" => {
            delete_swipe(state, chat_id, message_id, index)
        }
        ["chats", chat_id, "messages", message_id, "active-swipe"] if method == "PUT" => {
            set_active_swipe(state, chat_id, message_id, body)
        }
        ["chats", chat_id, "metadata"] if method == "PATCH" => {
            patch_chat_object_field(state, chat_id, "metadata", body)
        }
        ["world-state", chat_id] if method == "GET" => {
            Ok(get_required(state, "chats", chat_id)?
                .get("gameState")
                .cloned()
                .unwrap_or_else(|| json!({})))
        }
        ["world-state", chat_id] if method == "PATCH" => {
            patch_chat_object_field(state, chat_id, "gameState", body)
        }
        ["chats", chat_id, "summaries"] if method == "PATCH" => {
            patch_chat_object_field(state, chat_id, "metadata", body)
        }
        ["chats", chat_id, "generate-summary"] if method == "POST" => {
            generate_summary(state, chat_id, body)
        }
        ["chats", chat_id, "backfill-summaries"] if method == "POST" => {
            backfill_summaries(state, chat_id, body)
        }
        ["chats", chat_id, "autonomous-unread"] if method == "POST" => {
            mark_autonomous_unread(state, chat_id, body)
        }
        ["chats", chat_id, "autonomous-unread"] if method == "DELETE" => {
            clear_autonomous_unread(state, chat_id)
        }
        ["chats", chat_id, "memories"] if method == "GET" => {
            chat_array_field(state, chat_id, "memories")
        }
        ["chats", chat_id, "memories"] if method == "DELETE" => {
            set_chat_array_field(state, chat_id, "memories", Vec::new())
        }
        ["chats", chat_id, "memories", "refresh"] if method == "POST" => {
            refresh_chat_memories(state, chat_id)
        }
        ["chats", chat_id, "memories", memory_id] if method == "DELETE" => {
            delete_chat_array_item(state, chat_id, "memories", memory_id)
        }
        ["chats", chat_id, "notes"] if method == "GET" => chat_array_field(state, chat_id, "notes"),
        ["chats", chat_id, "notes"] if method == "DELETE" => {
            set_chat_array_field(state, chat_id, "notes", Vec::new())
        }
        ["chats", chat_id, "notes", note_id] if method == "DELETE" => {
            delete_chat_array_item(state, chat_id, "notes", note_id)
        }
        ["chats", chat_id, "branch"] if method == "POST" => branch_chat(state, chat_id, body),
        ["chats", chat_id, "connect"] if method == "POST" => {
            let target = body
                .get("targetChatId")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::invalid_input("targetChatId is required"))?;
            state
                .storage
                .patch("chats", chat_id, json!({ "connectedChatId": target }))?;
            state
                .storage
                .patch("chats", target, json!({ "connectedChatId": chat_id }))?;
            Ok(json!({ "connected": true }))
        }
        ["chats", chat_id, "disconnect"] if method == "POST" => {
            state
                .storage
                .patch("chats", chat_id, json!({ "connectedChatId": Value::Null }))?;
            Ok(json!({ "disconnected": true }))
        }
        ["chats", chat_id, "peek-prompt"] if method == "POST" => peek_prompt(state, chat_id),
        ["chats", chat_id] if method == "DELETE" => {
            delete_chat_with_messages(state, chat_id)?;
            Ok(json!({ "deleted": true }))
        }
        ["chats", chat_id] => {
            collection_item_or_action(state, method, "chats", chat_id, None, body)
        }
        ["chat-folders", "reorder"] if method == "POST" => {
            reorder_collection(state, "chat-folders", "orderedIds", body)
        }
        ["chat-folders", "move-chat"] if method == "POST" => {
            move_child_to_folder(state, "chats", "chatId", "folderId", body)
        }
        ["chat-folders", "reorder-chats"] if method == "POST" => {
            reorder_children(state, "chats", "orderedChatIds", Some("folderId"), body)
        }
        ["chat-folders"] => collection_root(state, method, "chat-folders", body),
        ["chat-folders", id] => {
            collection_item_or_action(state, method, "chat-folders", id, None, body)
        }
        ["connection-folders", "reorder"] if method == "POST" => {
            reorder_collection(state, "connection-folders", "orderedIds", body)
        }
        ["connection-folders", "move-connection"] if method == "POST" => {
            move_child_to_folder(state, "connections", "connectionId", "folderId", body)
        }
        ["connection-folders"] => collection_root(state, method, "connection-folders", body),
        ["connection-folders", id] => {
            collection_item_or_action(state, method, "connection-folders", id, None, body)
        }
        ["connections", id, "duplicate"] if method == "POST" => {
            duplicate_record(state, "connections", id)
        }
        ["connections", id, "default-parameters"] if method == "PUT" => {
            state
                .storage
                .patch("connections", id, json!({ "defaultParameters": body }))
        }
        ["connections", id, "models"] if method == "GET" => connection_models(state, id).await,
        ["connections", id, "test"] if method == "POST" => test_connection(state, id).await,
        ["connections", id, "test-message"] if method == "POST" => test_message(state, id).await,
        ["connections", id, "diagnose-claude-subscription"] if method == "POST" => {
            diagnose_claude_subscription(state, id)
        }
        ["connections", id, "test-image"] if method == "POST" => {
            test_image_generation(state, id).await
        }
        ["connections"] => collection_root(state, method, "connections", body),
        ["connections", id] => {
            collection_item_or_action(state, method, "connections", id, None, body)
        }
        ["prompts", "default"] if method == "GET" => default_prompt(state),
        ["prompts", preset_id, "full"] if method == "GET" => preset_full(state, preset_id),
        ["prompts", "export-bulk"] if method == "POST" => {
            export_records(state, "marinara_presets", "prompts", body)
        }
        ["prompts", preset_id, "export"] if method == "GET" => export_prompt(state, preset_id),
        ["prompts", preset_id, "preview"] if method == "POST" => {
            preview_prompt(state, preset_id, body)
        }
        ["prompts", preset_id, nested, "reorder"]
            if matches!(*nested, "groups" | "sections" | "variables") && method == "PUT" =>
        {
            reorder_prompt_nested(state, preset_id, nested, body)
        }
        ["prompts", preset_id, nested]
            if matches!(*nested, "groups" | "sections" | "variables") =>
        {
            prompt_nested_root(state, method, preset_id, nested, body)
        }
        ["prompts", preset_id, nested, nested_id]
            if matches!(*nested, "groups" | "sections" | "variables") =>
        {
            prompt_nested_item(state, method, preset_id, nested, nested_id, body)
        }
        ["prompts", id, "duplicate"] if method == "POST" => duplicate_record(state, "prompts", id),
        ["prompts", id, "set-default"] if method == "POST" => {
            set_default_prompt(state, id)
        }
        ["prompts"] => collection_root(state, method, "prompts", body),
        ["prompts", id] => collection_item_or_action(state, method, "prompts", id, None, body),
        ["lorebooks", "export-bulk"] if method == "POST" => export_lorebooks(state, body),
        ["lorebooks", "search", "entries"] if method == "GET" => search_lorebook_entries(
            state,
            route.query.get("q").map(String::as_str).unwrap_or(""),
        ),
        ["lorebooks", "scan", chat_id] if method == "GET" => scan_lorebooks(state, chat_id),
        ["lorebooks", "images", "file-path", encoded] if method == "GET" => {
            lorebook_image_file_path(state, encoded)
        }
        ["lorebooks", lorebook_id, "export"] if method == "GET" => export_lorebook(
            state,
            lorebook_id,
            route.query.get("format").map(String::as_str),
        ),
        ["lorebooks", lorebook_id, "image"] if method == "POST" => {
            update_lorebook_image(state, lorebook_id, body)
        }
        ["lorebooks", lorebook_id, "vectorize"] if method == "POST" => {
            vectorize_lorebook(state, lorebook_id, body).await
        }
        ["lorebooks", lorebook_id, "entries"] if method == "GET" => list_collection(
            state,
            "lorebook-entries",
            Some(("lorebookId", *lorebook_id)),
        ),
        ["lorebooks", lorebook_id, "entries"] if method == "POST" => {
            create_nested(state, "lorebook-entries", "lorebookId", lorebook_id, body)
        }
        ["lorebooks", lorebook_id, "entries", "bulk"] if method == "POST" => {
            create_lorebook_entries_bulk(state, lorebook_id, body)
        }
        ["lorebooks", lorebook_id, "entries", "reorder"] if method == "PUT" => {
            reorder_lorebook_entries(state, lorebook_id, body)
        }
        ["lorebooks", lorebook_id, "entries", "transfer"] if method == "POST" => {
            transfer_lorebook_entries(state, lorebook_id, body)
        }
        ["lorebooks", lorebook_id, "entries", entry_id] => nested_item(
            state,
            method,
            "lorebook-entries",
            "lorebookId",
            lorebook_id,
            entry_id,
            body,
        ),
        ["lorebooks", lorebook_id, "folders"] if method == "GET" => list_collection(
            state,
            "lorebook-folders",
            Some(("lorebookId", *lorebook_id)),
        ),
        ["lorebooks", lorebook_id, "folders"] if method == "POST" => {
            create_nested(state, "lorebook-folders", "lorebookId", lorebook_id, body)
        }
        ["lorebooks", lorebook_id, "folders", "reorder"] if method == "PUT" => {
            reorder_lorebook_folders(state, lorebook_id, body)
        }
        ["lorebooks", lorebook_id, "folders", folder_id] => nested_item(
            state,
            method,
            "lorebook-folders",
            "lorebookId",
            lorebook_id,
            folder_id,
            body,
        ),
        ["lorebooks"] => collection_root(state, method, "lorebooks", body),
        ["lorebooks", id] => collection_item_or_action(state, method, "lorebooks", id, None, body),
        ["game-assets"] if method == "GET" => Ok(json!({
            "items": state.game_assets.list(None)?,
            "root": state.game_assets.root().to_string_lossy()
        })),
        ["game-assets", "manifest"] if method == "GET" => game_assets_manifest(state),
        ["game-assets", "tree"] if method == "GET" => game_assets_tree(state),
        ["game-assets", "upload"] if method == "POST" => game_assets_upload(state, body),
        ["game-assets", "folders"] if method == "POST" => {
            let path = body.get("path").and_then(Value::as_str).unwrap_or("");
            state.game_assets.create_folder(path)?;
            Ok(json!({ "path": path }))
        }
        ["game-assets", "folders", "description"] if method == "PATCH" => {
            game_assets_folder_description(state, body)
        }
        ["game-assets", "folders", encoded] if method == "DELETE" => {
            let recursive = route.query.get("recursive").map(String::as_str) == Some("true");
            state.game_assets.remove(&decode_path(encoded), recursive)?;
            Ok(json!({ "deleted": true }))
        }
        ["game-assets", "file", encoded] if method == "DELETE" => {
            state.game_assets.remove(&decode_path(encoded), false)?;
            Ok(json!({ "deleted": true }))
        }
        ["game-assets", "file-path", encoded] if method == "GET" => {
            Ok(json!({ "path": state.game_assets.absolute_path_string(&decode_path(encoded))? }))
        }
        ["game-assets", "file-content", encoded] if method == "GET" => {
            Ok(json!({ "content": state.game_assets.read_text(&decode_path(encoded))? }))
        }
        ["game-assets", "file-content", encoded] if method == "PUT" => {
            let content = body.get("content").and_then(Value::as_str).unwrap_or("");
            state
                .game_assets
                .write_text(&decode_path(encoded), content)?;
            Ok(json!({ "saved": true }))
        }
        ["game-assets", "rename"] if method == "POST" => {
            let path = body
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::invalid_input("path is required"))?;
            let new_name = body
                .get("newName")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::invalid_input("newName is required"))?;
            state.game_assets.rename(path, new_name)
        }
        ["game-assets", "move"] if method == "POST" => {
            let path = body
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::invalid_input("path is required"))?;
            let target = body
                .get("targetFolder")
                .and_then(Value::as_str)
                .unwrap_or("");
            state.game_assets.move_to_folder(path, target)
        }
        ["game-assets", "copy"] if method == "POST" => {
            let path = body
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::invalid_input("path is required"))?;
            let target = body
                .get("targetFolder")
                .and_then(Value::as_str)
                .unwrap_or("");
            state.game_assets.copy_to_folder(path, target)
        }
        ["game-assets", "move-bulk"] if method == "POST" => {
            let paths = string_array_from_value(body.get("paths"));
            let target = body
                .get("targetFolder")
                .and_then(Value::as_str)
                .unwrap_or("");
            Ok(state.game_assets.move_many(&paths, target))
        }
        ["game-assets", "copy-bulk"] if method == "POST" => {
            let paths = string_array_from_value(body.get("paths"));
            let target = body
                .get("targetFolder")
                .and_then(Value::as_str)
                .unwrap_or("");
            Ok(state.game_assets.copy_many(&paths, target))
        }
        ["game-assets", "delete-bulk"] if method == "POST" => {
            let paths = string_array_from_value(body.get("paths"));
            Ok(state.game_assets.delete_many(&paths))
        }
        ["game-assets", "file-info", encoded] if method == "GET" => {
            state.game_assets.file_info(&decode_path(encoded))
        }
        ["game-assets", "rescan"] if method == "POST" => game_assets_rescan(state),
        ["game-assets", "open-folder"] if method == "POST" => game_assets_open_folder(state, body),
        ["sprites", "capabilities"] if method == "GET" => sprite_capabilities(),
        ["sprites", "generate-sheet", "preview"] if method == "POST" => {
            generate_sprite_sheet_preview(state, body).await
        }
        ["sprites", "generate-sheet"] if method == "POST" => {
            generate_sprite_sheet(state, body).await
        }
        ["sprites", "cleanup"] if method == "POST" => cleanup_generated_sprites(body),
        ["sprites", character_id, "cleanup-saved"] if method == "POST" => {
            cleanup_saved_sprites(state, character_id, body)
        }
        ["sprites", character_id, "cleanup-restore"] if method == "POST" => {
            restore_sprite_cleanup(state, character_id, body)
        }
        ["sprites", character_id] if method == "GET" => list_sprites(state, character_id),
        ["sprites", character_id] if method == "POST" => upload_sprite(state, character_id, body),
        ["sprites", character_id, expression] if method == "DELETE" => {
            delete_sprite(state, character_id, expression)
        }
        ["agents", "toggle", agent_type] if method == "PUT" => toggle_agent_type(state, agent_type),
        ["agents", "type", agent_type] if method == "PATCH" => {
            patch_agent_type(state, agent_type, body)
        }
        ["agents", "cadence", agent_type, chat_id] if method == "GET" => {
            agent_cadence_status(state, agent_type, chat_id)
        }
        ["agents", "runs", chat_id, "custom"] if method == "GET" => {
            list_collection(state, "agent-runs", Some(("chatId", *chat_id)))
        }
        ["agents", "runs", id] if method == "PATCH" => state.storage.patch("agent-runs", id, body),
        ["agents", "runs", chat_id] if method == "DELETE" => {
            clear_agent_runs_and_memory_for_chat(state, chat_id)
        }
        ["agents", "memory", agent_type, chat_id] => {
            agent_memory(state, method, agent_type, chat_id, body)
        }
        ["agents", "echo-messages", chat_id] => echo_messages(state, method, chat_id),
        ["agents"] => collection_root(state, method, "agents", body),
        ["agents", id] => collection_item_or_action(state, method, "agents", id, None, body),
        ["custom-tools", "capabilities"] if method == "GET" => Ok(custom_tool_capabilities()),
        ["custom-tools", "execute"] if method == "POST" => execute_custom_tool(state, body).await,
        ["regex-scripts", "reorder"] if method == "PUT" => {
            reorder_collection(state, "regex-scripts", "scriptIds", body)
        }
        ["themes", "active"] if method == "PUT" => {
            handle_singleton(state, "PUT", "app-settings", "active-theme", body)
        }
        ["themes", "active"] if method == "GET" => {
            handle_singleton(state, "GET", "app-settings", "active-theme", Value::Null)
        }
        ["chat-presets", rest @ ..] => chat_presets_call(state, method, rest, body),
        ["haptic", rest @ ..] => haptic_call(rest, body).await,
        ["spotify", rest @ ..] => spotify_call(state, method, rest, &route, body).await,
        [collection] => collection_root(state, method, collection, body),
        [collection, id] => collection_item_or_action(state, method, collection, id, None, body),
        _ => Err(AppError::new(
            "route_not_found",
            format!("Unknown local route: {method} {path}"),
        )),
    }
}
