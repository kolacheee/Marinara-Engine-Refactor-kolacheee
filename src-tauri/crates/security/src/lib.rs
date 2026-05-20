use marinara_core::{AppError, AppResult};
use std::path::{Component, Path, PathBuf};
use url::Url;

pub fn validate_collection_name(name: &str) -> AppResult<()> {
    let valid = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
    if valid {
        Ok(())
    } else {
        Err(AppError::invalid_input(format!(
            "Invalid collection name: {name}"
        )))
    }
}

pub fn assert_relative_safe_path(path: &str) -> AppResult<PathBuf> {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        return Err(AppError::invalid_input("Absolute paths are not allowed here"));
    }

    for component in candidate.components() {
        if matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_)) {
            return Err(AppError::invalid_input("Path escapes are not allowed here"));
        }
    }

    Ok(candidate.to_path_buf())
}

pub fn assert_inside_dir(base: &Path, path: &Path) -> AppResult<PathBuf> {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    let canonical_base = base.canonicalize().map_err(AppError::from)?;
    let canonical_path = canonicalize_existing_prefix(&joined)?;
    if canonical_path.starts_with(&canonical_base) {
        Ok(canonical_path)
    } else {
        Err(AppError::invalid_input("Path is outside the allowed directory"))
    }
}

fn canonicalize_existing_prefix(path: &Path) -> AppResult<PathBuf> {
    let mut missing = Vec::new();
    let mut current = path;
    while !current.exists() {
        let Some(parent) = current.parent() else {
            return Err(AppError::invalid_input("Path has no existing parent"));
        };
        if let Some(name) = current.file_name() {
            missing.push(name.to_os_string());
        }
        current = parent;
    }

    let mut canonical = current.canonicalize().map_err(AppError::from)?;
    for component in missing.iter().rev() {
        canonical.push(component);
    }
    Ok(canonical)
}

pub fn is_allowed_outbound_url(raw: &str, allow_local: bool) -> bool {
    let Ok(url) = Url::parse(raw) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    if allow_local {
        return true;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    !matches!(host, "localhost" | "127.0.0.1" | "::1")
}
