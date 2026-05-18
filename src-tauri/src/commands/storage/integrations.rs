use super::shared::ParsedPath;
use super::*;

mod haptic;
mod spotify;
mod tts;

pub(crate) async fn tts_call(
    state: &AppState,
    method: &str,
    rest: &[&str],
    body: Value,
) -> AppResult<Value> {
    tts::tts_call(state, method, rest, body).await
}

pub(crate) async fn spotify_call(
    state: &AppState,
    method: &str,
    rest: &[&str],
    route: &ParsedPath,
    body: Value,
) -> AppResult<Value> {
    spotify::spotify_call(state, method, rest, route, body).await
}

pub(crate) async fn game_spotify_candidates(
    state: &AppState,
    query: &str,
    limit: u32,
    recent_track_uris: &[String],
) -> AppResult<Value> {
    spotify::game_spotify_candidates(state, query, limit, recent_track_uris).await
}

pub(crate) async fn game_spotify_play(
    state: &AppState,
    track: &Value,
    device_id: Option<&str>,
) -> AppResult<Value> {
    spotify::game_spotify_play(state, track, device_id).await
}

pub(crate) async fn haptic_call(rest: &[&str], body: Value) -> AppResult<Value> {
    haptic::haptic_call(rest, body).await
}
pub(crate) fn sidecar_call(method: &str, rest: &[&str], body: Value) -> AppResult<Value> {
    match (method, rest) {
        ("GET", ["status"]) => Ok(json!({
            "enabled": false,
            "running": false,
            "ready": false,
            "inferenceReady": false,
            "modelLoaded": false,
            "platform": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "curatedModels": [],
            "message": "Sidecar is deferred external-service scope in the Tauri migration."
        })),
        ("PATCH", ["config"]) => Ok(json!({ "config": body })),
        ("POST", ["models", "list-huggingface"]) => Ok(json!({ "models": [] })),
        ("POST", ["test-message"]) => {
            Ok(json!({ "success": false, "response": "", "error": "Sidecar is deferred." }))
        }
        ("POST", ["download", "cancel"]) | ("POST", ["unload"]) | ("POST", ["restart"]) => {
            Ok(json!({ "ok": true }))
        }
        ("DELETE", ["model"]) => Ok(json!({ "deleted": false })),
        _ => Ok(
            json!({ "ok": false, "message": "Sidecar route is deferred external-service scope." }),
        ),
    }
}
