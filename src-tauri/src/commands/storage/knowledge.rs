use super::shared::*;
use super::*;

pub(crate) fn knowledge_meta_path(state: &AppState) -> std::path::PathBuf {
    state.data_dir.join("knowledge-sources").join("meta.json")
}

pub(crate) fn read_knowledge_meta(state: &AppState) -> AppResult<Map<String, Value>> {
    let path = knowledge_meta_path(state);
    if !path.exists() {
        return Ok(Map::new());
    }
    let parsed: Value = serde_json::from_slice(&fs::read(path)?)?;
    Ok(parsed.as_object().cloned().unwrap_or_default())
}

pub(crate) fn write_knowledge_meta(state: &AppState, meta: &Map<String, Value>) -> AppResult<()> {
    let path = knowledge_meta_path(state);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(meta)?)?;
    Ok(())
}

pub(crate) fn knowledge_sources_call(
    state: &AppState,
    method: &str,
    rest: &[&str],
    body: Value,
) -> AppResult<Value> {
    let dir = state.data_dir.join("knowledge-sources");
    fs::create_dir_all(&dir)?;
    match (method, rest) {
        ("GET", []) => {
            let meta = read_knowledge_meta(state)?;
            let mut rows = meta.values().cloned().collect::<Vec<_>>();
            rows.sort_by(|a, b| {
                b.get("uploadedAt")
                    .and_then(Value::as_str)
                    .cmp(&a.get("uploadedAt").and_then(Value::as_str))
            });
            Ok(Value::Array(rows))
        }
        ("POST", ["upload"]) => {
            let (original_name, _content_type, bytes) = decode_uploaded_file(&body)?;
            let ext = std::path::Path::new(&original_name)
                .extension()
                .map(|ext| format!(".{}", ext.to_string_lossy().to_ascii_lowercase()))
                .unwrap_or_default();
            let allowed = [
                ".txt", ".md", ".csv", ".json", ".xml", ".html", ".htm", ".log", ".yaml", ".yml",
                ".tsv", ".pdf",
            ];
            if !allowed.contains(&ext.as_str()) {
                return Err(AppError::invalid_input(format!(
                    "Unsupported knowledge source type: {ext}"
                )));
            }
            let id = new_id();
            let filename = format!("{id}{ext}");
            fs::write(dir.join(&filename), bytes)?;
            let entry = json!({
                "id": id,
                "originalName": original_name,
                "filename": filename,
                "size": fs::metadata(dir.join(&filename)).map(|m| m.len()).unwrap_or(0),
                "uploadedAt": now_iso()
            });
            let mut meta = read_knowledge_meta(state)?;
            meta.insert(id, entry.clone());
            write_knowledge_meta(state, &meta)?;
            Ok(entry)
        }
        ("DELETE", [id]) => {
            let mut meta = read_knowledge_meta(state)?;
            if let Some(entry) = meta.remove(*id) {
                if let Some(filename) = entry.get("filename").and_then(Value::as_str) {
                    let _ = fs::remove_file(dir.join(filename));
                }
                write_knowledge_meta(state, &meta)?;
                Ok(json!({ "success": true }))
            } else {
                Err(AppError::not_found("Knowledge source not found"))
            }
        }
        ("GET", [id, "text"]) => {
            let meta = read_knowledge_meta(state)?;
            let entry = meta
                .get(*id)
                .ok_or_else(|| AppError::not_found("Knowledge source not found"))?;
            let filename = entry
                .get("filename")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::not_found("Knowledge source file missing"))?;
            let text = extract_file_text(&dir.join(filename))?;
            Ok(
                json!({ "id": id, "originalName": entry.get("originalName").cloned().unwrap_or(Value::Null), "text": text }),
            )
        }
        _ => Err(AppError::new(
            "route_not_found",
            format!("Unknown knowledge-sources route: {method} /{}", rest.join("/")),
        )),
    }
}

fn extract_file_text(path: &std::path::Path) -> AppResult<String> {
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();

    if ext == "pdf" {
        return Ok(pdf_extract::extract_text(path)
            .unwrap_or_else(|_| "[PDF text extraction failed]".to_string()));
    }

    let bytes = fs::read(path)?;
    Ok(String::from_utf8(bytes)
        .unwrap_or_else(|err| String::from_utf8_lossy(err.as_bytes()).into_owned()))
}
