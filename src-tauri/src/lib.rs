mod app;
#[path = "commands/storage.rs"]
mod storage_commands;
mod state;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let state = app::build_state(app.handle())
                .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            storage_commands::api_request,
            storage_commands::api_stream_events,
            storage_commands::api_stream_channel,
            storage_commands::load_url_binary,
            storage_commands::profile_export,
            storage_commands::profile_import,
            storage_commands::game_assets_list,
            storage_commands::game_assets_tree,
            storage_commands::game_assets_rescan,
            storage_commands::game_assets_create_folder,
            storage_commands::game_assets_delete_folder,
            storage_commands::game_assets_delete_file,
            storage_commands::game_assets_file_path,
            storage_commands::game_assets_read_text,
            storage_commands::game_assets_write_text,
            storage_commands::game_assets_rename,
            storage_commands::game_assets_move,
            storage_commands::game_assets_copy,
            storage_commands::game_assets_move_bulk,
            storage_commands::game_assets_copy_bulk,
            storage_commands::game_assets_delete_bulk,
            storage_commands::game_assets_file_info,
            storage_commands::game_assets_folder_description,
            storage_commands::game_assets_upload,
            storage_commands::game_assets_open_folder,
            storage_commands::background_file_path,
            storage_commands::lorebook_image_file_path,
            storage_commands::gif_search,
            storage_commands::tts_config,
            storage_commands::tts_update_config,
            storage_commands::tts_voices,
            storage_commands::tts_speak,
            storage_commands::haptic_status,
            storage_commands::haptic_connect,
            storage_commands::haptic_disconnect,
            storage_commands::haptic_start_scan,
            storage_commands::haptic_stop_scan,
            storage_commands::haptic_command,
            storage_commands::haptic_stop_all,
            storage_commands::spotify_status,
            storage_commands::spotify_authorize,
            storage_commands::spotify_exchange,
            storage_commands::spotify_disconnect,
            storage_commands::spotify_player,
            storage_commands::spotify_devices,
            storage_commands::spotify_search_tracks,
            storage_commands::spotify_play_track,
            storage_commands::spotify_dj_mari_playlist,
            storage_commands::spotify_player_play,
            storage_commands::spotify_player_pause,
            storage_commands::spotify_player_next,
            storage_commands::spotify_player_previous,
            storage_commands::spotify_player_transfer,
            storage_commands::spotify_player_volume,
            storage_commands::spotify_player_shuffle,
            storage_commands::spotify_player_repeat,
            storage_commands::knowledge_sources_list,
            storage_commands::knowledge_source_upload,
            storage_commands::knowledge_source_delete,
            storage_commands::knowledge_source_text,
            storage_commands::import_marinara,
            storage_commands::import_marinara_file,
            storage_commands::import_st_character,
            storage_commands::import_st_character_batch,
            storage_commands::import_st_character_inspect,
            storage_commands::import_st_chat,
            storage_commands::import_st_chat_into_group,
            storage_commands::import_st_preset,
            storage_commands::import_st_lorebook,
            storage_commands::import_list_directory,
            storage_commands::import_st_bulk_scan,
            storage_commands::import_st_bulk_run,
            storage_commands::import_st_bulk_run_events,
            storage_commands::storage_list,
            storage_commands::storage_get,
            storage_commands::storage_create,
            storage_commands::storage_update,
            storage_commands::storage_delete,
            storage_commands::llm_complete,
            storage_commands::llm_stream_channel,
            storage_commands::llm_list_models,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
