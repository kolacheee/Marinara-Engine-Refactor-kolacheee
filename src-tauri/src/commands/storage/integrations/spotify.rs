use super::super::images::percent_encode_component;
use super::super::shared::*;
use super::super::*;
use super::spotify_callback::start_callback_listener;

const SPOTIFY_SCOPES: &str = "streaming user-modify-playback-state user-read-playback-state user-read-currently-playing user-read-private playlist-read-private playlist-modify-public playlist-modify-private user-library-read";
const SPOTIFY_REDIRECT_URI: &str = "http://127.0.0.1:8754/spotify/callback";
const AUTH_TTL_MS: u128 = 10 * 60_000;

pub(crate) async fn spotify_call(
    state: &AppState,
    method: &str,
    rest: &[&str],
    route: &ParsedPath,
    body: Value,
) -> AppResult<Value> {
    match (method, rest) {
        ("GET", ["authorize"]) | ("POST", ["authorize"]) => authorize(state, route, &body),
        ("POST", ["exchange"]) => exchange(state, body).await,
        ("POST", ["refresh"]) => {
            let agent_id = string_param(route, &body, "agentId")
                .ok_or_else(|| AppError::invalid_input("agentId is required"))?;
            refresh_agent_token(state, &agent_id)
                .await
                .map(|_| json!({ "success": true }))
        }
        ("GET", ["status"]) | ("POST", ["status"]) => status(state, route, &body),
        ("GET", ["access-token"]) => access_token(state, route, &body).await,
        ("GET", ["player"]) => player(state, route, &body).await,
        ("GET", ["devices"]) => devices(state, route, &body).await,
        ("GET", ["playlists"]) => playlists(state, route, &body).await,
        ("POST", ["playlist-tracks"]) => playlist_tracks(state, body).await,
        ("POST", ["search-tracks"]) => search_tracks(state, body).await,
        ("POST", ["play-track"]) => play_track(state, body).await,
        ("POST", ["dj-mari-playlist"]) => dj_mari_playlist(state, body).await,
        ("PUT", ["player", "play"]) => {
            player_control(state, route, body, "/me/player/play", "PUT").await
        }
        ("PUT", ["player", "pause"]) => {
            player_control(state, route, body, "/me/player/pause", "PUT").await
        }
        ("POST", ["player", "next"]) => {
            player_control(state, route, body, "/me/player/next", "POST").await
        }
        ("POST", ["player", "previous"]) => {
            player_control(state, route, body, "/me/player/previous", "POST").await
        }
        ("PUT", ["player", "volume"]) => player_volume(state, route, body).await,
        ("PUT", ["player", "shuffle"]) => player_shuffle(state, route, body).await,
        ("PUT", ["player", "repeat"]) => player_repeat(state, route, body).await,
        ("PUT", ["player", "transfer"]) => player_transfer(state, route, body).await,
        ("POST", ["disconnect"]) => disconnect(state, body),
        _ => Err(AppError::new(
            "route_not_found",
            format!("Unknown spotify route: {method} /{}", rest.join("/")),
        )),
    }
}

async fn search_tracks(state: &AppState, body: Value) -> AppResult<Value> {
    let query = body
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or("cinematic adventure soundtrack");
    let limit = body
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(50)
        .clamp(1, 50) as u32;
    let recent = body
        .get("recentTrackUris")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
        .filter(|uri| uri.starts_with("spotify:track:"))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>()
    })
    .unwrap_or_default();
    let route = ParsedPath::new("");
    let credentials = resolve_credentials(state, &route, &body).await?;
    let params = form_urlencoded(&[
        ("q", query),
        ("type", "track"),
        ("limit", &limit.to_string()),
    ]);
    let response = spotify_api(&credentials, &format!("/search?{params}"), "GET", None).await?;
    if !(200..300).contains(&response.status) {
        return Err(AppError::with_details(
            "spotify_api_error",
            "Spotify track search failed",
            json!({ "status": response.status, "body": response.body }),
        ));
    }
    let recent = recent
        .iter()
        .map(|uri| uri.as_str())
        .collect::<std::collections::HashSet<_>>();
    let mut tracks = response
        .json
        .get("tracks")
        .and_then(|tracks| tracks.get("items"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(map_track_candidate)
        .filter(|track| {
            track
                .get("uri")
                .and_then(Value::as_str)
                .is_some_and(|uri| !recent.contains(uri))
        })
        .collect::<Vec<_>>();
    if tracks.is_empty() {
        tracks = response
            .json
            .get("tracks")
            .and_then(|tracks| tracks.get("items"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(map_track_candidate)
            .collect();
    }
    Ok(json!({
        "enabled": true,
        "tracks": tracks,
        "candidateMode": "spotify_search",
        "source": "spotify"
    }))
}

async fn play_track(state: &AppState, body: Value) -> AppResult<Value> {
    let track = body
        .get("track")
        .ok_or_else(|| AppError::invalid_input("track is required"))?;
    let device_id = body.get("deviceId").and_then(Value::as_str);
    game_spotify_play(state, track, device_id).await
}

async fn playlist_tracks(state: &AppState, body: Value) -> AppResult<Value> {
    let playlist_id = body
        .get("playlistId")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::invalid_input("playlistId is required"))?;
    let limit = body
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(50)
        .clamp(1, 50) as u32;
    let offset = body
        .get("offset")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .min(10_000) as u32;
    let route = ParsedPath::new("");
    let credentials = resolve_credentials(state, &route, &body).await?;
    let path = if playlist_id == "liked" {
        format!("/me/tracks?limit={limit}&offset={offset}")
    } else {
        format!(
            "/playlists/{}/tracks?limit={limit}&offset={offset}",
            percent_encode_component(playlist_id)
        )
    };
    let response = spotify_api(&credentials, &path, "GET", None).await?;
    if !(200..300).contains(&response.status) {
        return Err(AppError::with_details(
            "spotify_api_error",
            "Spotify playlist tracks failed",
            json!({ "status": response.status, "body": response.body }),
        ));
    }
    let tracks = response
        .json
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| item.get("track").cloned().or(Some(item)))
        .filter_map(|track| map_track_candidate(track))
        .collect::<Vec<_>>();
    Ok(json!({
        "tracks": tracks,
        "next": response.json.get("next").cloned().unwrap_or(Value::Null),
        "total": response.json.get("total").cloned().unwrap_or(Value::Null),
        "offset": offset,
        "limit": limit
    }))
}

pub(crate) async fn game_spotify_play(
    state: &AppState,
    track: &Value,
    device_id: Option<&str>,
) -> AppResult<Value> {
    let uri = track
        .get("uri")
        .and_then(Value::as_str)
        .filter(|uri| uri.starts_with("spotify:track:"))
        .ok_or_else(|| AppError::invalid_input("A valid Spotify track URI is required"))?;
    let route = ParsedPath::new("");
    let body = Value::Null;
    let credentials = resolve_credentials(state, &route, &body).await?;
    let path = spotify_control_path("/me/player/play", device_id);
    let response = spotify_api(&credentials, &path, "PUT", Some(json!({ "uris": [uri] }))).await?;
    if !(200..300).contains(&response.status) && response.status != 204 {
        return Err(AppError::with_details(
            "spotify_api_error",
            "Spotify scene music playback failed",
            json!({ "status": response.status, "body": response.body }),
        ));
    }
    Ok(json!({ "played": true, "track": track }))
}

fn authorize(state: &AppState, route: &ParsedPath, body: &Value) -> AppResult<Value> {
    let client_id = string_param(route, body, "clientId")
        .ok_or_else(|| AppError::invalid_input("clientId is required"))?;
    let agent_id = string_param(route, body, "agentId")
        .ok_or_else(|| AppError::invalid_input("agentId is required"))?;
    let code_verifier = random_token(64);
    let code_challenge = code_challenge(&code_verifier);
    let auth_state = random_token(32);
    state.storage.upsert_with_id(
        "app-settings",
        &format!("spotify-pending-{auth_state}"),
        json!({
            "value": {
                "codeVerifier": code_verifier,
                "clientId": client_id,
                "agentId": agent_id,
                "redirectUri": SPOTIFY_REDIRECT_URI,
                "createdAt": now_millis()
            }
        }),
    )?;
    let callback_listener_started = start_callback_listener(state.clone());
    let params = form_urlencoded(&[
        ("response_type", "code"),
        ("client_id", &client_id),
        ("scope", SPOTIFY_SCOPES),
        ("code_challenge_method", "S256"),
        ("code_challenge", &code_challenge),
        ("redirect_uri", SPOTIFY_REDIRECT_URI),
        ("state", &auth_state),
    ]);
    Ok(json!({
        "authUrl": format!("https://accounts.spotify.com/authorize?{params}"),
        "redirectUri": SPOTIFY_REDIRECT_URI,
        "callbackListenerStarted": callback_listener_started
    }))
}

pub(super) async fn exchange(state: &AppState, body: Value) -> AppResult<Value> {
    let (code, auth_state) = spotify_code_and_state(&body)?;
    let key = format!("spotify-pending-{auth_state}");
    let pending_record = state.storage.get("app-settings", &key)?.ok_or_else(|| {
        AppError::invalid_input("Authorization session expired or was already used.")
    })?;
    let pending = pending_record
        .get("value")
        .cloned()
        .unwrap_or(pending_record);
    let created_at = pending
        .get("createdAt")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u128;
    if created_at > 0 && now_millis().saturating_sub(created_at) > AUTH_TTL_MS {
        let _ = state.storage.delete("app-settings", &key);
        return Err(AppError::invalid_input(
            "Authorization session expired or was already used.",
        ));
    }
    let code_verifier = pending
        .get("codeVerifier")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::invalid_input("Spotify authorization verifier is missing"))?
        .to_string();
    let client_id = pending
        .get("clientId")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::invalid_input("Spotify client id is missing"))?
        .to_string();
    let agent_id = pending
        .get("agentId")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::invalid_input("Spotify agent id is missing"))?
        .to_string();
    let redirect_uri = pending
        .get("redirectUri")
        .and_then(Value::as_str)
        .unwrap_or(SPOTIFY_REDIRECT_URI)
        .to_string();
    let token = spotify_token_request(&[
        ("client_id", client_id.as_str()),
        ("grant_type", "authorization_code"),
        ("code", code.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
        ("code_verifier", code_verifier.as_str()),
    ])
    .await?;
    let _ = state.storage.delete("app-settings", &key);
    save_spotify_tokens(state, &agent_id, &client_id, &token)?;
    Ok(json!({ "success": true }))
}

fn status(state: &AppState, route: &ParsedPath, body: &Value) -> AppResult<Value> {
    let agent_id = string_param(route, body, "agentId")
        .ok_or_else(|| AppError::invalid_input("agentId is required"))?;
    let agent = get_required(state, "agents", &agent_id)?;
    let settings = agent_settings(&agent);
    let has_token = settings
        .get("spotifyAccessToken")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.is_empty());
    let has_refresh = settings
        .get("spotifyRefreshToken")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.is_empty());
    let expires_at = settings
        .get("spotifyExpiresAt")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u128;
    let scopes = scope_list(
        settings
            .get("spotifyScope")
            .and_then(Value::as_str)
            .unwrap_or(""),
    );
    let missing_scopes = SPOTIFY_SCOPES
        .split_whitespace()
        .filter(|scope| !scopes.iter().any(|existing| existing == scope))
        .collect::<Vec<_>>();
    Ok(json!({
        "connected": has_token && has_refresh,
        "expired": expires_at > 0 && now_millis() > expires_at,
        "clientId": settings.get("spotifyClientId").cloned().unwrap_or(Value::Null),
        "redirectUri": SPOTIFY_REDIRECT_URI,
        "scopes": scopes,
        "missingScopes": missing_scopes
    }))
}

async fn access_token(state: &AppState, route: &ParsedPath, body: &Value) -> AppResult<Value> {
    let credentials = resolve_credentials(state, route, body).await?;
    Ok(json!({
        "accessToken": credentials.access_token,
        "expiresAt": credentials.expires_at,
        "agentId": credentials.agent_id,
        "scopes": credentials.scopes,
        "hasStreamingScope": credentials.scopes.iter().any(|scope| scope == "streaming")
    }))
}

async fn player(state: &AppState, route: &ParsedPath, body: &Value) -> AppResult<Value> {
    let credentials = resolve_credentials(state, route, body).await?;
    let response = spotify_api(&credentials, "/me/player", "GET", None).await?;
    if response.status == 204 {
        return Ok(json!({ "connected": true, "active": false }));
    }
    if !(200..300).contains(&response.status) {
        return Err(AppError::with_details(
            "spotify_api_error",
            "Spotify playback state failed",
            json!({ "status": response.status, "body": response.body }),
        ));
    }
    Ok(map_playback(&response.json))
}

async fn devices(state: &AppState, route: &ParsedPath, body: &Value) -> AppResult<Value> {
    let credentials = resolve_credentials(state, route, body).await?;
    let response = spotify_api(&credentials, "/me/player/devices", "GET", None).await?;
    if !(200..300).contains(&response.status) {
        return Err(AppError::with_details(
            "spotify_api_error",
            "Spotify devices failed",
            json!({ "status": response.status, "body": response.body }),
        ));
    }
    let devices = response
        .json
        .get("devices")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|device| {
            json!({
                "id": device.get("id").cloned().unwrap_or(Value::Null),
                "name": device.get("name").and_then(Value::as_str).unwrap_or("Spotify device"),
                "type": device.get("type").cloned().unwrap_or(Value::Null),
                "isActive": device.get("is_active").and_then(Value::as_bool).unwrap_or(false),
                "volume": device.get("volume_percent").cloned().unwrap_or(Value::Null)
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({ "devices": devices }))
}

async fn playlists(state: &AppState, route: &ParsedPath, body: &Value) -> AppResult<Value> {
    let credentials = resolve_credentials(state, route, body).await?;
    let limit = route
        .query
        .get("limit")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(50)
        .clamp(1, 50);
    let response = spotify_api(
        &credentials,
        &format!("/me/playlists?limit={limit}"),
        "GET",
        None,
    )
    .await?;
    if !(200..300).contains(&response.status) {
        return Err(AppError::with_details(
            "spotify_api_error",
            "Spotify playlists failed",
            json!({ "status": response.status, "body": response.body }),
        ));
    }
    let playlists = response
        .json
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|playlist| {
            json!({
                "id": playlist.get("id").and_then(Value::as_str).unwrap_or(""),
                "name": playlist.get("name").and_then(Value::as_str).unwrap_or("Untitled playlist"),
                "uri": playlist.get("uri").and_then(Value::as_str).unwrap_or(""),
                "trackCount": playlist.get("tracks").and_then(|tracks| tracks.get("total")).cloned().unwrap_or(Value::Null),
                "owned": Value::Null
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({ "playlists": playlists }))
}

async fn dj_mari_playlist(state: &AppState, body: Value) -> AppResult<Value> {
    let route = ParsedPath {
        parts: Vec::new(),
        query: HashMap::new(),
    };
    let credentials = resolve_credentials(state, &route, &body).await?;
    let user = spotify_api(&credentials, "/me", "GET", None).await?;
    if !(200..300).contains(&user.status) {
        return Err(AppError::with_details(
            "spotify_api_error",
            "Spotify user lookup failed",
            json!({ "status": user.status, "body": user.body }),
        ));
    }
    let user_id = user
        .json
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::new("spotify_api_error", "Spotify user id missing"))?;
    let playlist_body = json!({
        "name": "DJ Mari Mix",
        "description": "Created by Marinara Engine.",
        "public": false
    });
    let created = spotify_api(
        &credentials,
        &format!("/users/{}/playlists", percent_encode_component(user_id)),
        "POST",
        Some(playlist_body),
    )
    .await?;
    if !(200..300).contains(&created.status) {
        return Err(AppError::with_details(
            "spotify_api_error",
            "Spotify playlist creation failed",
            json!({ "status": created.status, "body": created.body }),
        ));
    }
    Ok(json!({
        "success": true,
        "name": created.json.get("name").and_then(Value::as_str).unwrap_or("DJ Mari Mix"),
        "playlistUrl": created.json.get("external_urls").and_then(|urls| urls.get("spotify")).cloned().unwrap_or(Value::Null),
        "requestedTrackCount": 0,
        "trackCount": 0,
        "playbackStarted": false
    }))
}

async fn player_control(
    state: &AppState,
    route: &ParsedPath,
    body: Value,
    path: &str,
    method: &str,
) -> AppResult<Value> {
    let credentials = resolve_credentials(state, route, &body).await?;
    let device_id = body.get("deviceId").and_then(Value::as_str);
    let path = spotify_control_path(path, device_id);
    let payload = if path.ends_with("/play") || path.contains("/play?") {
        let mut object = Map::new();
        if let Some(context_uri) = body
            .get("contextUri")
            .and_then(Value::as_str)
            .filter(|value| value.starts_with("spotify:"))
        {
            object.insert(
                "context_uri".to_string(),
                Value::String(context_uri.to_string()),
            );
        } else if let Some(uris) = body.get("uris").and_then(Value::as_array) {
            object.insert(
                "uris".to_string(),
                Value::Array(
                    uris.iter()
                        .filter_map(Value::as_str)
                        .filter(|uri| uri.starts_with("spotify:"))
                        .map(|uri| Value::String(uri.to_string()))
                        .collect(),
                ),
            );
        } else if let Some(uri) = body
            .get("uri")
            .and_then(Value::as_str)
            .filter(|value| value.starts_with("spotify:"))
        {
            object.insert("uris".to_string(), json!([uri]));
        }
        if object.is_empty() {
            None
        } else {
            Some(Value::Object(object))
        }
    } else {
        None
    };
    let response = spotify_api(&credentials, &path, method, payload).await?;
    if !(200..300).contains(&response.status) && response.status != 204 {
        return Err(AppError::with_details(
            "spotify_api_error",
            "Spotify playback command failed",
            json!({ "status": response.status, "body": response.body }),
        ));
    }
    Ok(json!({ "success": true }))
}

async fn player_volume(state: &AppState, route: &ParsedPath, body: Value) -> AppResult<Value> {
    let credentials = resolve_credentials(state, route, &body).await?;
    let volume = body
        .get("volume")
        .and_then(Value::as_i64)
        .unwrap_or(50)
        .clamp(0, 100);
    let base = format!("/me/player/volume?volume_percent={volume}");
    let path = spotify_control_path(&base, body.get("deviceId").and_then(Value::as_str));
    let response = spotify_api(&credentials, &path, "PUT", None).await?;
    if !(200..300).contains(&response.status) && response.status != 204 {
        let body_text = response.body.to_ascii_lowercase();
        if body_text.contains("cannot control device volume") {
            return Err(AppError::with_details(
                "SPOTIFY_VOLUME_UNSUPPORTED",
                "This Spotify device does not allow remote volume control. Use the device volume buttons instead.",
                json!({ "status": response.status }),
            ));
        }
        return Err(AppError::with_details(
            "spotify_api_error",
            "Spotify volume failed",
            json!({ "status": response.status, "body": response.body }),
        ));
    }
    Ok(json!({ "success": true, "volume": volume }))
}

async fn player_shuffle(state: &AppState, route: &ParsedPath, body: Value) -> AppResult<Value> {
    let credentials = resolve_credentials(state, route, &body).await?;
    let enabled = body
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let base = format!("/me/player/shuffle?state={enabled}");
    let path = spotify_control_path(&base, body.get("deviceId").and_then(Value::as_str));
    let response = spotify_api(&credentials, &path, "PUT", None).await?;
    if !(200..300).contains(&response.status) && response.status != 204 {
        return Err(AppError::with_details(
            "spotify_api_error",
            "Spotify shuffle failed",
            json!({ "status": response.status, "body": response.body }),
        ));
    }
    Ok(json!({ "success": true, "shuffle": enabled }))
}

async fn player_repeat(state: &AppState, route: &ParsedPath, body: Value) -> AppResult<Value> {
    let credentials = resolve_credentials(state, route, &body).await?;
    let repeat = body.get("state").and_then(Value::as_str).unwrap_or("off");
    if !matches!(repeat, "off" | "track" | "context") {
        return Err(AppError::invalid_input(
            "repeat state must be off, track, or context",
        ));
    }
    let base = format!("/me/player/repeat?state={repeat}");
    let path = spotify_control_path(&base, body.get("deviceId").and_then(Value::as_str));
    let response = spotify_api(&credentials, &path, "PUT", None).await?;
    if !(200..300).contains(&response.status) && response.status != 204 {
        return Err(AppError::with_details(
            "spotify_api_error",
            "Spotify repeat failed",
            json!({ "status": response.status, "body": response.body }),
        ));
    }
    Ok(json!({ "success": true, "repeat": repeat }))
}

async fn player_transfer(state: &AppState, route: &ParsedPath, body: Value) -> AppResult<Value> {
    let credentials = resolve_credentials(state, route, &body).await?;
    let device_id = required_string(&body, "deviceId")?;
    let response = spotify_api(
        &credentials,
        "/me/player",
        "PUT",
        Some(json!({ "device_ids": [device_id], "play": body.get("play").and_then(Value::as_bool).unwrap_or(false) })),
    )
    .await?;
    if !(200..300).contains(&response.status) && response.status != 204 {
        return Err(AppError::with_details(
            "spotify_api_error",
            "Spotify transfer failed",
            json!({ "status": response.status, "body": response.body }),
        ));
    }
    Ok(json!({ "success": true }))
}

fn disconnect(state: &AppState, body: Value) -> AppResult<Value> {
    let agent_id = body
        .get("agentId")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::invalid_input("agentId is required"))?;
    let agent = get_required(state, "agents", agent_id)?;
    let mut settings = agent_settings(&agent);
    for key in [
        "spotifyAccessToken",
        "spotifyRefreshToken",
        "spotifyExpiresAt",
        "spotifyScope",
    ] {
        settings.remove(key);
    }
    state.storage.patch(
        "agents",
        agent_id,
        json!({ "settings": Value::Object(settings) }),
    )?;
    Ok(json!({ "success": true }))
}

#[derive(Clone)]
struct SpotifyCredentials {
    access_token: String,
    agent_id: String,
    expires_at: u128,
    scopes: Vec<String>,
}

async fn resolve_credentials(
    state: &AppState,
    route: &ParsedPath,
    body: &Value,
) -> AppResult<SpotifyCredentials> {
    let agent = find_spotify_agent(state, string_param(route, body, "agentId").as_deref())?;
    let agent_id = agent
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let settings = agent_settings(&agent);
    let refresh_token = settings
        .get("spotifyRefreshToken")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let client_id = settings
        .get("spotifyClientId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if refresh_token.is_empty() || client_id.is_empty() {
        return Err(AppError::invalid_input(
            "Spotify is not connected. Open the Spotify DJ agent and connect your account.",
        ));
    }
    let mut access_token = settings
        .get("spotifyAccessToken")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let mut expires_at = settings
        .get("spotifyExpiresAt")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u128;
    let mut scopes = scope_list(
        settings
            .get("spotifyScope")
            .and_then(Value::as_str)
            .unwrap_or(""),
    );
    if access_token.is_empty()
        || (expires_at > 0 && now_millis() > expires_at.saturating_sub(60_000))
    {
        let token = refresh_agent_token(state, &agent_id).await?;
        access_token = token
            .get("access_token")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        expires_at = token.get("expiresAt").and_then(Value::as_u64).unwrap_or(0) as u128;
        scopes = scope_list(token.get("scope").and_then(Value::as_str).unwrap_or(""));
    }
    if access_token.is_empty() || (expires_at > 0 && now_millis() > expires_at) {
        return Err(AppError::new(
            "spotify_token_expired",
            "Spotify token expired. Reconnect Spotify and try again.",
        ));
    }
    Ok(SpotifyCredentials {
        access_token,
        agent_id,
        expires_at,
        scopes,
    })
}

fn find_spotify_agent(state: &AppState, preferred_agent_id: Option<&str>) -> AppResult<Value> {
    if let Some(id) = preferred_agent_id.filter(|id| !id.is_empty()) {
        let agent = get_required(state, "agents", id)?;
        if agent.get("type").and_then(Value::as_str) == Some("spotify") || id == "spotify" {
            return Ok(agent);
        }
    }
    find_by_field(state, "agents", "type", "spotify")?
        .ok_or_else(|| AppError::not_found("Spotify DJ agent is not configured."))
}

async fn refresh_agent_token(state: &AppState, agent_id: &str) -> AppResult<Value> {
    let agent = get_required(state, "agents", agent_id)?;
    let settings = agent_settings(&agent);
    let refresh_token = settings
        .get("spotifyRefreshToken")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::invalid_input("No Spotify refresh token configured"))?;
    let client_id = settings
        .get("spotifyClientId")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::invalid_input("No Spotify client ID configured"))?;
    let token = spotify_token_request(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", client_id),
    ])
    .await?;
    save_spotify_tokens(state, agent_id, client_id, &token)?;
    Ok(token)
}

fn save_spotify_tokens(
    state: &AppState,
    agent_id: &str,
    client_id: &str,
    token: &Value,
) -> AppResult<()> {
    let agent = get_required(state, "agents", agent_id)?;
    let mut settings = agent_settings(&agent);
    let access_token = token
        .get("access_token")
        .and_then(Value::as_str)
        .unwrap_or("");
    let refresh_token = token
        .get("refresh_token")
        .and_then(Value::as_str)
        .or_else(|| settings.get("spotifyRefreshToken").and_then(Value::as_str))
        .unwrap_or("")
        .to_string();
    let expires_in = token
        .get("expires_in")
        .and_then(Value::as_u64)
        .unwrap_or(3600);
    let scope = token
        .get("scope")
        .and_then(Value::as_str)
        .or_else(|| settings.get("spotifyScope").and_then(Value::as_str))
        .unwrap_or("")
        .to_string();
    settings.insert(
        "spotifyAccessToken".to_string(),
        Value::String(access_token.to_string()),
    );
    settings.insert(
        "spotifyRefreshToken".to_string(),
        Value::String(refresh_token),
    );
    settings.insert(
        "spotifyExpiresAt".to_string(),
        json!(now_millis() + (expires_in as u128 * 1000)),
    );
    settings.insert(
        "spotifyClientId".to_string(),
        Value::String(client_id.to_string()),
    );
    settings.insert("spotifyScope".to_string(), Value::String(scope));
    state.storage.patch(
        "agents",
        agent_id,
        json!({ "settings": Value::Object(settings) }),
    )?;
    Ok(())
}

async fn spotify_token_request(params: &[(&str, &str)]) -> AppResult<Value> {
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| AppError::new("spotify_client_error", error.to_string()))?
        .post("https://accounts.spotify.com/api/token")
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(form_urlencoded(params))
        .send()
        .await
        .map_err(|error| AppError::new("spotify_network_error", error.to_string()))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    let json = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({ "raw": text }));
    if !status.is_success() {
        return Err(AppError::with_details(
            "spotify_token_error",
            format!("Spotify token request failed with HTTP {status}"),
            json,
        ));
    }
    Ok(json)
}

struct SpotifyResponse {
    status: u16,
    body: String,
    json: Value,
}

async fn spotify_api(
    credentials: &SpotifyCredentials,
    path: &str,
    method: &str,
    body: Option<Value>,
) -> AppResult<SpotifyResponse> {
    let url = format!("https://api.spotify.com/v1{path}");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| AppError::new("spotify_client_error", error.to_string()))?;
    let method = method
        .parse::<reqwest::Method>()
        .map_err(|error| AppError::invalid_input(error.to_string()))?;
    let mut request = client
        .request(method, url)
        .bearer_auth(&credentials.access_token);
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request
        .send()
        .await
        .map_err(|error| AppError::new("spotify_network_error", error.to_string()))?;
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    let json = serde_json::from_str::<Value>(&body).unwrap_or(Value::Null);
    Ok(SpotifyResponse { status, body, json })
}

fn map_playback(data: &Value) -> Value {
    if data.is_null() {
        return json!({ "connected": true, "active": false });
    }
    let item = data.get("item").cloned().unwrap_or(Value::Null);
    let device = data.get("device").cloned().unwrap_or(Value::Null);
    let artists = item
        .get("artists")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|artist| artist.get("name").and_then(Value::as_str))
                .map(|name| Value::String(name.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!({
        "connected": true,
        "active": true,
        "isPlaying": data.get("is_playing").and_then(Value::as_bool).unwrap_or(false),
        "shuffle": data.get("shuffle_state").and_then(Value::as_bool).unwrap_or(false),
        "smartShuffle": data.get("smart_shuffle").and_then(Value::as_bool).unwrap_or(false),
        "repeat": match data.get("repeat_state").and_then(Value::as_str).unwrap_or("off") {
            "track" => "track",
            "context" => "context",
            _ => "off",
        },
        "progressMs": data.get("progress_ms").cloned().unwrap_or(Value::Null),
        "durationMs": item.get("duration_ms").cloned().unwrap_or(Value::Null),
        "item": if item.is_null() { Value::Null } else { json!({
            "id": item.get("id").cloned().unwrap_or(Value::Null),
            "uri": item.get("uri").cloned().unwrap_or(Value::Null),
            "name": item.get("name").and_then(Value::as_str).unwrap_or("Unknown track"),
            "type": item.get("type").and_then(Value::as_str).unwrap_or("track"),
            "artists": artists,
            "album": item.get("album").and_then(|album| album.get("name")).cloned().unwrap_or(Value::Null),
            "imageUrl": item.get("album").and_then(|album| album.get("images")).and_then(Value::as_array).and_then(|images| images.first()).and_then(|image| image.get("url")).cloned().unwrap_or(Value::Null)
        }) },
        "device": if device.is_null() { Value::Null } else { json!({
            "id": device.get("id").cloned().unwrap_or(Value::Null),
            "name": device.get("name").and_then(Value::as_str).unwrap_or("Spotify device"),
            "type": device.get("type").cloned().unwrap_or(Value::Null),
            "volume": device.get("volume_percent").cloned().unwrap_or(Value::Null),
            "isActive": device.get("is_active").and_then(Value::as_bool).unwrap_or(false)
        }) }
    })
}

fn agent_settings(agent: &Value) -> Map<String, Value> {
    match agent.get("settings") {
        Some(Value::Object(object)) => object.clone(),
        Some(Value::String(raw)) => serde_json::from_str::<Value>(raw)
            .ok()
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default(),
        _ => Map::new(),
    }
}

fn string_param(route: &ParsedPath, body: &Value, key: &str) -> Option<String> {
    body.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            route
                .query
                .get(key)
                .filter(|value| !value.trim().is_empty())
                .cloned()
        })
}

fn spotify_code_and_state(body: &Value) -> AppResult<(String, String)> {
    if let (Some(code), Some(state)) = (
        body.get("code")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty()),
        body.get("state")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty()),
    ) {
        return Ok((code.to_string(), state.to_string()));
    }
    let callback_url = body
        .get("callbackUrl")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            AppError::invalid_input(
                "Missing code or state. Paste the full URL Spotify redirected your browser to.",
            )
        })?;
    let query = callback_url
        .split_once('?')
        .map(|(_, query)| query)
        .unwrap_or(callback_url);
    let params = parse_query(query);
    if let Some(error) = params.get("error") {
        return Err(AppError::invalid_input(format!(
            "Spotify returned an error: {error}"
        )));
    }
    let code = params
        .get("code")
        .cloned()
        .ok_or_else(|| AppError::invalid_input("Pasted URL did not include a Spotify code"))?;
    let state = params
        .get("state")
        .cloned()
        .ok_or_else(|| AppError::invalid_input("Pasted URL did not include a Spotify state"))?;
    Ok((code, state))
}

fn scope_list(scope: &str) -> Vec<String> {
    scope.split_whitespace().map(ToOwned::to_owned).collect()
}

fn spotify_control_path(path: &str, device_id: Option<&str>) -> String {
    match device_id.filter(|value| !value.is_empty()) {
        Some(device_id) => {
            let separator = if path.contains('?') { '&' } else { '?' };
            format!(
                "{path}{separator}device_id={}",
                percent_encode_component(device_id)
            )
        }
        None => path.to_string(),
    }
}

fn map_track_candidate(item: Value) -> Option<Value> {
    let uri = item.get("uri").and_then(Value::as_str)?.to_string();
    if !uri.starts_with("spotify:track:") {
        return None;
    }
    let artists = item
        .get("artists")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|artist| artist.get("name").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    Some(json!({
        "uri": uri,
        "name": item.get("name").and_then(Value::as_str).unwrap_or("Unknown track"),
        "artist": artists,
        "album": item.get("album").and_then(|album| album.get("name")).cloned().unwrap_or(Value::Null),
        "imageUrl": item.get("album").and_then(|album| album.get("images")).and_then(Value::as_array).and_then(|images| images.first()).and_then(|image| image.get("url")).cloned().unwrap_or(Value::Null),
        "durationMs": item.get("duration_ms").cloned().unwrap_or(Value::Null),
        "score": Value::Null
    }))
}

fn random_token(length: usize) -> String {
    let raw = new_id().replace('-', "");
    raw.chars().cycle().take(length).collect()
}

fn code_challenge(verifier: &str) -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine as _;
    use sha2::{Digest, Sha256};
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn form_urlencoded(params: &[(&str, &str)]) -> String {
    params
        .iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                percent_encode_component(key),
                percent_encode_component(value)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn parse_query(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            Some((key.to_string(), percent_decode_component(value)))
        })
        .collect()
}

fn percent_decode_component(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                output.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = &value[index + 1..index + 3];
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    output.push(byte);
                    index += 3;
                } else {
                    output.push(bytes[index]);
                    index += 1;
                }
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&output).to_string()
}
