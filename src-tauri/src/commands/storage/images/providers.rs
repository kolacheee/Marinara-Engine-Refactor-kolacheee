use super::*;

const DEFAULT_OPENAI_IMAGE_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_STABILITY_BASE_URL: &str = "https://api.stability.ai/v2beta";
const DEFAULT_TOGETHER_BASE_URL: &str = "https://api.together.xyz/v1";
const DEFAULT_NOVELAI_BASE_URL: &str = "https://image.novelai.net";
const DEFAULT_OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";
const DEFAULT_XAI_BASE_URL: &str = "https://api.x.ai/v1";
const DEFAULT_HORDE_BASE_URL: &str = "https://stablehorde.net/api/v2";
const DEFAULT_AUTOMATIC1111_BASE_URL: &str = "http://localhost:7860";
const DEFAULT_COMFYUI_BASE_URL: &str = "http://127.0.0.1:8188";
const DEFAULT_NANOGPT_BASE_URL: &str = "https://nano-gpt.com/api/v1";

pub(crate) async fn generate_image_with_connection(
    connection: &Value,
    prompt: &str,
    width: u64,
    height: u64,
) -> AppResult<(String, String)> {
    if connection.get("provider").and_then(Value::as_str) != Some("image_generation") {
        return Err(AppError::invalid_input(
            "Selected connection is not an image-generation connection",
        ));
    }
    let source = image_source(connection);
    match source.as_str() {
        "pollinations" => generate_pollinations(connection, prompt, width, height).await,
        "stability" => generate_stability(connection, prompt).await,
        "automatic1111" | "drawthings" => generate_automatic1111(connection, prompt, width, height).await,
        "comfyui" => generate_comfyui(connection, prompt, width, height).await,
        "horde" => generate_horde(connection, prompt, width, height).await,
        "openrouter" | "gemini_image" => generate_chat_image(connection, prompt).await,
        "openai" | "xai" | "togetherai" | "nanogpt" | "blockentropy" | "" => {
            generate_openai_compatible_image(connection, &source, prompt, width, height).await
        }
        other => Err(AppError::invalid_input(format!(
            "Unsupported image generation service: {other}"
        ))),
    }
}

fn image_source(connection: &Value) -> String {
    connection
        .get("imageGenerationSource")
        .or_else(|| connection.get("imageService"))
        .and_then(Value::as_str)
        .or_else(|| connection.get("service").and_then(Value::as_str))
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

fn connection_model(connection: &Value, fallback: &str) -> String {
    connection
        .get("model")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn connection_api_key(connection: &Value) -> String {
    connection
        .get("apiKey")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn connection_base_url(connection: &Value, source: &str) -> String {
    let fallback = match source {
        "stability" => DEFAULT_STABILITY_BASE_URL,
        "togetherai" => DEFAULT_TOGETHER_BASE_URL,
        "novelai" => DEFAULT_NOVELAI_BASE_URL,
        "openrouter" | "gemini_image" => DEFAULT_OPENROUTER_BASE_URL,
        "xai" => DEFAULT_XAI_BASE_URL,
        "horde" => DEFAULT_HORDE_BASE_URL,
        "automatic1111" | "drawthings" => DEFAULT_AUTOMATIC1111_BASE_URL,
        "comfyui" => DEFAULT_COMFYUI_BASE_URL,
        "nanogpt" => DEFAULT_NANOGPT_BASE_URL,
        _ => DEFAULT_OPENAI_IMAGE_BASE_URL,
    };
    connection
        .get("baseUrl")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(fallback)
        .trim_end_matches('/')
        .to_string()
}

fn http_client(timeout_secs: u64) -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .map_err(|error| AppError::new("image_client_error", error.to_string()))
}

fn bearer(request: reqwest::RequestBuilder, api_key: &str) -> reqwest::RequestBuilder {
    if api_key.trim().is_empty() {
        request
    } else {
        request.bearer_auth(api_key)
    }
}

async fn response_json(response: reqwest::Response, provider: &str) -> AppResult<Value> {
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| AppError::new("image_response_error", error.to_string()))?;
    if !status.is_success() {
        return Err(AppError::new(
            "image_provider_error",
            format!("{provider} returned HTTP {status}: {}", sanitize_error(&text)),
        ));
    }
    serde_json::from_str::<Value>(&text).map_err(|error| {
        AppError::new(
            "image_response_error",
            format!("{provider} returned invalid JSON: {error}"),
        )
    })
}

async fn image_response_base64(response: reqwest::Response, provider: &str) -> AppResult<(String, String)> {
    let status = response.status();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("image/png")
        .to_string();
    let bytes = response
        .bytes()
        .await
        .map_err(|error| AppError::new("image_response_error", error.to_string()))?;
    if !status.is_success() {
        let text = String::from_utf8_lossy(&bytes);
        return Err(AppError::new(
            "image_provider_error",
            format!("{provider} returned HTTP {status}: {}", sanitize_error(&text)),
        ));
    }
    Ok((general_purpose::STANDARD.encode(bytes), content_type))
}

fn sanitize_error(text: &str) -> String {
    text.replace(['\n', '\r', '\t'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(300)
        .collect()
}

fn strip_data_url(value: &str) -> (&str, &str) {
    if let Some((meta, base64)) = value.split_once(',') {
        if meta.starts_with("data:") {
            let mime = meta
                .strip_prefix("data:")
                .and_then(|rest| rest.split(';').next())
                .unwrap_or("image/png");
            return (base64, mime);
        }
    }
    (value, "image/png")
}

async fn fetch_image_url(client: &reqwest::Client, url: &str) -> AppResult<(String, String)> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| AppError::new("image_network_error", error.to_string()))?;
    image_response_base64(response, "image URL").await
}

async fn generate_pollinations(
    connection: &Value,
    prompt: &str,
    width: u64,
    height: u64,
) -> AppResult<(String, String)> {
    let base = connection
        .get("baseUrl")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("https://image.pollinations.ai")
        .trim_end_matches('/');
    let encoded_prompt = percent_encode_component(prompt);
    let seed = now_millis() % 1_000_000_000;
    let url = format!("{base}/prompt/{encoded_prompt}?width={width}&height={height}&nologo=true&seed={seed}");
    fetch_image_url(&http_client(120)?, &url).await
}

async fn generate_openai_compatible_image(
    connection: &Value,
    source: &str,
    prompt: &str,
    width: u64,
    height: u64,
) -> AppResult<(String, String)> {
    let source = if source.is_empty() { "openai" } else { source };
    let base = connection_base_url(connection, source);
    let model = connection_model(
        connection,
        match source {
            "xai" => "grok-2-image",
            "togetherai" => "black-forest-labs/FLUX.1-schnell",
            _ => "gpt-image-1",
        },
    );
    let client = http_client(180)?;
    let payload = json!({
        "model": model,
        "prompt": prompt,
        "n": 1,
        "size": format!("{width}x{height}"),
        "response_format": "b64_json"
    });
    let response = bearer(
        client.post(format!("{base}/images/generations")).json(&payload),
        &connection_api_key(connection),
    )
    .send()
    .await
    .map_err(|error| AppError::new("image_network_error", error.to_string()))?;
    let json = response_json(response, source).await?;
    parse_image_json(&client, &json)
        .await
        .ok_or_else(|| AppError::new("image_response_error", format!("{source} returned no image data")))
}

async fn generate_chat_image(connection: &Value, prompt: &str) -> AppResult<(String, String)> {
    let source = image_source(connection);
    let base = connection_base_url(connection, &source);
    let model = connection_model(connection, "google/gemini-2.5-flash-image");
    let client = http_client(180)?;
    let payload = json!({
        "model": model,
        "messages": [{ "role": "user", "content": prompt }],
        "modalities": ["image", "text"]
    });
    let response = bearer(
        client.post(format!("{base}/chat/completions")).json(&payload),
        &connection_api_key(connection),
    )
    .send()
    .await
    .map_err(|error| AppError::new("image_network_error", error.to_string()))?;
    let json = response_json(response, &source).await?;
    parse_image_json(&client, &json)
        .await
        .ok_or_else(|| AppError::new("image_response_error", format!("{source} returned no image data")))
}

async fn parse_image_json(client: &reqwest::Client, json: &Value) -> Option<(String, String)> {
    if let Some(base64) = json
        .pointer("/data/0/b64_json")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        return Some((base64.to_string(), "image/png".to_string()));
    }
    if let Some(url) = json
        .pointer("/data/0/url")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        if url.starts_with("data:image/") {
            let (base64, mime) = strip_data_url(url);
            return Some((base64.to_string(), mime.to_string()));
        }
        return fetch_image_url(client, url).await.ok();
    }
    find_image_string(json).and_then(|value| {
        if value.starts_with("data:image/") {
            let (base64, mime) = strip_data_url(value);
            Some((base64.to_string(), mime.to_string()))
        } else {
            None
        }
    })
}

fn find_image_string(value: &Value) -> Option<&str> {
    match value {
        Value::String(raw) if raw.starts_with("data:image/") => Some(raw),
        Value::Array(items) => items.iter().find_map(find_image_string),
        Value::Object(map) => map.values().find_map(find_image_string),
        _ => None,
    }
}

async fn generate_stability(connection: &Value, prompt: &str) -> AppResult<(String, String)> {
    let base = connection_base_url(connection, "stability");
    let model = connection_model(connection, "stable-image-core");
    let endpoint = if model.contains("ultra") {
        "stable-image/generate/ultra"
    } else if model.contains("core") {
        "stable-image/generate/core"
    } else {
        "stable-image/generate/sd3"
    };
    let mut form = reqwest::multipart::Form::new()
        .text("prompt", prompt.to_string())
        .text("output_format", "png".to_string());
    if endpoint.ends_with("sd3") {
        form = form.text("model", model).text("mode", "text-to-image".to_string());
    }
    let response = bearer(
        http_client(180)?
            .post(format!("{base}/{endpoint}"))
            .header(reqwest::header::ACCEPT, "image/*")
            .multipart(form),
        &connection_api_key(connection),
    )
    .send()
    .await
    .map_err(|error| AppError::new("image_network_error", error.to_string()))?;
    image_response_base64(response, "stability").await
}

async fn generate_automatic1111(
    connection: &Value,
    prompt: &str,
    width: u64,
    height: u64,
) -> AppResult<(String, String)> {
    let base = connection_base_url(connection, "automatic1111");
    let model = connection
        .get("model")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty());
    let mut payload = json!({
        "prompt": prompt,
        "negative_prompt": "",
        "width": width,
        "height": height,
        "steps": 20,
        "cfg_scale": 7,
        "sampler_name": "Euler a"
    });
    if let Some(model) = model {
        payload["override_settings"] = json!({ "sd_model_checkpoint": model });
    }
    let response = http_client(180)?
        .post(format!("{base}/sdapi/v1/txt2img"))
        .json(&payload)
        .send()
        .await
        .map_err(|error| AppError::new("image_network_error", error.to_string()))?;
    let json = response_json(response, "automatic1111").await?;
    let image = json
        .get("images")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::new("image_response_error", "AUTOMATIC1111 returned no image"))?;
    let (base64, mime) = strip_data_url(image);
    Ok((base64.to_string(), mime.to_string()))
}

async fn generate_horde(
    connection: &Value,
    prompt: &str,
    width: u64,
    height: u64,
) -> AppResult<(String, String)> {
    let base = connection_base_url(connection, "horde");
    let api_key = connection_api_key(connection);
    let client = http_client(240)?;
    let mut request = client.post(format!("{base}/generate/async")).json(&json!({
        "prompt": prompt,
        "params": { "width": width, "height": height, "n": 1 },
        "nsfw": true,
        "trusted_workers": false,
        "slow_workers": true
    }));
    request = request.header("apikey", if api_key.trim().is_empty() { "0000000000" } else { &api_key });
    let submit = response_json(
        request
            .send()
            .await
            .map_err(|error| AppError::new("image_network_error", error.to_string()))?,
        "horde",
    )
    .await?;
    let id = submit
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::new("image_response_error", "Stable Horde did not return a request id"))?
        .to_string();
    for _ in 0..120 {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let status = response_json(
            client
                .get(format!("{base}/generate/status/{id}"))
                .header("apikey", if api_key.trim().is_empty() { "0000000000" } else { &api_key })
                .send()
                .await
                .map_err(|error| AppError::new("image_network_error", error.to_string()))?,
            "horde",
        )
        .await?;
        if let Some(img) = status
            .get("generations")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|item| item.get("img"))
            .and_then(Value::as_str)
        {
            if img.starts_with("http://") || img.starts_with("https://") {
                return fetch_image_url(&client, img).await;
            }
            let (base64, mime) = strip_data_url(img);
            return Ok((base64.to_string(), mime.to_string()));
        }
        if status.get("done").and_then(Value::as_bool).unwrap_or(false) {
            break;
        }
    }
    Err(AppError::new(
        "image_timeout",
        "Stable Horde did not finish image generation before the timeout",
    ))
}

async fn generate_comfyui(
    connection: &Value,
    prompt: &str,
    width: u64,
    height: u64,
) -> AppResult<(String, String)> {
    let workflow = connection
        .get("comfyuiWorkflow")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            AppError::invalid_input(
                "ComfyUI image generation requires a workflow JSON on the image connection",
            )
        })?;
    let seed = (now_millis() % 4_294_967_295) as u64;
    let workflow = workflow
        .replace("%prompt%", prompt)
        .replace("%width%", &width.to_string())
        .replace("%height%", &height.to_string())
        .replace("%seed%", &seed.to_string());
    let prompt_json = serde_json::from_str::<Value>(&workflow)
        .map_err(|error| AppError::invalid_input(format!("Invalid ComfyUI workflow JSON: {error}")))?;
    let base = connection_base_url(connection, "comfyui");
    let client = http_client(240)?;
    let response = response_json(
        client
            .post(format!("{base}/prompt"))
            .json(&json!({ "prompt": prompt_json }))
            .send()
            .await
            .map_err(|error| AppError::new("image_network_error", error.to_string()))?,
        "comfyui",
    )
    .await?;
    let prompt_id = response
        .get("prompt_id")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::new("image_response_error", "ComfyUI did not return a prompt id"))?
        .to_string();
    for _ in 0..120 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let history = response_json(
            client
                .get(format!("{base}/history/{prompt_id}"))
                .send()
                .await
                .map_err(|error| AppError::new("image_network_error", error.to_string()))?,
            "comfyui",
        )
        .await?;
        if let Some(image) = find_comfyui_image(&history, &prompt_id) {
            let filename = image.get("filename").and_then(Value::as_str).unwrap_or("");
            let subfolder = image.get("subfolder").and_then(Value::as_str).unwrap_or("");
            let kind = image.get("type").and_then(Value::as_str).unwrap_or("output");
            if !filename.is_empty() {
                let url = format!(
                    "{base}/view?filename={}&subfolder={}&type={}",
                    percent_encode_component(filename),
                    percent_encode_component(subfolder),
                    percent_encode_component(kind)
                );
                return fetch_image_url(&client, &url).await;
            }
        }
    }
    Err(AppError::new(
        "image_timeout",
        "ComfyUI did not finish image generation before the timeout",
    ))
}

fn find_comfyui_image<'a>(history: &'a Value, prompt_id: &str) -> Option<&'a Value> {
    history
        .get(prompt_id)
        .and_then(|value| value.get("outputs"))
        .and_then(Value::as_object)
        .and_then(|outputs| {
            outputs.values().find_map(|output| {
                output
                    .get("images")
                    .and_then(Value::as_array)
                    .and_then(|images| images.first())
            })
        })
}
