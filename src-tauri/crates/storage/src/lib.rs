use marinara_core::{ensure_object, new_id, now_iso, AppError, AppResult};
use marinara_security::validate_collection_name;
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct FileStorage {
    root: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl FileStorage {
    pub fn new(root: impl Into<PathBuf>) -> AppResult<Self> {
        let root = root.into();
        fs::create_dir_all(root.join("collections"))?;
        Ok(Self {
            root,
            lock: Arc::new(Mutex::new(())),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn list(&self, collection: &str) -> AppResult<Vec<Value>> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| AppError::new("lock_error", "Storage lock poisoned"))?;
        self.read_collection(collection)
    }

    pub fn list_where(
        &self,
        collection: &str,
        filters: &Map<String, Value>,
    ) -> AppResult<Vec<Value>> {
        let rows = self.list(collection)?;
        Ok(rows
            .into_iter()
            .filter(|row| {
                let Some(obj) = row.as_object() else {
                    return false;
                };
                filters
                    .iter()
                    .all(|(key, expected)| obj.get(key) == Some(expected))
            })
            .collect())
    }

    pub fn get(&self, collection: &str, id: &str) -> AppResult<Option<Value>> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| AppError::new("lock_error", "Storage lock poisoned"))?;
        Ok(self
            .read_collection(collection)?
            .into_iter()
            .find(|row| row.get("id").and_then(Value::as_str) == Some(id)))
    }

    pub fn create(&self, collection: &str, value: Value) -> AppResult<Value> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| AppError::new("lock_error", "Storage lock poisoned"))?;
        let mut rows = self.read_collection(collection)?;
        let mut object = ensure_object(value)?;
        let id = object
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.trim().is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(new_id);
        let now = now_iso();
        object.insert("id".to_string(), Value::String(id.clone()));
        object
            .entry("createdAt".to_string())
            .or_insert_with(|| Value::String(now.clone()));
        object
            .entry("updatedAt".to_string())
            .or_insert_with(|| Value::String(now));
        let record = Value::Object(object);
        rows.retain(|row| row.get("id").and_then(Value::as_str) != Some(id.as_str()));
        rows.push(record.clone());
        self.write_collection(collection, &rows)?;
        Ok(record)
    }

    pub fn upsert_with_id(&self, collection: &str, id: &str, value: Value) -> AppResult<Value> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| AppError::new("lock_error", "Storage lock poisoned"))?;
        let mut rows = self.read_collection(collection)?;
        let mut object = ensure_object(value)?;
        let now = now_iso();
        object.insert("id".to_string(), Value::String(id.to_string()));
        object
            .entry("createdAt".to_string())
            .or_insert_with(|| Value::String(now.clone()));
        object
            .entry("updatedAt".to_string())
            .or_insert_with(|| Value::String(now));
        let record = Value::Object(object);
        rows.retain(|row| row.get("id").and_then(Value::as_str) != Some(id));
        rows.push(record.clone());
        self.write_collection(collection, &rows)?;
        Ok(record)
    }

    pub fn patch(&self, collection: &str, id: &str, patch: Value) -> AppResult<Value> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| AppError::new("lock_error", "Storage lock poisoned"))?;
        let mut rows = self.read_collection(collection)?;
        let patch = ensure_object(patch)?;
        let mut found = None;
        for row in &mut rows {
            if row.get("id").and_then(Value::as_str) != Some(id) {
                continue;
            }
            let Some(object) = row.as_object_mut() else {
                return Err(AppError::invalid_input("Stored record is not an object"));
            };
            for (key, value) in patch {
                object.insert(key, value);
            }
            object.insert("updatedAt".to_string(), Value::String(now_iso()));
            found = Some(Value::Object(object.clone()));
            break;
        }
        let Some(record) = found else {
            return Err(AppError::not_found(format!(
                "{collection}/{id} was not found"
            )));
        };
        self.write_collection(collection, &rows)?;
        Ok(record)
    }

    pub fn delete(&self, collection: &str, id: &str) -> AppResult<bool> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| AppError::new("lock_error", "Storage lock poisoned"))?;
        let mut rows = self.read_collection(collection)?;
        let before = rows.len();
        rows.retain(|row| row.get("id").and_then(Value::as_str) != Some(id));
        let deleted = rows.len() != before;
        if deleted {
            self.write_collection(collection, &rows)?;
        }
        Ok(deleted)
    }

    pub fn replace_all(&self, collection: &str, rows: Vec<Value>) -> AppResult<()> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| AppError::new("lock_error", "Storage lock poisoned"))?;
        self.write_collection(collection, &rows)
    }

    pub fn clear_all(&self) -> AppResult<()> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| AppError::new("lock_error", "Storage lock poisoned"))?;
        let collections = self.root.join("collections");
        if collections.exists() {
            fs::remove_dir_all(&collections)?;
        }
        fs::create_dir_all(collections)?;
        Ok(())
    }

    fn collection_path(&self, collection: &str) -> AppResult<PathBuf> {
        validate_collection_name(collection)?;
        Ok(self
            .root
            .join("collections")
            .join(format!("{collection}.json")))
    }

    fn read_collection(&self, collection: &str) -> AppResult<Vec<Value>> {
        let path = self.collection_path(collection)?;
        if !path.exists() {
            return Ok(Vec::new());
        }
        let raw = fs::read_to_string(path)?;
        if raw.trim().is_empty() {
            return Ok(Vec::new());
        }
        let parsed: Value = serde_json::from_str(&raw)?;
        match parsed {
            Value::Array(rows) => Ok(rows),
            _ => Err(AppError::invalid_input(format!(
                "Collection {collection} did not contain a JSON array"
            ))),
        }
    }

    fn write_collection(&self, collection: &str, rows: &[Value]) -> AppResult<()> {
        let path = self.collection_path(collection)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(rows)?)?;
        fs::rename(tmp, path)?;
        Ok(())
    }
}

pub fn record_id(value: &Value) -> Option<&str> {
    value.get("id").and_then(Value::as_str)
}

pub fn merge_object_field(
    record: &mut Value,
    field: &str,
    patch: Map<String, Value>,
) -> AppResult<()> {
    let object = record
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_input("Stored record is not an object"))?;
    let current = object
        .entry(field.to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_input(format!("{field} is not an object")))?;
    for (key, value) in patch {
        current.insert(key, value);
    }
    object.insert("updatedAt".to_string(), Value::String(now_iso()));
    Ok(())
}
