use super::super::images::percent_encode_component;
use crate::state::AppState;
use base64::{engine::general_purpose, Engine as _};
use marinara_core::{AppError, AppResult};
use serde_json::{json, Value};
use std::fs::{self, File};
use std::io::{Read, Seek, Write};
use std::path::{Component, Path, PathBuf};

const PROFILE_ASSET_DIRS: &[&str] = &[
    "avatars",
    "sprites",
    "backgrounds",
    "game-assets",
    "fonts",
    "knowledge-sources",
    "lorebooks/images",
];

const OLD_ASSET_MARKERS: &[&str] = &[
    "/api/avatars/file/",
    "api/avatars/file/",
    "avatars/file/",
    "/api/backgrounds/file/",
    "api/backgrounds/file/",
    "backgrounds/file/",
    "/api/sprites/file/",
    "api/sprites/file/",
    "sprites/file/",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum LegacyProfileAssetKind {
    Avatar,
    Background,
    LorebookImage,
    FileDataUrl,
}

struct LegacyProfileAsset {
    value: String,
    absolute_path: String,
    filename: String,
    kind: LegacyProfileAssetKind,
}

enum ProfileAssetSource {
    Bytes(Vec<u8>),
    ZipEntry(String),
}

struct ProfileAssetRestore {
    relative: PathBuf,
    source: ProfileAssetSource,
}

pub(super) fn profile_assets(state: &AppState) -> AppResult<Vec<Value>> {
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

pub(super) fn restore_profile_assets(
    state: &AppState,
    raw_assets: Option<&Value>,
) -> AppResult<usize> {
    restore_profile_json_assets(state, raw_assets, false)
}

pub(super) fn restore_legacy_profile_json_assets(
    state: &AppState,
    raw_assets: Option<&Value>,
) -> AppResult<usize> {
    restore_profile_json_assets(state, raw_assets, true)
}

fn restore_profile_json_assets(
    state: &AppState,
    raw_assets: Option<&Value>,
    allow_legacy_data_field: bool,
) -> AppResult<usize> {
    restore_profile_json_assets_in_root(&state.data_dir, raw_assets, allow_legacy_data_field)
}

fn restore_profile_json_assets_in_root(
    data_dir: &Path,
    raw_assets: Option<&Value>,
    allow_legacy_data_field: bool,
) -> AppResult<usize> {
    let assets = decoded_profile_json_assets(raw_assets, allow_legacy_data_field)?;
    clear_profile_asset_dirs(data_dir)?;
    let restored = assets.len();
    for (relative, bytes) in assets {
        write_profile_asset_in_root(data_dir, &relative, &bytes)?;
    }
    Ok(restored)
}

fn decoded_profile_json_assets(
    raw_assets: Option<&Value>,
    allow_legacy_data_field: bool,
) -> AppResult<Vec<(PathBuf, Vec<u8>)>> {
    let Some(assets) = raw_assets.and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut decoded = Vec::new();
    for asset in assets {
        let Some(path) = asset.get("path").and_then(Value::as_str) else {
            continue;
        };
        let relative = safe_profile_asset_path(path)?;
        let raw_data = if allow_legacy_data_field {
            asset
                .get("base64")
                .or_else(|| asset.get("data"))
                .and_then(Value::as_str)
        } else {
            asset.get("base64").and_then(Value::as_str)
        };
        let Some(raw_data) = raw_data else {
            continue;
        };
        let bytes = decode_profile_asset_data(raw_data)?;
        decoded.push((relative, bytes));
    }
    Ok(decoded)
}

pub(super) fn restore_profile_zip_assets<R: Read + Seek>(
    state: &AppState,
    archive: &mut zip::ZipArchive<R>,
    names: &[String],
    profile_prefix: &str,
    raw_assets: Option<&Value>,
) -> AppResult<usize> {
    let assets = decoded_profile_zip_assets(raw_assets, names, profile_prefix)?;
    clear_profile_asset_dirs(&state.data_dir)?;
    let restored = assets.len();
    for asset in assets {
        match asset.source {
            ProfileAssetSource::Bytes(bytes) => {
                write_profile_asset_in_root(&state.data_dir, &asset.relative, &bytes)?;
            }
            ProfileAssetSource::ZipEntry(entry_name) => {
                let target = state.data_dir.join(asset.relative);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut entry = archive.by_name(&entry_name).map_err(|error| {
                    AppError::invalid_input(format!(
                        "Could not read profile asset {entry_name}: {error}"
                    ))
                })?;
                let mut output = File::create(target)?;
                std::io::copy(&mut entry, &mut output)?;
                output.flush()?;
            }
        }
    }
    Ok(restored)
}

fn decoded_profile_zip_assets(
    raw_assets: Option<&Value>,
    names: &[String],
    profile_prefix: &str,
) -> AppResult<Vec<ProfileAssetRestore>> {
    let Some(assets) = raw_assets.and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut decoded = Vec::new();
    for asset in assets {
        let Some(path) = asset.get("path").and_then(Value::as_str) else {
            continue;
        };
        let relative = safe_profile_asset_path(path)?;
        let source = if let Some(raw_data) = asset
            .get("base64")
            .or_else(|| asset.get("data"))
            .and_then(Value::as_str)
        {
            ProfileAssetSource::Bytes(decode_profile_asset_data(raw_data)?)
        } else if let Some(entry_name) = zip_asset_entry_name(names, profile_prefix, path) {
            ProfileAssetSource::ZipEntry(entry_name)
        } else {
            continue;
        };
        decoded.push(ProfileAssetRestore { relative, source });
    }
    Ok(decoded)
}

pub(super) fn normalize_legacy_profile_asset_paths(state: &AppState, value: &mut Value) {
    match value {
        Value::Object(object) => {
            for nested in object.values_mut() {
                normalize_legacy_profile_asset_paths(state, nested);
            }
            for field in [
                "avatar",
                "avatarPath",
                "avatarUrl",
                "imagePath",
                "imageUrl",
                "background",
                "backgroundUrl",
                "sprite",
                "spritePath",
                "spriteUrl",
            ] {
                let Some(raw) = object.get(field).and_then(Value::as_str) else {
                    continue;
                };
                let Some(asset) = legacy_profile_asset_for_path(state, raw) else {
                    continue;
                };
                object.insert(field.to_string(), Value::String(asset.value));
                if matches!(field, "avatar" | "avatarPath" | "avatarUrl")
                    && asset.kind == LegacyProfileAssetKind::Avatar
                {
                    object
                        .entry("avatarFilePath".to_string())
                        .or_insert_with(|| Value::String(asset.absolute_path.clone()));
                    object
                        .entry("avatarFilename".to_string())
                        .or_insert_with(|| Value::String(asset.filename.clone()));
                }
                if matches!(field, "imagePath" | "imageUrl")
                    && asset.kind == LegacyProfileAssetKind::LorebookImage
                {
                    object
                        .entry("imageFilePath".to_string())
                        .or_insert_with(|| Value::String(asset.absolute_path.clone()));
                    object
                        .entry("imageFilename".to_string())
                        .or_insert_with(|| Value::String(asset.filename.clone()));
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                normalize_legacy_profile_asset_paths(state, item);
            }
        }
        Value::String(raw) => {
            if !OLD_ASSET_MARKERS.iter().any(|marker| raw.contains(marker)) {
                return;
            }
            let trimmed = raw.trim_start();
            if !(trimmed.starts_with('{') || trimmed.starts_with('[')) {
                return;
            }
            if let Ok(mut parsed) = serde_json::from_str::<Value>(raw) {
                normalize_legacy_profile_asset_paths(state, &mut parsed);
                if let Ok(serialized) = serde_json::to_string(&parsed) {
                    *raw = serialized;
                }
            }
        }
        _ => {}
    }
}

fn legacy_profile_asset_for_path(state: &AppState, value: &str) -> Option<LegacyProfileAsset> {
    let relative = legacy_profile_asset_relative_path(value)?;
    let absolute = state.data_dir.join(&relative);
    if !absolute.is_file() {
        return None;
    }
    let filename = relative
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_default();
    let kind = legacy_profile_asset_kind(&relative);
    let value = match kind {
        LegacyProfileAssetKind::Avatar | LegacyProfileAssetKind::FileDataUrl => {
            data_url_from_file(&absolute)?
        }
        LegacyProfileAssetKind::Background => managed_asset_url(
            "marinara-background:",
            &relative_asset_tail(&relative, Path::new("backgrounds")),
        ),
        LegacyProfileAssetKind::LorebookImage => managed_asset_url(
            "marinara-lorebook-image:",
            &relative_asset_tail(&relative, Path::new("lorebooks/images")),
        ),
    };
    Some(LegacyProfileAsset {
        value,
        absolute_path: absolute.to_string_lossy().to_string(),
        filename,
        kind,
    })
}

fn legacy_profile_asset_relative_path(value: &str) -> Option<PathBuf> {
    let normalized = normalize_profile_path(value.trim());
    if normalized.starts_with("data:")
        || normalized.starts_with("http://")
        || normalized.starts_with("https://")
        || normalized.starts_with("asset:")
        || normalized.starts_with("marinara-")
    {
        return None;
    }
    let path = normalized
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .trim_start_matches('/');
    for (prefix, root) in [
        ("api/avatars/file/", "avatars"),
        ("avatars/file/", "avatars"),
        ("avatars/", "avatars"),
        ("api/backgrounds/file/", "backgrounds"),
        ("backgrounds/file/", "backgrounds"),
        ("backgrounds/", "backgrounds"),
        ("api/sprites/file/", "sprites"),
        ("sprites/file/", "sprites"),
        ("sprites/", "sprites"),
        ("api/lorebook-images/file/", "lorebooks/images"),
        ("lorebooks/images/file/", "lorebooks/images"),
        ("lorebooks/images/", "lorebooks/images"),
    ] {
        let Some(tail) = path.strip_prefix(prefix) else {
            continue;
        };
        if tail.is_empty() || should_skip_profile_asset_path(tail) {
            return None;
        }
        return Some(Path::new(root).join(tail));
    }
    None
}

fn legacy_profile_asset_kind(relative: &Path) -> LegacyProfileAssetKind {
    if relative.starts_with(Path::new("avatars")) {
        LegacyProfileAssetKind::Avatar
    } else if relative.starts_with(Path::new("backgrounds")) {
        LegacyProfileAssetKind::Background
    } else if relative.starts_with(Path::new("lorebooks/images")) {
        LegacyProfileAssetKind::LorebookImage
    } else {
        LegacyProfileAssetKind::FileDataUrl
    }
}

fn managed_asset_url(prefix: &str, path: &str) -> String {
    format!("{prefix}{}", percent_encode_component(path))
}

fn data_url_from_file(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    Some(format!(
        "data:{};base64,{}",
        image_mime_from_path(path),
        general_purpose::STANDARD.encode(bytes)
    ))
}

fn image_mime_from_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        _ => "image/png",
    }
}

fn decode_profile_asset_data(value: &str) -> AppResult<Vec<u8>> {
    let payload = value
        .split_once(',')
        .filter(|(header, _)| header.starts_with("data:"))
        .map(|(_, payload)| payload)
        .unwrap_or(value)
        .trim();
    general_purpose::STANDARD
        .decode(payload)
        .map_err(|error| AppError::invalid_input(format!("Invalid profile asset data: {error}")))
}

fn write_profile_asset_in_root(data_dir: &Path, relative: &Path, bytes: &[u8]) -> AppResult<()> {
    let target = data_dir.join(relative);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(target, bytes)?;
    Ok(())
}

fn clear_profile_asset_dirs(data_dir: &Path) -> AppResult<()> {
    for dir in PROFILE_ASSET_DIRS {
        let path = data_dir.join(dir);
        if !path.exists() {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

pub(super) fn normalize_profile_path(value: &str) -> String {
    value.replace('\\', "/")
}

pub(super) fn should_skip_profile_asset_path(value: &str) -> bool {
    let normalized = normalize_profile_path(value);
    normalized
        .split('/')
        .any(|segment| segment.is_empty() || segment.starts_with('.'))
}

pub(super) fn safe_profile_asset_path(value: &str) -> AppResult<PathBuf> {
    let normalized = normalize_profile_path(value);
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

fn zip_asset_entry_name(
    names: &[String],
    profile_prefix: &str,
    asset_path: &str,
) -> Option<String> {
    let normalized_asset = normalize_profile_path(asset_path);
    let prefixed = if profile_prefix.is_empty() {
        normalized_asset.clone()
    } else {
        format!("{}/{}", profile_prefix.trim_matches('/'), normalized_asset)
    };
    names
        .iter()
        .find(|name| normalize_zip_entry_name(name).eq_ignore_ascii_case(&prefixed))
        .cloned()
        .or_else(|| {
            names
                .iter()
                .find(|name| normalize_zip_entry_name(name).eq_ignore_ascii_case(&normalized_asset))
                .cloned()
        })
}

pub(super) fn normalize_zip_entry_name(value: &str) -> String {
    normalize_profile_path(value)
        .trim_start_matches('/')
        .to_string()
}

fn relative_asset_tail(relative: &Path, root: &Path) -> String {
    relative
        .strip_prefix(root)
        .ok()
        .map(profile_relative_path)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            relative
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("")
                .to_string()
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_data_dir(test_name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "marinara-profile-{test_name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temporary profile data dir should be created");
        path
    }

    #[test]
    fn profile_asset_restore_replaces_managed_asset_dirs() {
        let data_dir = temp_data_dir("replace-assets");
        fs::create_dir_all(data_dir.join("avatars")).unwrap();
        fs::create_dir_all(data_dir.join("backgrounds/nested")).unwrap();
        fs::create_dir_all(data_dir.join("lorebooks/images/old")).unwrap();
        fs::create_dir_all(data_dir.join("unrelated")).unwrap();
        fs::write(data_dir.join("avatars/stale.png"), b"stale").unwrap();
        fs::write(data_dir.join("backgrounds/nested/stale.jpg"), b"stale").unwrap();
        fs::write(data_dir.join("lorebooks/images/old/stale.webp"), b"stale").unwrap();
        fs::write(data_dir.join("lorebooks/notes.txt"), b"keep").unwrap();
        fs::write(data_dir.join("unrelated/keep.txt"), b"keep").unwrap();

        let assets = json!([
            {
                "path": "avatars/new.png",
                "base64": general_purpose::STANDARD.encode(b"new avatar"),
            },
            {
                "path": "lorebooks/images/book/new.webp",
                "base64": general_purpose::STANDARD.encode(b"new lorebook image"),
            }
        ]);

        let restored =
            restore_profile_json_assets_in_root(&data_dir, Some(&assets), false).unwrap();

        assert_eq!(restored, 2);
        assert_eq!(
            fs::read(data_dir.join("avatars/new.png")).unwrap(),
            b"new avatar"
        );
        assert_eq!(
            fs::read(data_dir.join("lorebooks/images/book/new.webp")).unwrap(),
            b"new lorebook image"
        );
        assert!(!data_dir.join("avatars/stale.png").exists());
        assert!(!data_dir.join("backgrounds/nested/stale.jpg").exists());
        assert!(!data_dir.join("lorebooks/images/old/stale.webp").exists());
        assert_eq!(
            fs::read(data_dir.join("lorebooks/notes.txt")).unwrap(),
            b"keep"
        );
        assert_eq!(
            fs::read(data_dir.join("unrelated/keep.txt")).unwrap(),
            b"keep"
        );

        fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn invalid_profile_asset_payload_does_not_clear_existing_assets() {
        let data_dir = temp_data_dir("invalid-keeps-assets");
        fs::create_dir_all(data_dir.join("avatars")).unwrap();
        fs::write(data_dir.join("avatars/stale.png"), b"stale").unwrap();
        let assets = json!([
            {
                "path": "../escape.png",
                "base64": general_purpose::STANDARD.encode(b"escape"),
            }
        ]);

        let result = restore_profile_json_assets_in_root(&data_dir, Some(&assets), false);

        assert!(result.is_err());
        assert_eq!(
            fs::read(data_dir.join("avatars/stale.png")).unwrap(),
            b"stale"
        );

        fs::remove_dir_all(data_dir).unwrap();
    }
}
