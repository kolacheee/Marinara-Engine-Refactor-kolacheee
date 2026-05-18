use super::shared::*;
use super::*;
use std::path::{Component, Path, PathBuf};

const PROFILE_COLLECTIONS: &[&str] = &[
    "characters",
    "character-groups",
    "character-versions",
    "personas",
    "persona-groups",
    "lorebooks",
    "lorebook-entries",
    "lorebook-folders",
    "prompts",
    "prompt-groups",
    "prompt-sections",
    "prompt-variables",
    "chat-presets",
    "agents",
    "agent-runs",
    "agent-memory",
    "themes",
    "extensions",
    "connections",
    "connection-folders",
    "chats",
    "chat-folders",
    "messages",
    "custom-tools",
    "regex-scripts",
    "app-settings",
    "gallery",
    "character-gallery",
    "background-metadata",
    "sprites",
    "knowledge-sources",
    "game-state-snapshots",
    "game-checkpoints",
];

const PROFILE_ASSET_DIRS: &[&str] = &[
    "avatars",
    "sprites",
    "backgrounds",
    "game-assets",
    "fonts",
    "knowledge-sources",
    "lorebooks/images",
];

pub(crate) fn profile_snapshot(state: &AppState) -> AppResult<Value> {
    Ok(json!({
        "type": "marinara_profile",
        "version": 1,
        "exportedAt": now_iso(),
        "runtime": "tauri",
        "data": {
            "collections": profile_collections(state)?,
            "assets": profile_assets(state)?,
        }
    }))
}

pub(crate) fn profile_call(
    state: &AppState,
    method: &str,
    rest: &[&str],
    route: &ParsedPath,
    body: Value,
) -> AppResult<Value> {
    match (method, rest) {
        ("GET", ["export"]) => export_profile(state, route.query.get("format").map(String::as_str)),
        ("POST", ["import"]) => import_profile(state, body),
        _ => Err(AppError::new(
            "route_not_found",
            format!("Unknown profile route: {method} /{}", rest.join("/")),
        )),
    }
}

fn export_profile(state: &AppState, format: Option<&str>) -> AppResult<Value> {
    match format {
        Some("native") | None => profile_snapshot(state),
        Some(_) => Err(AppError::invalid_input(
            "Only native Marinara profile JSON export is supported.",
        )),
    }
}

fn import_profile(state: &AppState, body: Value) -> AppResult<Value> {
    let data = body
        .get("data")
        .and_then(Value::as_object)
        .filter(|_| body.get("type").and_then(Value::as_str) == Some("marinara_profile"))
        .ok_or_else(|| AppError::invalid_input("Invalid Marinara profile export"))?;
    let collections = data
        .get("collections")
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::invalid_input("Native profile export must contain data.collections"))?;
    import_profile_collections(state, data, collections)
}

fn import_profile_collections(
    state: &AppState,
    data: &Map<String, Value>,
    collections: &Map<String, Value>,
) -> AppResult<Value> {
    let mut imported = Map::new();
    for collection in PROFILE_COLLECTIONS {
        let rows = collections
            .get(*collection)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        state.storage.replace_all(collection, rows.clone())?;
        imported.insert((*collection).to_string(), json!(rows.len()));
    }
    let restored_assets = restore_profile_assets(state, data.get("assets"))?;
    imported.insert("files".to_string(), json!(restored_assets));
    Ok(json!({ "success": true, "imported": imported }))
}

fn profile_collections(state: &AppState) -> AppResult<Map<String, Value>> {
    let mut collections = Map::new();
    for collection in PROFILE_COLLECTIONS {
        collections.insert((*collection).to_string(), Value::Array(state.storage.list(collection)?));
    }
    Ok(collections)
}

fn profile_assets(state: &AppState) -> AppResult<Vec<Value>> {
    let mut assets = Vec::new();
    for dir in PROFILE_ASSET_DIRS {
        let relative = Path::new(dir);
        collect_profile_assets(&state.data_dir, relative, &mut assets)?;
    }
    Ok(assets)
}

fn collect_profile_assets(root: &Path, relative: &Path, assets: &mut Vec<Value>) -> AppResult<()> {
    let dir = root.join(relative);
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(&dir)? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let next_relative = relative.join(name);
        if path.is_dir() {
            collect_profile_assets(root, &next_relative, assets)?;
        } else if path.is_file() {
            assets.push(json!({
                "path": profile_relative_path(&next_relative),
                "base64": general_purpose::STANDARD.encode(fs::read(path)?),
            }));
        }
    }
    Ok(())
}

fn restore_profile_assets(state: &AppState, raw_assets: Option<&Value>) -> AppResult<usize> {
    let Some(assets) = raw_assets.and_then(Value::as_array) else {
        return Ok(0);
    };
    let mut restored = 0usize;
    for asset in assets {
        let Some(path) = asset.get("path").and_then(Value::as_str) else {
            continue;
        };
        let Some(base64) = asset.get("base64").and_then(Value::as_str) else {
            continue;
        };
        let relative = safe_profile_asset_path(path)?;
        let bytes = general_purpose::STANDARD
            .decode(base64.trim())
            .map_err(|error| AppError::invalid_input(format!("Invalid profile asset data: {error}")))?;
        let target = state.data_dir.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(target, bytes)?;
        restored += 1;
    }
    Ok(restored)
}

fn safe_profile_asset_path(value: &str) -> AppResult<PathBuf> {
    let normalized = value.replace('\\', "/");
    let path = Path::new(&normalized);
    if path.is_absolute() {
        return Err(AppError::invalid_input("Invalid profile asset path"));
    }
    let mut output = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(segment) => {
                let segment = segment
                    .to_str()
                    .ok_or_else(|| AppError::invalid_input("Invalid profile asset path"))?;
                if segment.is_empty() || segment.starts_with('.') {
                    return Err(AppError::invalid_input("Invalid profile asset path"));
                }
                output.push(segment);
            }
            _ => return Err(AppError::invalid_input("Invalid profile asset path")),
        }
    }
    if output.as_os_str().is_empty()
        || !PROFILE_ASSET_DIRS
            .iter()
            .any(|allowed| output.starts_with(Path::new(allowed)))
    {
        return Err(AppError::invalid_input("Invalid profile asset path"));
    }
    Ok(output)
}

fn profile_relative_path(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str().map(ToOwned::to_owned),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}
