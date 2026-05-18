use super::shared::*;
use super::*;
use marinara_security::is_allowed_outbound_url;

pub(crate) fn custom_tool_capabilities() -> Value {
    json!({
        "staticResults": true,
        "webhooks": true,
        "scriptExecutionEnabled": false
    })
}

pub(crate) async fn execute_custom_tool(state: &AppState, body: Value) -> AppResult<Value> {
    let tool_name = required_string(&body, "toolName")?;
    let arguments = body.get("arguments").cloned().unwrap_or_else(|| json!({}));
    let tool = state
        .storage
        .list("custom-tools")?
        .into_iter()
        .find(|row| {
            row.get("name").and_then(Value::as_str) == Some(tool_name)
                && string_bool(row.get("enabled")).unwrap_or(true)
        })
        .ok_or_else(|| {
            AppError::invalid_input(format!("Custom tool not found or disabled: {tool_name}"))
        })?;

    match tool
        .get("executionType")
        .and_then(Value::as_str)
        .unwrap_or("static")
    {
        "static" => Ok(json!({
            "success": true,
            "result": tool
                .get("staticResult")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| AppError::invalid_input(format!("Static result is missing for custom tool: {tool_name}")))?
        })),
        "webhook" => execute_webhook_tool(&tool, tool_name, arguments).await,
        other => Err(AppError::invalid_input(format!(
            "Unsupported custom tool execution type: {other}"
        ))),
    }
}

fn string_bool(value: Option<&Value>) -> Option<bool> {
    match value {
        Some(Value::Bool(value)) => Some(*value),
        Some(Value::String(value)) => match value.as_str() {
            "true" | "1" => Some(true),
            "false" | "0" => Some(false),
            _ => None,
        },
        Some(Value::Number(value)) => value.as_i64().map(|value| value != 0),
        _ => None,
    }
}

async fn execute_webhook_tool(tool: &Value, tool_name: &str, arguments: Value) -> AppResult<Value> {
    let url = tool
        .get("webhookUrl")
        .and_then(Value::as_str)
        .filter(|url| !url.trim().is_empty())
        .ok_or_else(|| {
            AppError::invalid_input(format!(
                "Webhook URL is missing for custom tool: {tool_name}"
            ))
        })?;
    if !is_allowed_outbound_url(url, true) {
        return Err(AppError::invalid_input(format!(
            "Custom tool webhook URL is not allowed: {url}"
        )));
    }

    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| AppError::new("custom_tool_client_error", error.to_string()))?
        .post(url)
        .json(&json!({ "tool": tool_name, "arguments": arguments }))
        .send()
        .await
        .map_err(|error| AppError::new("custom_tool_webhook_error", error.to_string()))?;

    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| AppError::new("custom_tool_response_error", error.to_string()))?;
    if !status.is_success() {
        return Err(AppError::with_details(
            "custom_tool_webhook_failed",
            format!("Custom tool webhook returned HTTP {status}"),
            json!({ "body": text.chars().take(1000).collect::<String>() }),
        ));
    }

    Ok(json!({
        "success": true,
        "result": text
    }))
}
