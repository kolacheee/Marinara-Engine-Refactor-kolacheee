use crate::http_dispatch::{dispatch, InvokeRequest};
use crate::state::AppState;
use crate::storage_commands::llm;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::{engine::general_purpose, Engine as _};
use marinara_core::AppError;
use serde::Deserialize;
use serde_json::{json, Value};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tower_http::cors::CorsLayer;

#[derive(Clone)]
pub struct HttpState {
    app: AppState,
}

#[derive(Clone, Debug)]
pub struct ServerSecurityConfig {
    auth: ServerAuthConfig,
    allowed_origins: Vec<HeaderValue>,
}

#[derive(Clone, Debug)]
enum ServerAuthConfig {
    None,
    Basic { username: String, password: String },
    Bearer { token: String },
}

impl ServerSecurityConfig {
    pub fn from_env() -> Result<Self, String> {
        let auth = auth_from_env()?;
        let allowed_origins = std::env::var("MARINARA_SERVER_ALLOWED_ORIGINS")
            .ok()
            .map(|raw| parse_allowed_origins(&raw))
            .transpose()?
            .unwrap_or_else(default_allowed_origins);
        Ok(Self { auth, allowed_origins })
    }

    pub fn is_auth_enabled(&self) -> bool {
        !matches!(self.auth, ServerAuthConfig::None)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmStreamRequest {
    stream_id: String,
    request: Value,
}

pub async fn serve(
    state: AppState,
    addr: SocketAddr,
    security: ServerSecurityConfig,
) -> Result<(), std::io::Error> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router(state, security)).await
}

pub fn router(state: AppState, security: ServerSecurityConfig) -> Router {
    let protected_state = HttpState { app: state };
    let protected_routes = Router::new()
        .route("/api/invoke", post(invoke))
        .route("/api/llm/stream", post(llm_stream))
        .route("/api/llm/stream/:stream_id/cancel", post(llm_stream_cancel))
        .route_layer(middleware::from_fn_with_state(
            security.clone(),
            require_auth,
        ))
        .with_state(protected_state);

    Router::new()
        .route("/health", get(health))
        .merge(protected_routes)
        .layer(
            CorsLayer::new()
                .allow_origin(security.allowed_origins)
                .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
                .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT]),
        )
}

async fn health() -> Json<Value> {
    Json(json!({ "ok": true, "runtime": "marinara-server" }))
}

async fn require_auth(
    State(security): State<ServerSecurityConfig>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if is_authorized(request.headers(), &security.auth) {
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

fn is_authorized(headers: &HeaderMap, auth: &ServerAuthConfig) -> bool {
    let ServerAuthConfig::Basic { username, password } = auth else {
        if let ServerAuthConfig::Bearer { token } = auth {
            return headers
                .get(header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.strip_prefix("Bearer "))
                .is_some_and(|value| value == token);
        }
        return true;
    };
    let Some(value) = headers.get(header::AUTHORIZATION).and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let Some(encoded) = value.strip_prefix("Basic ") else {
        return false;
    };
    let Ok(decoded) = general_purpose::STANDARD.decode(encoded.trim()) else {
        return false;
    };
    let Ok(credentials) = String::from_utf8(decoded) else {
        return false;
    };
    credentials
        .split_once(':')
        .is_some_and(|(actual_username, actual_password)| {
            actual_username == username && actual_password == password
        })
}

fn auth_from_env() -> Result<ServerAuthConfig, String> {
    let basic = std::env::var("MARINARA_SERVER_BASIC_AUTH")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let api_key = std::env::var("MARINARA_SERVER_API_KEY")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(raw) = basic {
        let (username, password) = raw
            .split_once(':')
            .ok_or_else(|| "MARINARA_SERVER_BASIC_AUTH must be formatted as username:password".to_string())?;
        let username = username.trim().to_string();
        let password = password.trim().to_string();
        if username.is_empty() || password.is_empty() {
            return Err("MARINARA_SERVER_BASIC_AUTH username and password are required".to_string());
        }
        return Ok(ServerAuthConfig::Basic { username, password });
    }
    if let Some(token) = api_key {
        return Ok(ServerAuthConfig::Bearer { token });
    }
    Ok(ServerAuthConfig::None)
}

fn parse_allowed_origins(raw: &str) -> Result<Vec<HeaderValue>, String> {
    raw.split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .map(|origin| {
            HeaderValue::from_str(origin)
                .map_err(|error| format!("Invalid MARINARA_SERVER_ALLOWED_ORIGINS entry {origin}: {error}"))
        })
        .collect()
}

fn default_allowed_origins() -> Vec<HeaderValue> {
    [
        "http://localhost:1420",
        "http://127.0.0.1:1420",
        "tauri://localhost",
        "http://tauri.localhost",
    ]
    .into_iter()
    .filter_map(|origin| HeaderValue::from_str(origin).ok())
    .collect()
}

async fn invoke(
    State(state): State<HttpState>,
    Json(request): Json<InvokeRequest>,
) -> Result<Json<Value>, HttpError> {
    let command = request.command.clone();
    let started = Instant::now();
    println!("invoke {command} started");
    match dispatch(&state.app, request).await {
        Ok(value) => {
            println!("invoke {command} ok in {}ms", started.elapsed().as_millis());
            Ok(Json(value))
        }
        Err(error) => {
            println!(
                "invoke {command} error code={} message={} in {}ms",
                error.code,
                error.message,
                started.elapsed().as_millis()
            );
            Err(error.into())
        }
    }
}

async fn llm_stream(
    State(state): State<HttpState>,
    Json(body): Json<LlmStreamRequest>,
) -> Sse<UnboundedReceiverStream<Result<Event, Infallible>>> {
    let (tx, rx) = mpsc::unbounded_channel::<Result<Event, Infallible>>();
    tokio::spawn(async move {
        let stream_id = body.stream_id.clone();
        let started = Instant::now();
        println!("llm_stream {stream_id} started");
        let result = llm::llm_stream_events(&state.app, body.stream_id, body.request, |event| {
            let data = serde_json::to_string(&event)?;
            tx.send(Ok(Event::default().data(data)))
                .map_err(|error| AppError::new("sse_stream_error", error.to_string()))
        })
        .await;

        match result {
            Ok(()) => {
                println!(
                    "llm_stream {stream_id} ok in {}ms",
                    started.elapsed().as_millis()
                );
            }
            Err(error) => {
                println!(
                    "llm_stream {stream_id} error code={} message={} in {}ms",
                    error.code,
                    error.message,
                    started.elapsed().as_millis()
                );
                let payload = json!({
                    "type": "error",
                    "code": error.code,
                    "message": error.message,
                    "data": error.details,
                });
                let _ = tx.send(Ok(Event::default().data(payload.to_string())));
            }
        }
    });

    Sse::new(UnboundedReceiverStream::new(rx)).keep_alive(KeepAlive::default())
}

async fn llm_stream_cancel(
    State(state): State<HttpState>,
    Path(stream_id): Path<String>,
) -> Result<Json<Value>, HttpError> {
    let started = Instant::now();
    println!("llm_stream_cancel {stream_id} started");
    match llm::llm_stream_cancel(&state.app, &stream_id) {
        Ok(value) => {
            println!(
                "llm_stream_cancel {stream_id} ok in {}ms",
                started.elapsed().as_millis()
            );
            Ok(Json(value))
        }
        Err(error) => {
            println!(
                "llm_stream_cancel {stream_id} error code={} message={} in {}ms",
                error.code,
                error.message,
                started.elapsed().as_millis()
            );
            Err(error.into())
        }
    }
}

struct HttpError(AppError);

impl From<AppError> for HttpError {
    fn from(value: AppError) -> Self {
        Self(value)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let status = match self.0.code.as_str() {
            "not_found" => StatusCode::NOT_FOUND,
            "invalid_input" => StatusCode::BAD_REQUEST,
            "unsupported_command" => StatusCode::NOT_IMPLEMENTED,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let payload = json!({
            "code": self.0.code,
            "message": self.0.message,
            "details": self.0.details,
        });
        (status, Json(payload)).into_response()
    }
}
