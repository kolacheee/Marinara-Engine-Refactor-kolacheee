use base64::{engine::general_purpose, Engine as _};
use marinara_core::{now_iso, AppError, AppResult};
use marinara_security::assert_relative_safe_path;
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

const MANAGED_GAME_ASSET_CATEGORIES: &[&str] = &["music", "sfx", "ambient", "sprites", "backgrounds"];

#[derive(Clone)]
pub struct AssetService {
    root: PathBuf,
}

impl AssetService {
    pub fn new(root: impl Into<PathBuf>) -> AppResult<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn seed_missing_from(&self, seed_root: &Path) -> AppResult<()> {
        if !seed_root.exists() {
            return Ok(());
        }
        copy_missing(seed_root, &self.root)
    }

    pub fn absolute_path(&self, path: &str) -> AppResult<PathBuf> {
        Ok(self.root.join(assert_relative_safe_path(path)?))
    }

    pub fn absolute_path_string(&self, path: &str) -> AppResult<String> {
        Ok(self.absolute_path(path)?.to_string_lossy().to_string())
    }

    pub fn list(&self, subfolder: Option<&str>) -> AppResult<Vec<Value>> {
        let dir = match subfolder {
            Some(path) if !path.trim().is_empty() => self.absolute_path(path)?,
            _ => self.root.clone(),
        };
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut rows = Vec::new();
        for entry in fs::read_dir(dir)? {
            rows.push(self.entry_to_json(entry?.path())?);
        }
        sort_asset_rows(&mut rows);
        Ok(rows)
    }

    pub fn tree(&self) -> AppResult<Value> {
        self.node_for_path(&self.root, "game-assets")
    }

    pub fn manifest(&self) -> AppResult<Value> {
        let mut assets = Map::new();
        let mut by_category: Map<String, Value> = Map::new();
        let mut count = 0usize;
        self.collect_manifest_entries(&self.root, &mut assets, &mut by_category, &mut count)?;
        Ok(json!({
            "scannedAt": now_iso(),
            "count": count,
            "root": self.root.to_string_lossy(),
            "assets": assets,
            "byCategory": by_category
        }))
    }

    pub fn set_folder_description(&self, path: &str, description: &str) -> AppResult<Value> {
        let folder = self.absolute_path(path)?;
        if !folder.exists() {
            fs::create_dir_all(&folder)?;
        }
        if !folder.is_dir() {
            return Err(AppError::invalid_input("Asset path is not a folder"));
        }
        let meta_path = folder.join("meta.json");
        let mut meta = if meta_path.exists() {
            fs::read_to_string(&meta_path)
                .ok()
                .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
                .and_then(|value| value.as_object().cloned())
                .unwrap_or_default()
        } else {
            Map::new()
        };
        meta.insert(
            "description".to_string(),
            Value::String(description.to_string()),
        );
        fs::write(&meta_path, serde_json::to_vec_pretty(&Value::Object(meta))?)?;
        Ok(json!({ "path": path, "description": description }))
    }

    pub fn read_text(&self, path: &str) -> AppResult<String> {
        Ok(fs::read_to_string(self.absolute_path(path)?)?)
    }

    pub fn write_text(&self, path: &str, content: &str) -> AppResult<()> {
        let path = self.absolute_path(path)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;
        Ok(())
    }

    pub fn create_folder(&self, path: &str) -> AppResult<()> {
        fs::create_dir_all(self.absolute_path(path)?)?;
        Ok(())
    }

    pub fn remove(&self, path: &str, recursive: bool) -> AppResult<()> {
        let path = self.absolute_path(path)?;
        if path.is_dir() {
            if recursive {
                fs::remove_dir_all(path)?;
            } else {
                fs::remove_dir(path)?;
            }
        } else if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn rename(&self, path: &str, new_name: &str) -> AppResult<Value> {
        if new_name.contains('/') || new_name.contains('\\') || new_name.trim().is_empty() {
            return Err(AppError::invalid_input("Invalid asset name"));
        }
        let source = self.absolute_path(path)?;
        let target = source
            .parent()
            .ok_or_else(|| AppError::invalid_input("Asset has no parent folder"))?
            .join(new_name);
        fs::rename(&source, &target)?;
        Ok(json!({ "path": self.relative_string(&target) }))
    }

    pub fn copy_to_folder(&self, path: &str, target_folder: &str) -> AppResult<Value> {
        let source = self.absolute_path(path)?;
        let target_dir = self.absolute_path(target_folder)?;
        fs::create_dir_all(&target_dir)?;
        let file_name = source
            .file_name()
            .ok_or_else(|| AppError::invalid_input("Asset has no filename"))?;
        let target = unique_target_path(&target_dir.join(file_name))?;
        if source.is_dir() {
            copy_missing(&source, &target)?;
        } else {
            fs::copy(&source, &target)?;
        }
        Ok(json!({ "path": self.relative_string(&target) }))
    }

    pub fn move_to_folder(&self, path: &str, target_folder: &str) -> AppResult<Value> {
        let source = self.absolute_path(path)?;
        let target_dir = self.absolute_path(target_folder)?;
        fs::create_dir_all(&target_dir)?;
        let file_name = source
            .file_name()
            .ok_or_else(|| AppError::invalid_input("Asset has no filename"))?;
        let target = unique_target_path(&target_dir.join(file_name))?;
        fs::rename(&source, &target)?;
        Ok(json!({ "path": self.relative_string(&target) }))
    }

    pub fn write_upload(&self, category: &str, subcategory: Option<&str>, file: &Value) -> AppResult<Value> {
        if !MANAGED_GAME_ASSET_CATEGORIES.contains(&category) {
            return Err(AppError::invalid_input("Invalid game asset category"));
        }
        let name = file
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| !name.trim().is_empty())
            .ok_or_else(|| AppError::invalid_input("Uploaded file is missing a name"))?;
        if name.contains('/') || name.contains('\\') {
            return Err(AppError::invalid_input("Uploaded filename must not include path separators"));
        }
        let base64 = file
            .get("base64")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::invalid_input("Uploaded file is missing base64 data"))?;
        let bytes = general_purpose::STANDARD
            .decode(base64)
            .map_err(|error| AppError::invalid_input(format!("Invalid upload encoding: {error}")))?;

        let mut rel = PathBuf::from(category);
        if let Some(subcategory) = subcategory.filter(|value| !value.trim().is_empty()) {
            rel.push(assert_relative_safe_path(subcategory)?);
        }
        let dir = self.root.join(rel);
        fs::create_dir_all(&dir)?;
        let target = unique_target_path(&dir.join(name))?;
        fs::write(&target, bytes)?;
        let item = self.entry_to_json(target)?;
        Ok(json!({ "uploaded": true, "item": item }))
    }

    pub fn delete_many(&self, paths: &[String]) -> Value {
        let mut succeeded = Vec::new();
        let mut failed = Vec::new();
        for path in paths {
            match self.remove(path, false) {
                Ok(()) => succeeded.push(Value::String(path.clone())),
                Err(error) => failed.push(json!({ "path": path, "error": error.message })),
            }
        }
        json!({ "succeeded": succeeded, "failed": failed })
    }

    pub fn copy_many(&self, paths: &[String], target_folder: &str) -> Value {
        self.transfer_many(paths, target_folder, false)
    }

    pub fn move_many(&self, paths: &[String], target_folder: &str) -> Value {
        self.transfer_many(paths, target_folder, true)
    }

    pub fn file_info(&self, path: &str) -> AppResult<Value> {
        let absolute = self.absolute_path(path)?;
        let metadata = fs::metadata(&absolute)?;
        let name = absolute
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string());
        Ok(json!({
            "name": name,
            "path": self.relative_string(&absolute),
            "absolutePath": absolute.to_string_lossy(),
            "size": if metadata.is_file() { metadata.len() } else { 0 },
            "format": absolute.extension().map(|ext| ext.to_string_lossy().to_ascii_lowercase()),
            "modified": now_iso(),
            "created": now_iso()
        }))
    }

    fn transfer_many(&self, paths: &[String], target_folder: &str, move_files: bool) -> Value {
        let mut succeeded = Vec::new();
        let mut failed = Vec::new();
        for path in paths {
            let result = if move_files {
                self.move_to_folder(path, target_folder)
            } else {
                self.copy_to_folder(path, target_folder)
            };
            match result {
                Ok(value) => succeeded.push(value),
                Err(error) => failed.push(json!({ "path": path, "error": error.message })),
            }
        }
        json!({ "succeeded": succeeded, "failed": failed, "targetFolder": target_folder })
    }

    fn node_for_path(&self, path: &Path, root_name: &str) -> AppResult<Value> {
        let metadata = fs::metadata(path)?;
        let rel = self.relative_string(path);
        let name = if rel.is_empty() {
            root_name.to_string()
        } else {
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| root_name.to_string())
        };
        if metadata.is_dir() {
            let description = folder_description(path);
            let mut children = Vec::new();
            for entry in fs::read_dir(path)? {
                let child_path = entry?.path();
                if child_path.file_name().and_then(|name| name.to_str()) == Some("meta.json") {
                    continue;
                }
                children.push(self.node_for_path(&child_path, root_name)?);
            }
            sort_asset_rows(&mut children);
            let mut node = json!({
                "name": name,
                "path": rel,
                "type": "folder",
                "children": children,
                "size": 0,
                "modified": now_iso(),
                "absolutePath": path.to_string_lossy()
            });
            if let Some(description) = description {
                node["description"] = Value::String(description);
            }
            return Ok(node);
        }
        self.entry_to_json(path.to_path_buf())
    }

    fn entry_to_json(&self, path: PathBuf) -> AppResult<Value> {
        let metadata = fs::metadata(&path)?;
        let rel = self.relative_string(&path);
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| rel.clone());
        let ext = path
            .extension()
            .map(|ext| format!(".{}", ext.to_string_lossy().to_ascii_lowercase()))
            .unwrap_or_default();
        Ok(json!({
            "path": rel,
            "absolutePath": path.to_string_lossy(),
            "name": name,
            "type": if metadata.is_dir() { "folder" } else { "file" },
            "isDirectory": metadata.is_dir(),
            "ext": ext,
            "size": if metadata.is_file() { metadata.len() } else { 0 },
            "modified": now_iso()
        }))
    }

    fn collect_manifest_entries(
        &self,
        path: &Path,
        assets: &mut Map<String, Value>,
        by_category: &mut Map<String, Value>,
        count: &mut usize,
    ) -> AppResult<()> {
        if !path.exists() {
            return Ok(());
        }
        for entry in fs::read_dir(path)? {
            let path = entry?.path();
            if path.is_dir() {
                self.collect_manifest_entries(&path, assets, by_category, count)?;
                continue;
            }
            let rel = self.relative_string(&path);
            let segments: Vec<&str> = rel.split('/').collect();
            let Some(category) = segments.first().copied() else {
                continue;
            };
            if !MANAGED_GAME_ASSET_CATEGORIES.contains(&category) || segments.len() < 2 {
                continue;
            }
            let stem_path = rel
                .rsplit_once('.')
                .map(|(stem, _)| stem)
                .unwrap_or(rel.as_str())
                .to_string();
            let tag = stem_path.replace('/', ":");
            let ext = path
                .extension()
                .map(|ext| format!(".{}", ext.to_string_lossy().to_ascii_lowercase()))
                .unwrap_or_default();
            let subcategory = if segments.len() > 2 {
                segments[1..segments.len() - 1].join("/")
            } else {
                String::new()
            };
            let name = path
                .file_stem()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| tag.clone());
            let value = json!({
                "tag": tag,
                "category": category,
                "subcategory": subcategory,
                "name": name,
                "path": rel,
                "absolutePath": path.to_string_lossy(),
                "ext": ext
            });
            by_category
                .entry(category.to_string())
                .or_insert_with(|| Value::Array(Vec::new()))
                .as_array_mut()
                .expect("by_category entry is always an array")
                .push(value.clone());
            assets.insert(tag, value);
            *count += 1;
        }
        Ok(())
    }

    fn relative_string(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
            .trim_start_matches('/')
            .to_string()
    }
}

fn folder_description(path: &Path) -> Option<String> {
    let meta = fs::read_to_string(path.join("meta.json")).ok()?;
    let value: Value = serde_json::from_str(&meta).ok()?;
    value
        .get("description")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn copy_missing(source: &Path, target: &Path) -> AppResult<()> {
    if source.is_dir() {
        fs::create_dir_all(target)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_missing(&entry.path(), &target.join(entry.file_name()))?;
        }
        return Ok(());
    }
    if !target.exists() {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(source, target)?;
    }
    Ok(())
}

fn unique_target_path(target: &Path) -> AppResult<PathBuf> {
    if !target.exists() {
        return Ok(target.to_path_buf());
    }
    let parent = target.parent().unwrap_or_else(|| Path::new(""));
    let stem = target
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| "asset".to_string());
    let ext = target
        .extension()
        .map(|ext| format!(".{}", ext.to_string_lossy()))
        .unwrap_or_default();
    for index in 1..10_000 {
        let candidate = parent.join(format!("{stem}-{index}{ext}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(AppError::invalid_input("Could not find an available filename"))
}

fn sort_asset_rows(rows: &mut [Value]) {
    rows.sort_by(|a, b| {
        let a_dir = a
            .get("type")
            .and_then(Value::as_str)
            .map(|kind| kind == "folder")
            .or_else(|| a.get("isDirectory").and_then(Value::as_bool))
            .unwrap_or(false);
        let b_dir = b
            .get("type")
            .and_then(Value::as_str)
            .map(|kind| kind == "folder")
            .or_else(|| b.get("isDirectory").and_then(Value::as_bool))
            .unwrap_or(false);
        b_dir.cmp(&a_dir).then_with(|| {
            let a_name = a.get("name").and_then(Value::as_str).unwrap_or("");
            let b_name = b.get("name").and_then(Value::as_str).unwrap_or("");
            a_name.cmp(b_name)
        })
    });
}
