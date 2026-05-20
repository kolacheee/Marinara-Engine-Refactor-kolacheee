use super::assets::{
    normalize_zip_entry_name, profile_assets_need_zip_restore, restore_profile_zip_assets,
};
use super::{import_profile_collections, legacy::import_legacy_profile_tables};
use crate::state::AppState;
use marinara_core::{AppError, AppResult};
use serde_json::Value;
use std::fs::File;
use std::io::Read;
use std::path::Path;

const PROFILE_JSON_ENTRY: &str = "marinara-profile.json";
const MAX_PROFILE_JSON_BYTES: usize = 128 * 1024 * 1024;

pub(super) fn import_profile_zip(state: &AppState, path: &Path) -> AppResult<Value> {
    let file = File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| AppError::invalid_input(format!("Could not read profile ZIP: {error}")))?;
    let names = zip_entry_names(&mut archive)?;
    let (profile_entry, profile_prefix) = profile_json_entry(&names)?;
    let envelope = read_profile_zip_json(&mut archive, &profile_entry)?;
    let data = envelope
        .get("data")
        .and_then(Value::as_object)
        .filter(|_| envelope.get("type").and_then(Value::as_str) == Some("marinara_profile"))
        .ok_or_else(|| AppError::invalid_input("Invalid Marinara profile export"))?;
    let files = data
        .get("fileStorage")
        .and_then(|value| value.get("files"))
        .or_else(|| data.get("assets"));
    let zip_asset_refs = profile_assets_need_zip_restore(files);
    let restored_assets =
        restore_profile_zip_assets(state, &mut archive, &names, &profile_prefix, files)?;
    let mut result = if let Some(collections) = data.get("collections").and_then(Value::as_object) {
        import_profile_collections(state, data, collections)?
    } else {
        let tables = data
            .get("fileStorage")
            .and_then(|value| value.get("tables"))
            .and_then(Value::as_object)
            .ok_or_else(|| {
                AppError::invalid_input(
                    "Profile ZIP must contain data.collections or data.fileStorage.tables",
                )
            })?;
        import_legacy_profile_tables(state, data, tables)?
    };
    if restored_assets > 0 || zip_asset_refs {
        if let Some(imported) = result.get_mut("imported").and_then(Value::as_object_mut) {
            imported.insert("files".to_string(), serde_json::json!(restored_assets));
        }
    }
    Ok(result)
}

fn zip_entry_names<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
) -> AppResult<Vec<String>> {
    let mut names = Vec::new();
    for index in 0..archive.len() {
        let file = archive.by_index(index).map_err(|error| {
            AppError::invalid_input(format!("Could not read profile ZIP entry: {error}"))
        })?;
        names.push(file.name().to_string());
    }
    Ok(names)
}

fn profile_json_entry(names: &[String]) -> AppResult<(String, String)> {
    for name in names {
        let normalized = normalize_zip_entry_name(name);
        if normalized == PROFILE_JSON_ENTRY
            || normalized.ends_with(&format!("/{PROFILE_JSON_ENTRY}"))
        {
            let prefix = normalized
                .strip_suffix(PROFILE_JSON_ENTRY)
                .unwrap_or("")
                .trim_end_matches('/')
                .to_string();
            return Ok((name.clone(), prefix));
        }
    }
    Err(AppError::invalid_input(
        "Profile ZIP is missing marinara-profile.json",
    ))
}

fn read_profile_zip_json<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    entry_name: &str,
) -> AppResult<Value> {
    let entry = archive.by_name(entry_name).map_err(|error| {
        AppError::invalid_input(format!("Could not read marinara-profile.json: {error}"))
    })?;
    let mut raw = Vec::new();
    let mut limited = entry.take(MAX_PROFILE_JSON_BYTES as u64);
    limited.read_to_end(&mut raw)?;
    if raw.len() == MAX_PROFILE_JSON_BYTES {
        return Err(AppError::invalid_input(
            "marinara-profile.json in profile ZIP is too large",
        ));
    }
    Ok(serde_json::from_slice(&raw)?)
}
