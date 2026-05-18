use super::http::{http_binary, http_json};
use super::images::percent_encode_component;
use super::shared::*;
use super::*;

pub(crate) async fn bot_browser_call(
    state: &AppState,
    method: &str,
    rest: &[&str],
    route: &ParsedPath,
    body: Value,
) -> AppResult<Value> {
    match (method, rest) {
        ("GET", ["chub", "search"]) => {
            let q = route.query.get("q").cloned().unwrap_or_default();
            let page = route.query.get("page").map(String::as_str).unwrap_or("1");
            let sort = route
                .query
                .get("sort")
                .map(String::as_str)
                .unwrap_or("download_count");
            let nsfw = route
                .query
                .get("nsfw")
                .map(String::as_str)
                .unwrap_or("true");
            let mut params = vec![
                ("search".to_string(), q),
                ("first".to_string(), "48".to_string()),
                ("page".to_string(), page.to_string()),
                ("nsfw".to_string(), nsfw.to_string()),
                ("nsfl".to_string(), nsfw.to_string()),
                ("include_forks".to_string(), "true".to_string()),
                ("venus".to_string(), "false".to_string()),
                (
                    "min_tokens".to_string(),
                    route
                        .query
                        .get("min_tokens")
                        .cloned()
                        .unwrap_or_else(|| "50".to_string()),
                ),
            ];
            if sort != "default" {
                params.push(("sort".to_string(), sort.to_string()));
            }
            for key in [
                "asc",
                "max_days_ago",
                "special_mode",
                "username",
                "max_tokens",
                "tags",
                "excludeTags",
            ] {
                if let Some(value) = route.query.get(key) {
                    let upstream = match key {
                        "tags" => "topics",
                        "excludeTags" => "excludetopics",
                        _ => key,
                    };
                    params.push((upstream.to_string(), value.clone()));
                }
            }
            for key in [
                "require_images",
                "require_lore",
                "require_expressions",
                "require_alternate_greetings",
            ] {
                if route.query.get(key).map(String::as_str) == Some("true") {
                    params.push((key.to_string(), "true".to_string()));
                }
            }
            let query = params
                .iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect::<Vec<_>>()
                .join("&");
            http_json(&format!("https://api.chub.ai/search?{query}")).await
        }
        ("GET", ["chub", "character", path @ ..]) if !path.is_empty() => {
            let full_path = path.join("/");
            http_json(&format!(
                "https://api.chub.ai/api/characters/{}?full=true&nocache={}",
                full_path,
                now_millis()
            ))
            .await
        }
        ("GET", ["chub", "avatar", path @ ..]) if !path.is_empty() => {
            let full_path = path.join("/");
            match http_binary(
                &format!("https://avatars.charhub.io/avatars/{full_path}/avatar.webp"),
                "image/webp",
            )
            .await
            {
                Ok(value) => Ok(value),
                Err(_) => {
                    http_binary(
                        &format!(
                            "https://avatars.charhub.io/avatars/{full_path}/chara_card_v2.png"
                        ),
                        "image/png",
                    )
                    .await
                }
            }
        }
        ("GET", ["chub", "download", path @ ..]) if !path.is_empty() => {
            let full_path = path.join("/");
            http_binary(
                &format!("https://avatars.charhub.io/avatars/{full_path}/chara_card_v2.png"),
                "image/png",
            )
            .await
        }
        ("GET", ["janny", "token"]) => Ok(json!({ "token": JANNY_FALLBACK_TOKEN })),
        ("POST", ["janny", "search"]) => {
            let token = body
                .get("token")
                .and_then(Value::as_str)
                .unwrap_or(JANNY_FALLBACK_TOKEN);
            let payload = body.get("payload").cloned().unwrap_or_else(|| body.clone());
            json_post(
                "https://search.jannyai.com/multi-search",
                &payload,
                &[
                    ("Accept", "*/*"),
                    ("Content-Type", "application/json"),
                    ("Authorization", &format!("Bearer {token}")),
                    ("Origin", "https://jannyai.com"),
                    ("Referer", "https://jannyai.com/"),
                    ("x-meilisearch-client", "Meilisearch instant-meilisearch (v0.19.0) ; Meilisearch JavaScript (v0.41.0)"),
                ],
            )
            .await
        }
        ("GET", ["janny", "character", char_id]) => {
            let slug = route
                .query
                .get("slug")
                .map(String::as_str)
                .unwrap_or("character");
            janny_character(char_id, slug).await
        }
        ("GET", ["janny", "avatar", path @ ..]) if !path.is_empty() => {
            http_binary(
                &format!("https://image.jannyai.com/bot-avatars/{}", path.join("/")),
                "image/webp",
            )
            .await
        }
        ("GET", ["chartavern", "search"]) => {
            json_get_owned(
                &format!(
                    "https://character-tavern.com/api/search/cards?{}",
                    query_string(&[
                        ("query", route.query.get("q").map(String::as_str).unwrap_or("")),
                        ("sort", route.query.get("sort").map(String::as_str).unwrap_or("most_popular")),
                        ("page", route.query.get("page").map(String::as_str).unwrap_or("1")),
                        ("limit", route.query.get("limit").map(String::as_str).unwrap_or("60")),
                        ("tags", route.query.get("tags").map(String::as_str).unwrap_or("")),
                        ("exclude_tags", route.query.get("excludeTags").map(String::as_str).unwrap_or("")),
                        ("minimum_tokens", route.query.get("min_tokens").map(String::as_str).unwrap_or("")),
                        ("maximum_tokens", route.query.get("max_tokens").map(String::as_str).unwrap_or("")),
                    ])
                ),
                ct_headers(state)?,
            )
            .await
        }
        ("GET", ["chartavern", "character", author, slug]) => {
            json_get_owned(
                &format!(
                    "https://character-tavern.com/api/character/{}/{}",
                    percent_encode_component(author),
                    percent_encode_component(slug)
                ),
                ct_headers(state)?,
            )
            .await
        }
        ("GET", ["chartavern", "download", path @ ..]) if !path.is_empty() => {
            http_binary(
                &format!("https://cards.character-tavern.com/{}.png", path.join("/")),
                "image/png",
            )
            .await
        }
        ("GET", ["chartavern", "avatar", path @ ..]) if !path.is_empty() => {
            let path = path.join("/");
            match http_binary(
                &format!("https://cards.character-tavern.com/cdn-cgi/image/format=auto,width=320,quality=85/{path}.png"),
                "image/png",
            )
            .await
            {
                Ok(value) => Ok(value),
                Err(_) => {
                    http_binary(
                        &format!("https://cards.character-tavern.com/{path}.png"),
                        "image/png",
                    )
                    .await
                }
            }
        }
        ("GET", ["chartavern", "top-tags"]) => {
            json_get("https://character-tavern.com/api/catalog/top-tags", &[]).await
        }
        ("GET", ["wyvern", "search"]) => {
            let mut params = vec![
                ("limit", route.query.get("limit").map(String::as_str).unwrap_or("48")),
                ("page", route.query.get("page").map(String::as_str).unwrap_or("1")),
            ];
            if route.query.get("q").map(String::as_str).unwrap_or("").is_empty() {
                params.push(("sort", route.query.get("sort").map(String::as_str).unwrap_or("popular")));
                params.push(("order", route.query.get("order").map(String::as_str).unwrap_or("DESC")));
            } else {
                params.push(("q", route.query.get("q").map(String::as_str).unwrap_or("")));
            }
            if let Some(tags) = route.query.get("tags") {
                params.push(("tags", tags));
            }
            if let Some(rating) = route.query.get("rating") {
                params.push(("rating", rating));
            }
            json_get(
                &format!(
                    "https://api.wyvern.chat/exploreSearch/characters?{}",
                    query_string(&params)
                ),
                &[("Accept", "application/json")],
            )
            .await
        }
        ("GET", ["wyvern", "character", id]) => {
            json_get(
                &format!("https://api.wyvern.chat/characters/{}", percent_encode_component(id)),
                &[("Accept", "application/json")],
            )
            .await
        }
        ("GET", ["wyvern", "avatar", path @ ..]) if !path.is_empty() => {
            let raw = path.join("/");
            let url = if raw.starts_with("http") {
                raw
            } else if raw.contains("imagedelivery.net") {
                format!("https://{raw}")
            } else {
                format!("https://imagedelivery.net/{raw}")
            };
            http_binary(&url, "image/webp").await
        }
        ("GET", ["pygmalion", "search"]) => {
            let message = json!({
                "query": route.query.get("q").map(String::as_str).unwrap_or(""),
                "orderBy": route.query.get("orderBy").map(String::as_str).unwrap_or("downloads"),
                "orderDescending": route.query.get("orderDescending").map(String::as_str).unwrap_or("true") == "true",
                "pageSize": route.query.get("pageSize").and_then(|value| value.parse::<u64>().ok()).unwrap_or(48),
                "page": route.query.get("page").and_then(|value| value.parse::<u64>().ok()).unwrap_or(0)
            });
            let token = provider_secret(state, "bot-browser-pygmalion-token")?;
            if !token.is_empty()
                && route.query.get("includeSensitive").map(String::as_str) == Some("true")
            {
                json_post(
                    "https://server.pygmalion.chat/galatea.v1.PublicCharacterService/CharacterSearch",
                    &message,
                    &[
                        ("Content-Type", "application/json"),
                        ("Accept", "application/json"),
                        ("Authorization", &format!("Bearer {token}")),
                    ],
                )
                .await
            } else {
                json_get(
                    &format!(
                        "https://server.pygmalion.chat/galatea.v1.PublicCharacterService/CharacterSearch?{}",
                        query_string(&[
                            ("connect", "v1"),
                            ("encoding", "json"),
                            ("message", &message.to_string())
                        ])
                    ),
                    &[("Accept", "application/json")],
                )
                .await
            }
        }
        ("GET", ["pygmalion", "character"]) => {
            let id = route
                .query
                .get("id")
                .ok_or_else(|| AppError::invalid_input("Missing character id"))?;
            let message = json!({ "characterMetaId": id });
            let token = provider_secret(state, "bot-browser-pygmalion-token")?;
            if token.is_empty() {
                json_get(
                    &format!(
                        "https://server.pygmalion.chat/galatea.v1.PublicCharacterService/Character?{}",
                        query_string(&[
                            ("connect", "v1"),
                            ("encoding", "json"),
                            ("message", &message.to_string())
                        ])
                    ),
                    &[("Accept", "application/json")],
                )
                .await
            } else {
                json_post(
                    "https://server.pygmalion.chat/galatea.v1.PublicCharacterService/Character",
                    &message,
                    &[
                        ("Content-Type", "application/json"),
                        ("Accept", "application/json"),
                        ("Authorization", &format!("Bearer {token}")),
                    ],
                )
                .await
            }
        }
        ("GET", ["pygmalion", "avatar", path @ ..]) if !path.is_empty() => {
            let raw = path.join("/");
            let url = if raw.starts_with("http") {
                raw
            } else {
                format!("https://assets.pygmalion.chat/{raw}")
            };
            http_binary(&url, "image/webp").await
        }
        ("GET", ["datacat", "recent"]) => {
            json_get(
                &format!(
                    "https://datacat.run/api/characters/recent-public?{}",
                    query_string(&[
                        ("limit", route.query.get("limit").map(String::as_str).unwrap_or("80")),
                        ("offset", route.query.get("offset").map(String::as_str).unwrap_or("0")),
                        ("summary", "1"),
                        ("minTotalTokens", route.query.get("min_tokens").map(String::as_str).unwrap_or("889")),
                        ("tagIds", route.query.get("tagIds").map(String::as_str).unwrap_or("")),
                        ("search", route.query.get("q").map(String::as_str).unwrap_or(""))
                    ])
                ),
                &datacat_headers(),
            )
            .await
        }
        ("GET", ["datacat", "fresh"]) => {
            json_get(
                &format!(
                    "https://datacat.run/api/characters/fresh?{}",
                    query_string(&[
                        ("summary", "1"),
                        ("sortBy", route.query.get("sortBy").map(String::as_str).unwrap_or("score")),
                        ("limit24", route.query.get("limit24").map(String::as_str).unwrap_or("80")),
                        ("limitWeek", route.query.get("limitWeek").map(String::as_str).unwrap_or("20"))
                    ])
                ),
                &datacat_headers(),
            )
            .await
        }
        ("GET", ["datacat", "tags"]) => {
            json_get(
                &format!(
                    "https://datacat.run/api/tags/faceted?{}",
                    query_string(&[
                        ("mode", "recent"),
                        ("minTotalTokens", route.query.get("min_tokens").map(String::as_str).unwrap_or("889")),
                        ("activeTagIds", route.query.get("activeTagIds").map(String::as_str).unwrap_or(""))
                    ])
                ),
                &datacat_headers(),
            )
            .await
        }
        ("GET", ["datacat", "character", id]) => {
            json_get(
                &format!("https://datacat.run/api/characters/{}", percent_encode_component(id)),
                &datacat_headers(),
            )
            .await
        }
        ("GET", ["datacat", "download", id]) => {
            json_get(
                &format!(
                    "https://datacat.run/api/characters/{}/download?t={}",
                    percent_encode_component(id),
                    now_millis()
                ),
                &datacat_headers(),
            )
            .await
        }
        ("GET", ["datacat", "avatar", path @ ..]) if !path.is_empty() => {
            let raw = path.join("/");
            let url = if raw.starts_with("https://ella.janitorai.com/") {
                raw
            } else {
                format!("https://ella.janitorai.com/bot-avatars/{raw}")
            };
            http_binary(&url, "image/webp").await
        }
        ("GET", ["pygmalion", "session"]) | ("GET", ["chartavern", "session"]) => {
            let key = if rest.first() == Some(&"pygmalion") {
                "bot-browser-pygmalion-token"
            } else {
                "bot-browser-chartavern-cookie"
            };
            Ok(json!({ "active": !provider_secret(state, key)?.is_empty() }))
        }
        ("POST", ["pygmalion", "set-token"]) => {
            let token = body
                .get("token")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| AppError::invalid_input("token is required"))?;
            state.storage.upsert_with_id(
                "app-settings",
                "bot-browser-pygmalion-token",
                json!({ "value": token }),
            )?;
            Ok(json!({ "ok": true, "stored": true }))
        }
        ("POST", ["chartavern", "set-cookie"]) => {
            let cookie = body
                .get("cookie")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| AppError::invalid_input("cookie is required"))?;
            let value = cookie.strip_prefix("session=").unwrap_or(cookie);
            state.storage.upsert_with_id(
                "app-settings",
                "bot-browser-chartavern-cookie",
                json!({ "value": format!("session={value}") }),
            )?;
            Ok(json!({ "ok": true, "stored": true }))
        }
        ("GET", ["pygmalion", "validate"]) | ("GET", ["chartavern", "validate"]) => {
            if rest.first() == Some(&"pygmalion") {
                validate_pygmalion_token(state).await
            } else {
                validate_chartavern_cookie(state).await
            }
        }
        ("POST", ["pygmalion", "logout"]) => {
            let _ = state.storage.delete("app-settings", "bot-browser-pygmalion-token")?;
            Ok(json!({ "ok": true }))
        }
        ("POST", ["chartavern", "logout"]) => {
            let _ = state.storage.delete("app-settings", "bot-browser-chartavern-cookie")?;
            Ok(json!({ "ok": true }))
        }
        _ => Err(AppError::new(
            "route_not_found",
            format!("Unknown bot-browser route: {method} /{}", rest.join("/")),
        )),
    }
}

const JANNY_FALLBACK_TOKEN: &str =
    "88a6463b66e04fb07ba87ee3db06af337f492ce511d93df6e2d2968cb2ff2b30";

async fn json_get(url: &str, headers: &[(&str, &str)]) -> AppResult<Value> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| AppError::new("http_client_error", error.to_string()))?;
    let mut request = client.get(url).header(
        reqwest::header::USER_AGENT,
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/131 Safari/537.36",
    );
    for (key, value) in headers {
        if !value.is_empty() {
            request = request.header(*key, *value);
        }
    }
    let response = request
        .send()
        .await
        .map_err(|error| AppError::new("upstream_request_failed", error.to_string()))?;
    upstream_json(response).await
}

async fn json_get_owned(url: &str, headers: Vec<(&str, String)>) -> AppResult<Value> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| AppError::new("http_client_error", error.to_string()))?;
    let mut request = client.get(url).header(
        reqwest::header::USER_AGENT,
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/131 Safari/537.36",
    );
    for (key, value) in headers {
        if !value.is_empty() {
            request = request.header(key, value);
        }
    }
    let response = request
        .send()
        .await
        .map_err(|error| AppError::new("upstream_request_failed", error.to_string()))?;
    upstream_json(response).await
}

async fn json_post(url: &str, body: &Value, headers: &[(&str, &str)]) -> AppResult<Value> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| AppError::new("http_client_error", error.to_string()))?;
    let mut request = client.post(url).json(body).header(
        reqwest::header::USER_AGENT,
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/131 Safari/537.36",
    );
    for (key, value) in headers {
        if !value.is_empty() {
            request = request.header(*key, *value);
        }
    }
    let response = request
        .send()
        .await
        .map_err(|error| AppError::new("upstream_request_failed", error.to_string()))?;
    upstream_json(response).await
}

async fn upstream_json(response: reqwest::Response) -> AppResult<Value> {
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(AppError::with_details(
            "upstream_request_failed",
            format!("Upstream returned {status}"),
            json!({ "body": text.chars().take(500).collect::<String>() }),
        ));
    }
    response
        .json::<Value>()
        .await
        .map_err(|error| AppError::new("upstream_json_error", error.to_string()))
}

async fn janny_character(char_id: &str, slug: &str) -> AppResult<Value> {
    let page_url = format!(
        "https://jannyai.com/characters/{}_{}",
        percent_encode_component(char_id),
        percent_encode_component(slug)
    );
    let html = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| AppError::new("http_client_error", error.to_string()))?
        .get(format!("https://corsproxy.io/?url={}", percent_encode_component(&page_url)))
        .send()
        .await
        .map_err(|error| AppError::new("upstream_request_failed", error.to_string()))?
        .text()
        .await
        .map_err(|error| AppError::new("upstream_body_error", error.to_string()))?;
    Ok(json!({ "html": html, "character": parse_janny_character_html(&html) }))
}

fn parse_janny_character_html(html: &str) -> Value {
    let decoded = html
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&#39;", "'");
    json!({
        "description": extract_between(&decoded, "\"description\":\"", "\"").unwrap_or_default(),
        "personality": extract_between(&decoded, "\"personality\":\"", "\"").unwrap_or_default(),
        "scenario": extract_between(&decoded, "\"scenario\":\"", "\"").unwrap_or_default(),
        "firstMessage": extract_between(&decoded, "\"firstMessage\":\"", "\"").unwrap_or_default(),
        "exampleDialogs": extract_between(&decoded, "\"exampleDialogs\":\"", "\"").unwrap_or_default()
    })
}

fn extract_between(haystack: &str, start: &str, end: &str) -> Option<String> {
    let from = haystack.find(start)? + start.len();
    let rest = &haystack[from..];
    let to = rest.find(end)?;
    Some(rest[..to].replace("\\n", "\n").replace("\\\"", "\""))
}

fn query_string(params: &[(&str, &str)]) -> String {
    params
        .iter()
        .filter(|(_, value)| !value.is_empty())
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

fn provider_secret(state: &AppState, key: &str) -> AppResult<String> {
    Ok(state
        .storage
        .get("app-settings", key)?
        .and_then(|record| record.get("value").and_then(Value::as_str).map(ToOwned::to_owned))
        .unwrap_or_default())
}

async fn validate_pygmalion_token(state: &AppState) -> AppResult<Value> {
    let token = provider_secret(state, "bot-browser-pygmalion-token")?;
    if token.is_empty() {
        return Ok(json!({ "valid": false, "reason": "no token stored" }));
    }
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| AppError::new("bot_browser_http_error", error.to_string()))?
        .post("https://server.pygmalion.chat/galatea.v1.PublicCharacterService/CharacterSearch")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("Authorization", format!("Bearer {token}"))
        .header("Origin", "https://pygmalion.chat")
        .header("Referer", "https://pygmalion.chat/")
        .json(&json!({
            "query": "",
            "orderBy": "downloads",
            "orderDescending": true,
            "pageSize": 1,
            "page": 0,
            "includeSensitive": true
        }))
        .send()
        .await
        .map_err(|error| AppError::new("bot_browser_http_error", error.to_string()))?;
    if response.status().is_success() {
        return Ok(json!({ "valid": true }));
    }
    if matches!(response.status().as_u16(), 401 | 403) {
        let _ = state
            .storage
            .delete("app-settings", "bot-browser-pygmalion-token")?;
        return Ok(json!({ "valid": false, "reason": "Token rejected (expired or invalid)" }));
    }
    Ok(json!({ "valid": false, "reason": format!("HTTP {}", response.status()) }))
}

async fn validate_chartavern_cookie(state: &AppState) -> AppResult<Value> {
    let cookie = provider_secret(state, "bot-browser-chartavern-cookie")?;
    if cookie.is_empty() {
        return Ok(json!({ "valid": false, "reason": "no cookies stored" }));
    }
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| AppError::new("bot_browser_http_error", error.to_string()))?
        .get("https://character-tavern.com/api/search/cards?query=test&limit=5")
        .header("Accept", "application/json")
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko)",
        )
        .header("Cookie", &cookie)
        .send()
        .await
        .map_err(|error| AppError::new("bot_browser_http_error", error.to_string()))?;
    if response.status().is_success() {
        let rejected = response
            .headers()
            .get("set-cookie")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.contains("session=;") || value.contains("Max-Age=0"));
        let json: Value = response.json().await.unwrap_or_else(|_| json!({}));
        if rejected {
            let _ = state
                .storage
                .delete("app-settings", "bot-browser-chartavern-cookie")?;
            return Ok(json!({ "valid": false, "reason": "Session rejected/expired by server" }));
        }
        let has_nsfw = json
            .get("hits")
            .and_then(Value::as_array)
            .is_some_and(|hits| {
                hits.iter()
                    .any(|hit| hit.get("isNSFW").and_then(Value::as_bool).unwrap_or(false))
            });
        return Ok(json!({ "valid": true, "hasNsfw": has_nsfw }));
    }
    if response.status().as_u16() == 403 {
        let _ = state
            .storage
            .delete("app-settings", "bot-browser-chartavern-cookie")?;
        return Ok(json!({ "valid": false, "reason": "rejected (cookies expired or invalid)" }));
    }
    Ok(json!({ "valid": false, "reason": format!("HTTP {}", response.status()) }))
}

fn ct_headers(state: &AppState) -> AppResult<Vec<(&'static str, String)>> {
    let cookie = provider_secret(state, "bot-browser-chartavern-cookie")?;
    Ok(vec![
        ("Accept", "application/json".to_string()),
        ("Cookie", cookie),
    ])
}

fn datacat_headers() -> Vec<(&'static str, &'static str)> {
    vec![
        ("Accept", "application/json"),
        ("Origin", "https://datacat.run"),
        ("Referer", "https://datacat.run/"),
    ]
}
