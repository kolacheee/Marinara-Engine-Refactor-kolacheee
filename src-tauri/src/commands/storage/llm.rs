use super::shared::*;
use super::*;
use marinara_security::is_allowed_outbound_url;

pub(crate) fn resolve_llm_connection_for_request(
    state: &AppState,
    body: &Value,
) -> AppResult<Value> {
    if let Some(connection) = body.get("connection").filter(|value| value.is_object()) {
        return Ok(connection.clone());
    }
    if let Some(connection_id) = body
        .get("connectionId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
    {
        return get_required(state, "connections", connection_id);
    }
    if body.get("provider").is_some() && body.get("model").is_some() {
        return Ok(body.clone());
    }
    let connections = state.storage.list("connections")?;
    if let Some(default) = connections
        .iter()
        .find(|connection| {
            connection
                .get("isDefault")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .cloned()
    {
        return Ok(default);
    }
    connections
        .into_iter()
        .next()
        .ok_or_else(|| AppError::invalid_input("No LLM connection is configured"))
}

pub(crate) fn llm_request_from_body(
    state: &AppState,
    body: Value,
) -> AppResult<marinara_llm::LlmRequest> {
    let connection = resolve_llm_connection_for_request(state, &body)?;
    let messages = body
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::invalid_input("messages is required"))?
        .iter()
        .map(|message| {
            Ok(marinara_llm::LlmMessage {
                role: message
                    .get("role")
                    .and_then(Value::as_str)
                    .unwrap_or("user")
                    .to_string(),
                content: message
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .collect::<AppResult<Vec<_>>>()?;
    Ok(marinara_llm::LlmRequest {
        connection: llm_connection_from_value(&connection)?,
        messages,
        parameters: body.get("parameters").cloned().unwrap_or_else(|| json!({})),
    })
}

pub(crate) async fn llm_complete(state: &AppState, body: Value) -> AppResult<Value> {
    let content = marinara_llm::complete(llm_request_from_body(state, body)?).await?;
    Ok(Value::String(content))
}

pub(crate) async fn llm_stream_events(state: &AppState, body: Value) -> AppResult<Vec<Value>> {
    let content = marinara_llm::complete(llm_request_from_body(state, body)?).await?;
    Ok(vec![
        json!({ "type": "start" }),
        json!({ "type": "token", "text": content }),
        json!({ "type": "done" }),
    ])
}

pub(crate) async fn llm_models(state: &AppState, connection_id: Option<&str>) -> AppResult<Value> {
    let connection = connection_id
        .and_then(|id| state.storage.get("connections", id).ok().flatten())
        .or_else(|| state.storage.list("connections").ok().and_then(|rows| rows.into_iter().next()));
    let provider = connection
        .as_ref()
        .and_then(|value| value.get("provider"))
        .and_then(Value::as_str)
        .unwrap_or("openai");
    let mut models = match connection.as_ref() {
        Some(connection) => fetch_provider_models(connection).await.unwrap_or_else(|_| provider_model_catalog(provider)),
        None => provider_model_catalog(provider),
    };
    if let Some(connection) = connection.as_ref() {
        for key in ["model", "embeddingModel", "imageModel"] {
            if let Some(model) = connection
                .get(key)
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
            {
                push_model(&mut models, model, provider);
            }
        }
    }
    Ok(Value::Array(models))
}
pub(crate) fn llm_connection_from_value(value: &Value) -> AppResult<marinara_llm::LlmConnection> {
    let provider = value
        .get("provider")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::invalid_input("Connection provider is required"))?
        .to_string();
    let model = value
        .get("model")
        .and_then(Value::as_str)
        .filter(|model| !model.trim().is_empty())
        .ok_or_else(|| AppError::invalid_input("Connection model is required"))?
        .to_string();
    let api_key = value
        .get("apiKey")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let base_url = value
        .get("baseUrl")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    Ok(marinara_llm::LlmConnection {
        provider,
        model,
        api_key,
        base_url,
    })
}

pub(crate) async fn connection_models(state: &AppState, id: &str) -> AppResult<Value> {
    let models = llm_models(state, Some(id)).await?;
    Ok(json!({ "models": models }))
}

pub(crate) fn diagnose_claude_subscription(state: &AppState, id: &str) -> AppResult<Value> {
    let connection = get_required(state, "connections", id)?;
    let provider = connection
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or("");
    let requested_model = connection.get("model").cloned().unwrap_or(Value::Null);
    let is_claude = provider == "anthropic"
        || requested_model
            .as_str()
            .map(|model| model.to_ascii_lowercase().contains("claude"))
            .unwrap_or(false);
    Ok(json!({
        "success": is_claude,
        "requestedModel": requested_model,
        "modelsBilled": if is_claude { vec![requested_model.clone()] } else { Vec::<Value>::new() },
        "modelUsageDetail": if is_claude {
            vec![json!({ "model": requested_model, "source": "configured-connection" })]
        } else {
            Vec::<Value>::new()
        },
        "message": if is_claude {
            "Claude-compatible connection is configured locally. Provider billing details are available in the Anthropic account console."
        } else {
            "This connection is not configured as an Anthropic/Claude provider."
        }
    }))
}

fn provider_model_catalog(provider: &str) -> Vec<Value> {
    let ids: &[&str] = match provider {
        "anthropic" => &[
            "claude-3-5-sonnet-latest",
            "claude-3-5-haiku-latest",
            "claude-3-opus-latest",
        ],
        "google" | "google_vertex" => &[
            "gemini-1.5-pro",
            "gemini-1.5-flash",
            "text-embedding-004",
        ],
        "openrouter" => &[
            "openai/gpt-4o-mini",
            "anthropic/claude-3.5-sonnet",
            "google/gemini-flash-1.5",
        ],
        "ollama" => &["llama3.1", "mistral", "nomic-embed-text"],
        "xai" => &["grok-2-latest", "grok-2-mini-latest"],
        _ => &["gpt-4o", "gpt-4o-mini", "text-embedding-3-small", "text-embedding-3-large"],
    };
    ids.iter()
        .map(|id| json!({ "id": id, "name": id, "provider": provider }))
        .collect()
}

fn push_model(models: &mut Vec<Value>, id: &str, provider: &str) {
    if models
        .iter()
        .any(|model| model.get("id").and_then(Value::as_str) == Some(id))
    {
        return;
    }
    models.insert(0, json!({ "id": id, "name": id, "provider": provider }));
}

async fn fetch_provider_models(connection: &Value) -> AppResult<Vec<Value>> {
    let provider = connection
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or("openai");
    if matches!(provider, "claude_subscription" | "openai_chatgpt") {
        return Ok(provider_model_catalog(provider));
    }
    if provider == "image_generation" {
        return fetch_image_models(connection).await;
    }
    if provider == "ollama" {
        return fetch_ollama_models(connection).await;
    }
    let base = connection_base_url(connection);
    if base.is_empty() {
        return Ok(provider_model_catalog(provider));
    }
    let url = model_endpoint(provider, &base, connection);
    ensure_model_url_allowed(&url)?;
    let mut request = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| AppError::new("models_client_error", error.to_string()))?
        .get(url)
        .header("accept", "application/json");
    let api_key = connection
        .get("apiKey")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if provider == "anthropic" {
        request = request
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01");
    } else if !api_key.is_empty() && provider != "google" {
        request = request.bearer_auth(api_key);
    }
    let response = request
        .send()
        .await
        .map_err(|error| AppError::new("models_network_error", error.to_string()))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| AppError::new("models_response_error", error.to_string()))?;
    if !status.is_success() {
        return Err(AppError::new(
            "models_provider_error",
            format!("Provider returned HTTP {status}: {}", sanitize_provider_body(&text)),
        ));
    }
    let json = serde_json::from_str::<Value>(&text)
        .map_err(|error| AppError::new("models_json_error", error.to_string()))?;
    Ok(normalize_models_response(provider, &json))
}

async fn fetch_ollama_models(connection: &Value) -> AppResult<Vec<Value>> {
    let base = connection_base_url(connection);
    let url = format!("{base}/api/tags");
    ensure_model_url_allowed(&url)?;
    let json = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| AppError::new("models_client_error", error.to_string()))?
        .get(url)
        .send()
        .await
        .map_err(|error| AppError::new("models_network_error", error.to_string()))?
        .json::<Value>()
        .await
        .map_err(|error| AppError::new("models_json_error", error.to_string()))?;
    Ok(json
        .get("models")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|model| model.get("name").and_then(Value::as_str))
        .map(|id| json!({ "id": id, "name": id, "provider": "ollama" }))
        .collect())
}

async fn fetch_image_models(connection: &Value) -> AppResult<Vec<Value>> {
    let source = image_source(connection);
    let base = connection_base_url(connection);
    if source == "stability" {
        return Ok(vec![
            json!({ "id": "stable-image-core", "name": "Stable Image Core", "provider": "image_generation" }),
            json!({ "id": "stable-image-ultra", "name": "Stable Image Ultra", "provider": "image_generation" }),
            json!({ "id": "sd3.5-large", "name": "Stable Diffusion 3.5 Large", "provider": "image_generation" }),
            json!({ "id": "sd3.5-medium", "name": "Stable Diffusion 3.5 Medium", "provider": "image_generation" }),
        ]);
    }
    if base.is_empty() {
        return Ok(provider_model_catalog("image_generation"));
    }
    match source.as_str() {
        "comfyui" => fetch_json_models(
            &format!("{base}/object_info/CheckpointLoaderSimple"),
            connection,
            "image_generation",
            |json| {
                json.get("CheckpointLoaderSimple")
                    .and_then(|value| value.get("input"))
                    .and_then(|value| value.get("required"))
                    .and_then(|value| value.get("ckpt_name"))
                    .and_then(Value::as_array)
                    .and_then(|items| items.first())
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(|id| json!({ "id": id, "name": id, "provider": "image_generation" }))
                    .collect()
            },
        )
        .await,
        "automatic1111" | "drawthings" => fetch_json_models(
            &format!("{base}/sdapi/v1/sd-models"),
            connection,
            "image_generation",
            |json| {
                json.as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|model| {
                        model
                            .get("title")
                            .or_else(|| model.get("model_name"))
                            .and_then(Value::as_str)
                    })
                    .map(|id| json!({ "id": id, "name": id, "provider": "image_generation" }))
                    .collect()
            },
        )
        .await,
        "horde" => {
            let url = format!("{}/api/v2/status/models?type=image", base.trim_end_matches('/'));
            fetch_json_models(&url, connection, "image_generation", |json| {
                json.as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|model| model.get("name").or_else(|| model.get("id")).and_then(Value::as_str))
                    .map(|id| json!({ "id": id, "name": id, "provider": "image_generation" }))
                    .collect()
            })
            .await
        }
        "nanogpt" => fetch_json_models(&format!("{base}/image-models"), connection, "image_generation", |json| {
            normalize_openai_data_models(json, "image_generation")
        })
        .await,
        "openrouter" => fetch_json_models(
            &format!("{base}/models?output_modalities=image"),
            connection,
            "image_generation",
            |json| normalize_openai_data_models(json, "image_generation"),
        )
        .await,
        _ => Ok(provider_model_catalog("image_generation")),
    }
}

async fn fetch_json_models<F>(
    url: &str,
    connection: &Value,
    provider: &str,
    normalize: F,
) -> AppResult<Vec<Value>>
where
    F: Fn(&Value) -> Vec<Value>,
{
    ensure_model_url_allowed(url)?;
    let mut request = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| AppError::new("models_client_error", error.to_string()))?
        .get(url)
        .header("accept", "application/json");
    if let Some(api_key) = connection
        .get("apiKey")
        .and_then(Value::as_str)
        .filter(|key| !key.trim().is_empty())
    {
        request = request.bearer_auth(api_key.trim());
    }
    let response = request
        .send()
        .await
        .map_err(|error| AppError::new("models_network_error", error.to_string()))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| AppError::new("models_response_error", error.to_string()))?;
    if !status.is_success() {
        return Err(AppError::new(
            "models_provider_error",
            format!("{provider} returned HTTP {status}: {}", sanitize_provider_body(&text)),
        ));
    }
    let json = serde_json::from_str::<Value>(&text)
        .map_err(|error| AppError::new("models_json_error", error.to_string()))?;
    Ok(normalize(&json))
}

fn normalize_models_response(provider: &str, json: &Value) -> Vec<Value> {
    match provider {
        "google" => json
            .get("models")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|model| {
                model
                    .get("supportedGenerationMethods")
                    .and_then(Value::as_array)
                    .is_none_or(|methods| {
                        methods.iter().any(|method| method.as_str() == Some("generateContent"))
                    })
            })
            .filter_map(|model| {
                let id = model
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim_start_matches("models/");
                (!id.is_empty()).then(|| {
                    json!({ "id": id, "name": model.get("displayName").and_then(Value::as_str).unwrap_or(id), "provider": provider })
                })
            })
            .collect(),
        "google_vertex" => json
            .get("publisherModels")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|model| {
                let id = model
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .rsplit("/models/")
                    .next()
                    .unwrap_or("");
                (!id.is_empty()).then(|| {
                    json!({ "id": id, "name": model.get("displayName").and_then(Value::as_str).unwrap_or(id), "provider": provider })
                })
            })
            .collect(),
        "anthropic" => json
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|model| model_id(model).map(|id| (id, model)))
            .map(|(id, model)| {
                json!({ "id": id, "name": model.get("display_name").and_then(Value::as_str).unwrap_or(id), "provider": provider })
            })
            .collect(),
        "cohere" => {
            let data_models = normalize_openai_data_models(json, provider);
            if !data_models.is_empty() {
                return data_models;
            }
            json.get("models")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|model| {
                    model
                        .get("endpoints")
                        .and_then(Value::as_array)
                        .is_none_or(|items| items.iter().any(|item| item.as_str() == Some("chat")))
                })
                .filter_map(|model| model.get("name").and_then(Value::as_str))
                .map(|id| json!({ "id": id, "name": id, "provider": provider }))
                .collect()
        }
        _ => normalize_openai_data_models(json, provider),
    }
}

fn normalize_openai_data_models(json: &Value, provider: &str) -> Vec<Value> {
    json.get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|model| model_id(model).map(|id| (id, model)))
        .map(|(id, model)| {
            json!({ "id": id, "name": model.get("name").and_then(Value::as_str).unwrap_or(id), "provider": provider })
        })
        .collect()
}

fn model_id(model: &Value) -> Option<&str> {
    model
        .get("id")
        .or_else(|| model.get("name"))
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
}

fn model_endpoint(provider: &str, base: &str, connection: &Value) -> String {
    let base = base.trim_end_matches('/');
    match provider {
        "anthropic" => format!("{base}/v1/models"),
        "google" if base.ends_with("/v1beta") || base.ends_with("/v1") => {
            format!("{base}/models?key={}", connection.get("apiKey").and_then(Value::as_str).unwrap_or(""))
        }
        "google" => format!(
            "{base}/v1beta/models?key={}",
            connection.get("apiKey").and_then(Value::as_str).unwrap_or("")
        ),
        "google_vertex" => format!("{base}/models"),
        _ => format!("{base}/models"),
    }
}

fn connection_base_url(connection: &Value) -> String {
    let provider = connection
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or("openai");
    connection
        .get("baseUrl")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| provider_default_base_url(provider))
        .trim_end_matches('/')
        .to_string()
}

fn provider_default_base_url(provider: &str) -> &'static str {
    match provider {
        "anthropic" => "https://api.anthropic.com",
        "google" | "google_vertex" => "https://generativelanguage.googleapis.com",
        "openrouter" => "https://openrouter.ai/api/v1",
        "xai" => "https://api.x.ai/v1",
        "ollama" => "http://127.0.0.1:11434",
        "mistral" => "https://api.mistral.ai/v1",
        "cohere" => "https://api.cohere.ai/v2",
        "togetherai" => "https://api.together.xyz/v1",
        _ => "https://api.openai.com/v1",
    }
}

fn image_source(connection: &Value) -> String {
    connection
        .get("imageGenerationSource")
        .or_else(|| connection.get("imageService"))
        .or_else(|| connection.get("model"))
        .and_then(Value::as_str)
        .unwrap_or("pollinations")
        .trim()
        .to_ascii_lowercase()
}

fn ensure_model_url_allowed(url: &str) -> AppResult<()> {
    if is_allowed_outbound_url(url, true) {
        Ok(())
    } else {
        Err(AppError::invalid_input(format!(
            "Outbound model URL is not allowed: {url}"
        )))
    }
}

fn sanitize_provider_body(body: &str) -> String {
    if body.contains("<html") || body.contains("<!DOCTYPE") {
        "Provider returned HTML instead of JSON".to_string()
    } else {
        body.chars().take(300).collect()
    }
}
