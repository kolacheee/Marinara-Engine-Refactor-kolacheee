use super::llm::llm_connection_from_value;
use super::shared::*;
use super::*;

pub(crate) async fn test_connection(state: &AppState, id: &str) -> AppResult<Value> {
    let started = std::time::Instant::now();
    let connection = get_required(state, "connections", id)?;
    let request = marinara_llm::LlmRequest {
        connection: llm_connection_from_value(&connection)?,
        messages: vec![marinara_llm::LlmMessage {
            role: "user".to_string(),
            content: "Reply with exactly: ok".to_string(),
            name: None,
            tool_call_id: None,
            tool_calls: None,
        }],
        parameters: json!({ "maxTokens": 16, "temperature": 0 }),
        tools: Vec::new(),
    };
    let response = marinara_llm::complete(request).await?;
    Ok(json!({
        "success": true,
        "message": response,
        "latencyMs": started.elapsed().as_millis(),
        "modelName": connection.get("model").and_then(Value::as_str)
    }))
}

pub(crate) async fn test_message(state: &AppState, id: &str) -> AppResult<Value> {
    let started = std::time::Instant::now();
    let connection = get_required(state, "connections", id)?;
    let request = marinara_llm::LlmRequest {
        connection: llm_connection_from_value(&connection)?,
        messages: vec![marinara_llm::LlmMessage {
            role: "user".to_string(),
            content: "hi".to_string(),
            name: None,
            tool_call_id: None,
            tool_calls: None,
        }],
        parameters: json!({ "maxTokens": 64, "temperature": 0.7 }),
        tools: Vec::new(),
    };
    let response = marinara_llm::complete(request).await?;
    Ok(json!({
        "success": true,
        "response": response,
        "latencyMs": started.elapsed().as_millis()
    }))
}
