use super::images::{generate_pollinations_image, prompt_override};
use super::shared::*;
use super::*;

use image::{DynamicImage, GenericImageView, ImageFormat, Rgba};
use std::io::Cursor;
use std::path::{Path, PathBuf};

const SPRITE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "avif", "svg"];
const CLEANUP_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp"];

#[derive(Clone)]
struct SpritePlan {
    expressions: Vec<String>,
    appearance: String,
    cols: u32,
    rows: u32,
    sprite_type: String,
    prompt: String,
    sheet_width: u32,
    sheet_height: u32,
}

pub(crate) fn sprite_capabilities() -> AppResult<Value> {
    Ok(json!({
        "imageProcessingAvailable": true,
        "spriteGenerationAvailable": true,
        "backgroundRemovalAvailable": true,
        "reason": Value::Null,
        "backgroundRemover": {
            "engine": "builtin",
            "installed": true,
            "command": Value::Null,
            "source": "local",
            "runtimeDir": "",
            "reason": Value::Null
        }
    }))
}

pub(crate) async fn generate_sprite_sheet_preview(
    state: &AppState,
    body: Value,
) -> AppResult<Value> {
    validate_sprite_generation_body(state, &body)?;
    let plan = build_sprite_plan(&body);
    if plan.should_generate_individually() {
        let items = plan
            .expressions
            .iter()
            .map(|expression| {
                let prompt =
                    single_sprite_prompt(&plan.sprite_type, &plan.appearance, expression, &body);
                json!({
                    "id": sprite_prompt_review_id("expression", &plan.sprite_type, expression),
                    "kind": "sprite",
                    "title": format!("Expression: {}", expression.replace('_', " ")),
                    "prompt": prompt,
                    "width": 1024,
                    "height": 1024
                })
            })
            .collect::<Vec<_>>();
        return Ok(json!({ "items": items }));
    }
    Ok(json!({
        "items": [{
            "id": sprite_prompt_review_id("sheet", &plan.sprite_type, &format!("{}x{}-{}", plan.cols, plan.rows, plan.expressions.join(","))),
            "kind": "sprite",
            "title": if plan.sprite_type == "full-body" {
                format!("Full-body sprites: {}x{}", plan.cols, plan.rows)
            } else {
                format!("Expression sprites: {}x{}", plan.cols, plan.rows)
            },
            "prompt": plan.prompt,
            "width": plan.sheet_width,
            "height": plan.sheet_height
        }]
    }))
}

pub(crate) async fn generate_sprite_sheet(state: &AppState, body: Value) -> AppResult<Value> {
    validate_sprite_generation_body(state, &body)?;
    let plan = build_sprite_plan(&body);
    let reference_note = body
        .get("referenceImages")
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty())
        .map(|_| " Use the provided reference image(s) to preserve face, hair, body, outfit, colors, and distinctive features.")
        .unwrap_or("");

    if plan.should_generate_individually() {
        let mut cells = Vec::new();
        let mut failed = Vec::new();
        for expression in &plan.expressions {
            let prompt_id = sprite_prompt_review_id("expression", &plan.sprite_type, expression);
            let prompt = prompt_override(&body, &prompt_id).unwrap_or_else(|| {
                single_sprite_prompt(&plan.sprite_type, &plan.appearance, expression, &body)
            });
            match generate_pollinations_image(&format!("{prompt}{reference_note}"), 1024, 1024)
                .await
            {
                Ok((base64, _mime_type)) => {
                    let base64 = if body
                        .get("noBackground")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                    {
                        cleanup_image_base64(&base64, cleanup_strength(&body))?
                    } else {
                        base64
                    };
                    cells.push(json!({ "expression": expression, "base64": base64 }));
                }
                Err(error) => {
                    failed.push(json!({ "expression": expression, "error": error.message }))
                }
            }
        }
        if cells.is_empty() {
            return Err(AppError::new(
                "sprite_generation_failed",
                "All expression generations failed",
            ));
        }
        return Ok(json!({
            "sheetBase64": "",
            "cells": cells,
            "failedExpressions": failed
        }));
    }

    let prompt_id = sprite_prompt_review_id(
        "sheet",
        &plan.sprite_type,
        &format!("{}x{}-{}", plan.cols, plan.rows, plan.expressions.join(",")),
    );
    let prompt = prompt_override(&body, &prompt_id).unwrap_or_else(|| plan.prompt.clone());
    let (sheet_base64, _mime_type) = generate_pollinations_image(
        &format!("{prompt}{reference_note}"),
        plan.sheet_width as u64,
        plan.sheet_height as u64,
    )
    .await?;
    let sheet_base64 = if body
        .get("noBackground")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        cleanup_image_base64(&sheet_base64, cleanup_strength(&body))?
    } else {
        sheet_base64
    };
    let cells = slice_sprite_sheet(&sheet_base64, &plan)?;
    Ok(json!({ "sheetBase64": sheet_base64, "cells": cells }))
}

pub(crate) fn cleanup_generated_sprites(body: Value) -> AppResult<Value> {
    let cells = body
        .get("cells")
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty())
        .ok_or_else(|| AppError::invalid_input("At least one cell is required"))?;
    let strength = cleanup_strength(&body);
    let mut processed = Vec::new();
    for cell in cells {
        let expression = cell.get("expression").and_then(Value::as_str).unwrap_or("");
        let base64 = cell.get("base64").and_then(Value::as_str).ok_or_else(|| {
            AppError::invalid_input(format!("Invalid base64 image for expression: {expression}"))
        })?;
        processed.push(json!({
            "expression": expression,
            "base64": cleanup_image_base64(base64, strength)?
        }));
    }
    Ok(json!({
        "cells": processed,
        "engine": cleanup_engine(&body),
        "backgroundRemoverProcessed": 0,
        "builtinProcessed": cells.len()
    }))
}

pub(crate) fn list_sprites(state: &AppState, character_id: &str) -> AppResult<Value> {
    validate_safe_segment(character_id, "character ID")?;
    let dir = sprites_dir(state, character_id);
    fs::create_dir_all(&dir)?;
    let mut items = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() || !is_sprite_file(&path) {
            continue;
        }
        items.push(sprite_info_from_path(&path)?);
    }
    items.sort_by(|a, b| {
        a.get("expression")
            .and_then(Value::as_str)
            .unwrap_or("")
            .cmp(b.get("expression").and_then(Value::as_str).unwrap_or(""))
    });
    Ok(Value::Array(items))
}

pub(crate) fn upload_sprite(state: &AppState, character_id: &str, body: Value) -> AppResult<Value> {
    validate_safe_segment(character_id, "character ID")?;
    let expression = body
        .get("expression")
        .and_then(Value::as_str)
        .map(normalize_sprite_expression)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::invalid_input("Expression label is required"))?;
    let image = body
        .get("image")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::invalid_input("No image data provided"))?;
    let (bytes, ext) = decode_image_value(image)?;
    let dir = sprites_dir(state, character_id);
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{expression}.{ext}"));
    fs::write(&path, bytes)?;
    sprite_info_from_path(&path)
}

pub(crate) fn cleanup_saved_sprites(
    state: &AppState,
    character_id: &str,
    body: Value,
) -> AppResult<Value> {
    validate_safe_segment(character_id, "character ID")?;
    let dir = sprites_dir(state, character_id);
    if !dir.exists() {
        return Err(AppError::not_found("No sprites found"));
    }
    let requested = string_array_from_value(body.get("expressions"))
        .into_iter()
        .map(|expr| normalize_sprite_expression(&expr))
        .collect::<Vec<_>>();
    let targets = sprite_file_paths(&dir)?
        .into_iter()
        .filter(|path| {
            if requested.is_empty() {
                return true;
            }
            let expr = expression_from_path(path);
            requested.iter().any(|item| item == &expr)
        })
        .collect::<Vec<_>>();
    if targets.is_empty() {
        return Err(AppError::not_found("No matching sprites found"));
    }

    let backup_id = format!("{}-{}", now_millis(), new_id());
    let backup_dir = dir.join(".cleanup-backups").join(&backup_id);
    fs::create_dir_all(&backup_dir)?;
    let mut entries = Vec::new();
    let mut failed = Vec::new();
    let mut processed = 0usize;
    for path in targets {
        let expression = expression_from_path(&path);
        let filename = file_name(&path)?;
        if !is_cleanup_file(&path) {
            failed.push(json!({ "expression": expression, "error": "Only PNG, JPEG, and WEBP sprites can be background-cleaned" }));
            continue;
        }
        match cleanup_file_to_png(&path, cleanup_strength(&body)) {
            Ok(cleaned) => {
                fs::copy(&path, backup_dir.join(&filename))?;
                let output_filename = format!("{expression}.png");
                let output_path = dir.join(&output_filename);
                fs::write(&output_path, cleaned)?;
                if path != output_path {
                    let _ = fs::remove_file(&path);
                }
                entries.push(json!({
                    "expression": expression,
                    "originalFilename": filename,
                    "cleanedFilename": output_filename,
                    "backupFilename": filename
                }));
                processed += 1;
            }
            Err(error) => failed.push(json!({ "expression": expression, "error": error.message })),
        }
    }
    if entries.is_empty() {
        let _ = fs::remove_dir_all(&backup_dir);
    } else {
        fs::write(
            backup_dir.join("manifest.json"),
            serde_json::to_vec_pretty(&json!({
                "id": backup_id,
                "createdAt": now_iso(),
                "entries": entries
            }))?,
        )?;
    }
    Ok(json!({
        "processed": processed,
        "failed": failed,
        "backupId": if processed > 0 { json!(backup_id) } else { Value::Null },
        "engine": cleanup_engine(&body),
        "backgroundRemoverProcessed": 0,
        "builtinProcessed": processed,
        "sprites": list_sprites(state, character_id)?
    }))
}

pub(crate) fn restore_sprite_cleanup(
    state: &AppState,
    character_id: &str,
    body: Value,
) -> AppResult<Value> {
    validate_safe_segment(character_id, "character ID")?;
    let backup_id = body
        .get("backupId")
        .and_then(Value::as_str)
        .filter(|id| id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-'))
        .ok_or_else(|| AppError::invalid_input("Invalid backup ID"))?;
    let dir = sprites_dir(state, character_id);
    let backup_dir = dir.join(".cleanup-backups").join(backup_id);
    let manifest_path = backup_dir.join("manifest.json");
    if !manifest_path.exists() {
        return Err(AppError::not_found("Cleanup backup was not found"));
    }
    let manifest: Value = serde_json::from_slice(&fs::read(&manifest_path)?)?;
    let mut restored = 0usize;
    let mut failed = Vec::new();
    for entry in manifest
        .get("entries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let expression = entry
            .get("expression")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let backup_filename = entry
            .get("backupFilename")
            .and_then(Value::as_str)
            .unwrap_or("");
        let original_filename = entry
            .get("originalFilename")
            .and_then(Value::as_str)
            .unwrap_or("");
        let cleaned_filename = entry
            .get("cleanedFilename")
            .and_then(Value::as_str)
            .unwrap_or("");
        if [backup_filename, original_filename, cleaned_filename]
            .iter()
            .any(|name| validate_safe_segment(name, "backup filename").is_err())
        {
            failed.push(json!({ "expression": expression, "error": "Backup entry has an invalid filename" }));
            continue;
        }
        match fs::copy(
            backup_dir.join(backup_filename),
            dir.join(original_filename),
        ) {
            Ok(_) => {
                if cleaned_filename != original_filename {
                    let _ = fs::remove_file(dir.join(cleaned_filename));
                }
                restored += 1;
            }
            Err(error) => {
                failed.push(json!({ "expression": expression, "error": error.to_string() }))
            }
        }
    }
    if restored > 0 && failed.is_empty() {
        let _ = fs::remove_dir_all(&backup_dir);
    }
    Ok(
        json!({ "restored": restored, "failed": failed, "sprites": list_sprites(state, character_id)? }),
    )
}

pub(crate) fn delete_sprite(
    state: &AppState,
    character_id: &str,
    expression: &str,
) -> AppResult<Value> {
    validate_safe_segment(character_id, "character ID")?;
    let dir = sprites_dir(state, character_id);
    let normalized = normalize_sprite_expression(expression);
    let mut deleted = false;
    if dir.exists() {
        for path in sprite_file_paths(&dir)? {
            if expression_from_path(&path) == normalized {
                fs::remove_file(path)?;
                deleted = true;
            }
        }
    }
    if !deleted {
        for sprite in match list_collection(state, "sprites", Some(("characterId", character_id)))?
        {
            Value::Array(rows) => rows,
            _ => Vec::new(),
        } {
            if sprite
                .get("expression")
                .and_then(Value::as_str)
                .map(normalize_sprite_expression)
                == Some(normalized.clone())
            {
                if let Some(id) = sprite.get("id").and_then(Value::as_str) {
                    state.storage.delete("sprites", id)?;
                    deleted = true;
                }
            }
        }
    }
    Ok(json!({ "deleted": deleted }))
}

fn validate_sprite_generation_body(state: &AppState, body: &Value) -> AppResult<()> {
    let connection_id = required_string(body, "connectionId")?;
    let connection = get_required(state, "connections", connection_id)?;
    if connection.get("provider").and_then(Value::as_str) != Some("image_generation") {
        return Err(AppError::invalid_input(
            "Image generation connection not found or could not be decrypted",
        ));
    }
    required_string(body, "appearance")?;
    if string_array_from_value(body.get("expressions")).is_empty() {
        return Err(AppError::invalid_input(
            "At least one expression is required",
        ));
    }
    Ok(())
}

impl SpritePlan {
    fn should_generate_individually(&self) -> bool {
        self.sprite_type != "full-body"
            || self.expressions.len() == 1
            || self.cols == 1 && self.rows == 1
    }
}

fn build_sprite_plan(body: &Value) -> SpritePlan {
    let cols = body
        .get("cols")
        .and_then(Value::as_u64)
        .unwrap_or(2)
        .clamp(1, 6) as u32;
    let rows = body
        .get("rows")
        .and_then(Value::as_u64)
        .unwrap_or(3)
        .clamp(1, 6) as u32;
    let sprite_type = body
        .get("spriteType")
        .and_then(Value::as_str)
        .unwrap_or("expressions")
        .to_string();
    let full_body_expression_mode = body
        .get("fullBodyExpressionMode")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && sprite_type == "full-body";
    let expressions = string_array_from_value(body.get("expressions"))
        .into_iter()
        .take((cols * rows) as usize)
        .collect::<Vec<_>>();
    let appearance = body
        .get("appearance")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let (sheet_width, sheet_height, cell_width, cell_height) =
        resolve_canvas(cols, rows, &sprite_type);
    let prompt = if full_body_expression_mode {
        full_body_expression_sheet_prompt(
            cols,
            rows,
            &expressions,
            &appearance,
            sheet_width,
            sheet_height,
            cell_width,
            cell_height,
        )
    } else if sprite_type == "full-body" && expressions.len() == 1 && cols == 1 && rows == 1 {
        single_full_body_prompt(
            &appearance,
            expressions.first().map(String::as_str).unwrap_or("idle"),
            body,
        )
    } else if sprite_type == "full-body" {
        full_body_sheet_prompt(
            cols,
            rows,
            &expressions,
            &appearance,
            sheet_width,
            sheet_height,
            cell_width,
            cell_height,
        )
    } else if expressions.len() == 1 && cols == 1 && rows == 1 {
        single_portrait_prompt(
            &appearance,
            expressions.first().map(String::as_str).unwrap_or("neutral"),
            body,
        )
    } else {
        expression_sheet_prompt(cols, rows, &expressions, &appearance, body)
    };
    SpritePlan {
        expressions,
        appearance,
        cols,
        rows,
        sprite_type,
        prompt,
        sheet_width,
        sheet_height,
    }
}

fn resolve_canvas(cols: u32, rows: u32, sprite_type: &str) -> (u32, u32, u32, u32) {
    let cell_width = 512;
    let cell_height = if sprite_type == "full-body" { 768 } else { 512 };
    (
        cols * cell_width,
        rows * cell_height,
        cell_width,
        cell_height,
    )
}

fn expression_sheet_prompt(
    cols: u32,
    rows: u32,
    expressions: &[String],
    appearance: &str,
    body: &Value,
) -> String {
    let prompt = format!(
        "character expression sprite sheet source image, designed to be sliced into cells, EXACTLY {} total portrait cells and every cell must be filled, strict {cols} columns by {rows} rows grid, no extra rows, no extra columns, no extra panels, solid white background, thin straight borders or clean gutters separating every cell, same character in every cell, same outfit, same camera distance, same lighting, consistent art style, expressions left-to-right top-to-bottom, one cell per expression, no duplicates and none missing: {}, {appearance}, each cell shows one head-and-shoulders portrait with the requested facial expression, centered with no cropping, no text, no labels, no numbers, no captions, no watermark",
        expressions.len(),
        expressions.join(", ")
    );
    transparent_prompt(prompt, body)
}

fn single_portrait_prompt(appearance: &str, expression: &str, body: &Value) -> String {
    transparent_prompt(format!("single character portrait sprite, one character only, head and shoulders portrait, centered in frame, no cropping, solid white studio background, {appearance}, facial expression: {expression}, anime/game sprite style, consistent character design, no grid, no panel borders, no text, no labels, no watermark"), body)
}

fn single_full_body_prompt(appearance: &str, pose: &str, body: &Value) -> String {
    transparent_prompt(format!("single full-body character sprite, one character only, entire body visible from head to toe, centered in frame, no cropping, solid white studio background, {appearance}, pose/action: {pose}, anime/game sprite style, consistent character design, no grid, no panel borders, no text, no labels, no watermark"), body)
}

fn full_body_sheet_prompt(
    cols: u32,
    rows: u32,
    expressions: &[String],
    appearance: &str,
    sheet_width: u32,
    sheet_height: u32,
    cell_width: u32,
    cell_height: u32,
) -> String {
    format!("full-body character pose sprite sheet source image, designed to be sliced into cells, target output canvas is {sheet_width}x{sheet_height} pixels, with each cell exactly {cell_width}x{cell_height} pixels, EXACTLY {} total grid cells and every cell must be filled, strict {cols} columns by {rows} rows grid, all vertical grid cuts are evenly spaced, solid white background, same character in every cell, same outfit, same proportions, first {} cells left-to-right top-to-bottom must match these poses: {}, {appearance}, each cell shows one complete full-body character from head to toe, centered upright, feet visible, no cropping, no text, no labels, no watermark", cols * rows, expressions.len(), expressions.join(", "))
}

fn full_body_expression_sheet_prompt(
    cols: u32,
    rows: u32,
    expressions: &[String],
    appearance: &str,
    sheet_width: u32,
    sheet_height: u32,
    cell_width: u32,
    cell_height: u32,
) -> String {
    format!("full-body character expression sprite sheet source image, designed to be sliced into cells, target output canvas is {sheet_width}x{sheet_height} pixels, with each cell exactly {cell_width}x{cell_height} pixels, strict {cols} columns by {rows} rows grid, solid white background, same character and full-body pose family in every cell, expressions left-to-right top-to-bottom: {}, {appearance}, entire body visible from head to toe, centered in each cell, no cropping, no text, no labels, no watermark", expressions.join(", "))
}

fn single_sprite_prompt(
    sprite_type: &str,
    appearance: &str,
    expression: &str,
    body: &Value,
) -> String {
    if sprite_type == "full-body" {
        single_full_body_prompt(appearance, expression, body)
    } else {
        single_portrait_prompt(appearance, expression, body)
    }
}

fn transparent_prompt(prompt: String, body: &Value) -> String {
    if body
        .get("nativeTransparentPng")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        prompt
            .replace("solid white studio background", "no background, png format")
            .replace("solid white background", "no background, png format")
    } else {
        prompt
    }
}

fn sprite_prompt_review_id(kind: &str, sprite_type: &str, label: &str) -> String {
    let normalized = label
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, ',' | '_' | '-') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .chars()
        .take(120)
        .collect::<String>();
    format!(
        "sprite:{}:{kind}:{}",
        if sprite_type.is_empty() {
            "expressions"
        } else {
            sprite_type
        },
        if normalized.is_empty() {
            "request"
        } else {
            &normalized
        }
    )
}

fn slice_sprite_sheet(sheet_base64: &str, plan: &SpritePlan) -> AppResult<Vec<Value>> {
    let bytes = general_purpose::STANDARD
        .decode(extract_base64_image_data(sheet_base64))
        .map_err(|error| {
            AppError::invalid_input(format!("Invalid generated sheet image: {error}"))
        })?;
    let image = image::load_from_memory(&bytes).map_err(image_error)?;
    let (width, height) = image.dimensions();
    let cell_width = width / plan.cols.max(1);
    let cell_height = height / plan.rows.max(1);
    let mut cells = Vec::new();
    for row in 0..plan.rows {
        for col in 0..plan.cols {
            let index = (row * plan.cols + col) as usize;
            if index >= plan.expressions.len() {
                break;
            }
            let crop = image.crop_imm(col * cell_width, row * cell_height, cell_width, cell_height);
            cells.push(json!({
                "expression": plan.expressions[index],
                "base64": encode_png_base64(crop)?
            }));
        }
    }
    Ok(cells)
}

fn cleanup_file_to_png(path: &Path, strength: u8) -> AppResult<Vec<u8>> {
    let bytes = fs::read(path)?;
    let image = image::load_from_memory(&bytes).map_err(image_error)?;
    encode_png(cleanup_image(image, strength))
}

fn cleanup_image_base64(value: &str, strength: u8) -> AppResult<String> {
    let bytes = general_purpose::STANDARD
        .decode(extract_base64_image_data(value))
        .map_err(|error| AppError::invalid_input(format!("Invalid base64 image: {error}")))?;
    let image = image::load_from_memory(&bytes).map_err(image_error)?;
    Ok(general_purpose::STANDARD.encode(encode_png(cleanup_image(image, strength))?))
}

fn cleanup_image(image: DynamicImage, strength: u8) -> DynamicImage {
    let mut rgba = image.to_rgba8();
    let threshold = 18.0 + (strength as f32 * 1.6);
    for pixel in rgba.pixels_mut() {
        let Rgba([r, g, b, a]) = *pixel;
        let distance =
            ((255.0 - r as f32).powi(2) + (255.0 - g as f32).powi(2) + (255.0 - b as f32).powi(2))
                .sqrt();
        if distance <= threshold {
            let alpha = ((distance / threshold).clamp(0.0, 1.0) * a as f32) as u8;
            *pixel = Rgba([r, g, b, alpha]);
        }
    }
    DynamicImage::ImageRgba8(rgba)
}

fn encode_png_base64(image: DynamicImage) -> AppResult<String> {
    Ok(general_purpose::STANDARD.encode(encode_png(image)?))
}

fn encode_png(image: DynamicImage) -> AppResult<Vec<u8>> {
    let mut cursor = Cursor::new(Vec::new());
    image
        .write_to(&mut cursor, ImageFormat::Png)
        .map_err(image_error)?;
    Ok(cursor.into_inner())
}

fn decode_image_value(value: &str) -> AppResult<(Vec<u8>, String)> {
    let (mime, base64) = if let Some((prefix, payload)) = value.split_once(',') {
        let mime = prefix
            .strip_prefix("data:")
            .and_then(|rest| rest.split(';').next())
            .unwrap_or("image/png");
        (mime, payload)
    } else {
        ("image/png", value)
    };
    let ext = match mime {
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "image/avif" => "avif",
        "image/svg+xml" => "svg",
        _ => "png",
    };
    let bytes = general_purpose::STANDARD
        .decode(base64.trim())
        .map_err(|error| AppError::invalid_input(format!("Invalid image data: {error}")))?;
    Ok((bytes, ext.to_string()))
}

fn extract_base64_image_data(value: &str) -> &str {
    value
        .split_once(',')
        .map(|(_, data)| data)
        .unwrap_or(value)
        .trim()
}

fn sprite_info_from_path(path: &Path) -> AppResult<Value> {
    let filename = file_name(path)?;
    let expression = expression_from_path(path);
    let bytes = fs::read(path)?;
    let mime = mime_for_path(path);
    Ok(json!({
        "expression": expression,
        "filename": filename,
        "url": format!("data:{mime};base64,{}", general_purpose::STANDARD.encode(bytes))
    }))
}

fn sprites_dir(state: &AppState, character_id: &str) -> PathBuf {
    state.data_dir.join("sprites").join(character_id)
}

fn sprite_file_paths(dir: &Path) -> AppResult<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_file() && is_sprite_file(&path) {
            paths.push(path);
        }
    }
    Ok(paths)
}

fn file_name(path: &Path) -> AppResult<String> {
    path.file_name()
        .and_then(|value| value.to_str())
        .map(ToOwned::to_owned)
        .ok_or_else(|| AppError::invalid_input("Invalid sprite filename"))
}

fn expression_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_string()
}

fn is_sprite_file(path: &Path) -> bool {
    extension(path).is_some_and(|ext| SPRITE_EXTENSIONS.contains(&ext.as_str()))
}

fn is_cleanup_file(path: &Path) -> bool {
    extension(path).is_some_and(|ext| CLEANUP_EXTENSIONS.contains(&ext.as_str()))
}

fn extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
}

fn mime_for_path(path: &Path) -> &'static str {
    match extension(path).as_deref() {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("avif") => "image/avif",
        Some("svg") => "image/svg+xml",
        _ => "image/png",
    }
}

fn normalize_sprite_expression(raw: &str) -> String {
    raw.trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn cleanup_strength(body: &Value) -> u8 {
    body.get("cleanupStrength")
        .and_then(Value::as_u64)
        .unwrap_or(35)
        .min(100) as u8
}

fn cleanup_engine(body: &Value) -> String {
    match body
        .get("engine")
        .and_then(Value::as_str)
        .unwrap_or("auto")
        .to_ascii_lowercase()
        .as_str()
    {
        "backgroundremover" | "background-remover" | "ai" => "backgroundremover".to_string(),
        "builtin" | "built-in" | "matte" | "white" => "builtin".to_string(),
        _ => "auto".to_string(),
    }
}

fn validate_safe_segment(value: &str, label: &str) -> AppResult<()> {
    if value.is_empty() || value.contains("..") || value.contains('/') || value.contains('\\') {
        Err(AppError::invalid_input(format!("Invalid {label}")))
    } else {
        Ok(())
    }
}

fn image_error(error: image::ImageError) -> AppError {
    AppError::new("image_processing_error", error.to_string())
}
