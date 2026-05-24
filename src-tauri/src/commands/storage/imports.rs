use super::*;

#[path = "imports/service.rs"]
mod service;

pub(crate) fn import_call(state: &AppState, rest: &[&str], body: Value) -> AppResult<Value> {
    service::import_call(state, rest, body)
}

pub(crate) fn import_stream_channel(
    state: &AppState,
    rest: &[&str],
    body: Value,
    on_event: tauri::ipc::Channel<Value>,
) -> AppResult<()> {
    service::import_stream_channel(state, rest, body, on_event)
}
