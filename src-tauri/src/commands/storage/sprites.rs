use super::images::{
    generate_image_with_options, image_generation_options, is_openai_gpt_image_model,
    prompt_override,
};
use super::shared::*;
use super::*;

use image::{DynamicImage, GenericImageView, ImageFormat, Rgba, RgbaImage};
use std::collections::VecDeque;
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
    generate_individually: bool,
    model: String,
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
        "cleanupEngine": {
            "engine": "builtin",
            "installed": true,
            "command": Value::Null,
            "source": "local",
            "runtimeDir": "",
            "reason": Value::Null
        }
    }))
}

pub(crate) fn sprite_cleanup_status() -> AppResult<Value> {
    Ok(json!({
        "available": true,
        "engine": "builtin",
        "installed": true,
        "source": "local",
        "reason": Value::Null,
        "cleanupEngine": {
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
    let connection_id = required_string(&body, "connectionId")?;
    let connection = get_required(state, "connections", connection_id)?;
    let plan = build_sprite_plan(&body, Some(&connection));
    if plan.should_generate_individually() {
        let items = plan
            .expressions
            .iter()
            .map(|expression| {
                let prompt = single_sprite_prompt(
                    &plan.sprite_type,
                    &plan.appearance,
                    expression,
                    &body,
                    &plan.model,
                );
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
    let connection_id = required_string(&body, "connectionId")?;
    let connection = get_required(state, "connections", connection_id)?;
    let image_options = image_generation_options(&body);
    let plan = build_sprite_plan(&body, Some(&connection));
    let reference_note = (!image_options.reference_images.is_empty())
        .then_some(" Use the provided reference image(s) to preserve face, hair, body, outfit, colors, and distinctive features.")
        .unwrap_or("");

    if plan.should_generate_individually() {
        let mut cells = Vec::new();
        let mut failed = Vec::new();
        for expression in &plan.expressions {
            let prompt_id = sprite_prompt_review_id("expression", &plan.sprite_type, expression);
            let prompt = prompt_override(&body, &prompt_id).unwrap_or_else(|| {
                single_sprite_prompt(
                    &plan.sprite_type,
                    &plan.appearance,
                    expression,
                    &body,
                    &plan.model,
                )
            });
            match generate_image_with_options(
                &connection,
                &format!("{prompt}{reference_note}"),
                1024,
                1024,
                image_options.clone(),
            )
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
    let (sheet_base64, _mime_type) = generate_image_with_options(
        &connection,
        &format!("{prompt}{reference_note}"),
        plan.sheet_width as u64,
        plan.sheet_height as u64,
        image_options,
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
        "externalCleanupProcessed": 0,
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

pub(crate) fn sprite_file(
    state: &AppState,
    character_id: &str,
    filename: &str,
) -> AppResult<Value> {
    validate_safe_segment(character_id, "character ID")?;
    validate_safe_segment(filename, "sprite filename")?;
    let path = sprites_dir(state, character_id).join(filename);
    if !path.is_file() || !is_sprite_file(&path) {
        return Err(AppError::not_found("Sprite file was not found"));
    }
    sprite_info_from_path(&path)
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

pub(crate) fn clean_saved_sprites(
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

    let restore_point_id = format!("{}-{}", now_millis(), new_id());
    let restore_point_dir = dir.join(".cleanup-restore-points").join(&restore_point_id);
    fs::create_dir_all(&restore_point_dir)?;
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
                fs::copy(&path, restore_point_dir.join(&filename))?;
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
                    "restorePointFilename": filename
                }));
                processed += 1;
            }
            Err(error) => failed.push(json!({ "expression": expression, "error": error.message })),
        }
    }
    if entries.is_empty() {
        let _ = fs::remove_dir_all(&restore_point_dir);
    } else {
        fs::write(
            restore_point_dir.join("manifest.json"),
            serde_json::to_vec_pretty(&json!({
                "id": restore_point_id,
                "createdAt": now_iso(),
                "entries": entries
            }))?,
        )?;
    }
    Ok(json!({
        "processed": processed,
        "failed": failed,
        "restorePointId": if processed > 0 { json!(restore_point_id) } else { Value::Null },
        "engine": cleanup_engine(&body),
        "externalCleanupProcessed": 0,
        "builtinProcessed": processed,
        "sprites": list_sprites(state, character_id)?
    }))
}

pub(crate) fn restore_sprite_cleanup_point(
    state: &AppState,
    character_id: &str,
    body: Value,
) -> AppResult<Value> {
    validate_safe_segment(character_id, "character ID")?;
    let restore_point_id = body
        .get("restorePointId")
        .and_then(Value::as_str)
        .filter(|id| id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-'))
        .ok_or_else(|| AppError::invalid_input("Invalid cleanup restore point ID"))?;
    let dir = sprites_dir(state, character_id);
    let restore_point_dir = dir.join(".cleanup-restore-points").join(restore_point_id);
    let manifest_path = restore_point_dir.join("manifest.json");
    if !manifest_path.exists() {
        return Err(AppError::not_found("Cleanup restore point was not found"));
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
        let restore_point_filename = entry
            .get("restorePointFilename")
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
        if [restore_point_filename, original_filename, cleaned_filename]
            .iter()
            .any(|name| validate_safe_segment(name, "restore point filename").is_err())
        {
            failed.push(json!({ "expression": expression, "error": "Restore point entry has an invalid filename" }));
            continue;
        }
        match fs::copy(
            restore_point_dir.join(restore_point_filename),
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
        let _ = fs::remove_dir_all(&restore_point_dir);
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
        self.generate_individually
    }
}

fn build_sprite_plan(body: &Value, connection: Option<&Value>) -> SpritePlan {
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
    let model = connection
        .and_then(|value| value.get("model"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let single_portrait =
        sprite_type != "full-body" && expressions.len() == 1 && cols == 1 && rows == 1;
    let single_full_body =
        sprite_type == "full-body" && expressions.len() == 1 && cols == 1 && rows == 1;
    let generate_individually =
        sprite_type != "full-body" && !single_portrait && is_openai_gpt_image_model(model);
    let (sheet_width, sheet_height, cell_width, cell_height) =
        resolve_canvas(cols, rows, &sprite_type, model);
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
    } else if single_full_body {
        single_full_body_prompt(
            &appearance,
            expressions.first().map(String::as_str).unwrap_or("idle"),
            body,
            model,
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
    } else if single_portrait {
        single_portrait_prompt(
            &appearance,
            expressions.first().map(String::as_str).unwrap_or("neutral"),
            body,
            model,
        )
    } else {
        expression_sheet_prompt(cols, rows, &expressions, &appearance, body, model)
    };
    let prompt = transparent_prompt(prompt, body, model);
    SpritePlan {
        expressions,
        appearance,
        cols,
        rows,
        sprite_type,
        generate_individually,
        model: model.to_string(),
        prompt,
        sheet_width,
        sheet_height,
    }
}

fn resolve_canvas(cols: u32, rows: u32, sprite_type: &str, model: &str) -> (u32, u32, u32, u32) {
    let cell_width = 512;
    let cell_height = if sprite_type == "full-body" { 768 } else { 512 };
    let requested_width = cols * cell_width;
    let requested_height = rows * cell_height;
    if is_openai_gpt_image_model(model) {
        let ratio = requested_width as f64 / requested_height.max(1) as f64;
        let (sheet_width, sheet_height) = if ratio > 1.12 {
            (1536, 1024)
        } else if ratio < 0.88 {
            (1024, 1536)
        } else {
            (1024, 1024)
        };
        return (
            sheet_width,
            sheet_height,
            sheet_width / cols.max(1),
            sheet_height / rows.max(1),
        );
    }
    (requested_width, requested_height, cell_width, cell_height)
}

fn format_sprite_label_for_prompt(label: &str) -> String {
    label.trim().replace(['_', '-'], " ")
}

fn is_gpt_image_2_model(model: &str) -> bool {
    let lower = model.trim().to_ascii_lowercase();
    lower == "gpt-image-2" || lower.starts_with("gpt-image-2-")
}

fn expression_sheet_prompt(
    cols: u32,
    rows: u32,
    expressions: &[String],
    appearance: &str,
    body: &Value,
    model: &str,
) -> String {
    let expression_list = expressions
        .iter()
        .map(|expression| format_sprite_label_for_prompt(expression))
        .collect::<Vec<_>>()
        .join(", ");
    let prompt = format!(
        "character expression sprite sheet source image, designed to be sliced into cells, EXACTLY {} total portrait cells and every cell must be filled, strict {cols} columns by {rows} rows grid, no extra rows, no extra columns, no extra panels, solid white background, thin straight borders or clean gutters separating every cell, same character in every cell, same outfit, same camera distance, same lighting, consistent art style, expressions left-to-right top-to-bottom, one cell per expression, no duplicates and none missing: {}, {appearance}, each cell shows one head-and-shoulders portrait with the requested facial expression, centered with no cropping, no text, no labels, no numbers, no captions, no watermark",
        expressions.len(),
        expression_list
    );
    transparent_prompt(prompt, body, model)
}

fn single_portrait_prompt(appearance: &str, expression: &str, body: &Value, model: &str) -> String {
    let expression = format_sprite_label_for_prompt(expression);
    transparent_prompt(format!("single character portrait sprite, one character only, head and shoulders portrait, centered in frame, no cropping, solid white studio background, {appearance}, facial expression: {expression}, anime/game sprite style, consistent character design, no grid, no panel borders, no text, no labels, no watermark"), body, model)
}

fn single_full_body_prompt(appearance: &str, pose: &str, body: &Value, model: &str) -> String {
    let pose = format_sprite_label_for_prompt(pose);
    transparent_prompt(format!("single full-body character sprite, one character only, entire body visible from head to toe, centered in frame, no cropping, solid white studio background, {appearance}, pose/action: {pose}, anime/game sprite style, consistent character design, no grid, no panel borders, no text, no labels, no watermark"), body, model)
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
    let cell_count = cols * rows;
    let readable = expressions
        .iter()
        .map(|expression| format_sprite_label_for_prompt(expression))
        .collect::<Vec<_>>();
    let filler_count = cell_count.saturating_sub(readable.len() as u32);
    [
        "full-body character pose sprite sheet source image, designed to be sliced into cells,".to_string(),
        format!("target output canvas is {sheet_width}x{sheet_height} pixels, with each cell exactly {cell_width}x{cell_height} pixels,"),
        format!("EXACTLY {cell_count} total grid cells and every cell must be filled, strict {cols} columns by {rows} rows grid,"),
        "all vertical grid cuts are evenly spaced, solid white background, thin straight borders or clean gutters separating every cell,".to_string(),
        "same character in every cell, same outfit, same proportions, same scale, consistent art style,".to_string(),
        format!("first {} cells left-to-right top-to-bottom must match these poses: {},", readable.len(), readable.join(", ")),
        if filler_count > 0 {
            format!("fill the remaining {filler_count} cells with neutral idle filler sprites; filler cells are ignored after slicing,")
        } else {
            String::new()
        },
        format!("{appearance},"),
        "each cell shows one complete full-body character from head to toe, centered upright, feet visible, no cropping,".to_string(),
        "the character must use no more than 78% of the cell height; leave padding above the head and below the feet inside every cell,".to_string(),
        "keep every sprite fully inside its own cell; no hair, feet, clothing, weapons, shadows, or effects may cross into another cell,".to_string(),
        "do not make one single large full-body image, do not make a poster, comic page, collage, diagonal layout, or merged composition,".to_string(),
        "all cells same size, perfectly aligned, no overlapping, no merged cells, no blank cells, no text, no labels, no numbers, no captions, no watermark".to_string(),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" ")
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
    let cell_count = cols * rows;
    let readable = expressions
        .iter()
        .map(|expression| format_sprite_label_for_prompt(expression))
        .collect::<Vec<_>>();
    let filler_count = cell_count.saturating_sub(readable.len() as u32);
    [
        "full-body character expression sprite sheet source image, designed to be sliced into cells,".to_string(),
        format!("target output canvas is {sheet_width}x{sheet_height} pixels, with each cell exactly {cell_width}x{cell_height} pixels,"),
        format!("strict {cols} columns by {rows} rows grid, exactly {cell_count} equally sized tall rectangular cells,"),
        format!("all vertical grid cuts are evenly spaced every {cell_width} pixels and all horizontal grid cuts every {cell_height} pixels,"),
        "solid white background, thin straight borders or clean gutters separating every cell,".to_string(),
        "same character in every cell, same outfit, same proportions, same scale, consistent art style,".to_string(),
        format!("first {} cells left-to-right top-to-bottom must match these facial expressions while keeping the same relaxed standing idle pose: {},", readable.len(), readable.join(", ")),
        if filler_count > 0 {
            format!("fill the remaining {filler_count} cells with neutral relaxed standing idle filler sprites; filler cells are ignored after slicing,")
        } else {
            String::new()
        },
        format!("{appearance},"),
        "each cell shows one complete full-body character from head to toe, centered upright, feet visible, no cropping,".to_string(),
        "the character must use no more than 78% of the cell height; leave at least 10% empty padding above the head and 12% empty padding below the feet inside every cell,".to_string(),
        "feet and shoes must be clearly above the bottom border or gutter, especially in the final row, never touching or cut by the cell edge,".to_string(),
        "keep every sprite fully inside its own cell; no hair, feet, clothing, weapons, shadows, or effects may cross into another cell,".to_string(),
        "only the face and mood change between the expression cells; body pose stays idle and relaxed,".to_string(),
        "do not create action, walking, running, attack, casting, combat, jumping, sitting, or victory poses,".to_string(),
        "do not make one single large full-body image, do not make a poster, comic page, collage, diagonal layout, or merged composition,".to_string(),
        "all cells same size, perfectly aligned, no overlapping, no merged cells, no blank cells, no text, no labels, no numbers, no captions, no watermark".to_string(),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" ")
}

fn single_sprite_prompt(
    sprite_type: &str,
    appearance: &str,
    expression: &str,
    body: &Value,
    model: &str,
) -> String {
    if sprite_type == "full-body" {
        single_full_body_prompt(appearance, expression, body, model)
    } else {
        single_portrait_prompt(appearance, expression, body, model)
    }
}

fn transparent_prompt(prompt: String, body: &Value, model: &str) -> String {
    if !body
        .get("nativeTransparentPng")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return prompt;
    }
    let cleanup_friendly = is_gpt_image_2_model(model);
    let replacement = if cleanup_friendly {
        "no background, png format. If transparent output is unsupported, use a perfectly flat pure white #ffffff background with no shadows, gradients, scenery, floor line, or texture behind the character"
    } else {
        "no background, png format"
    };
    let mut updated = prompt
        .replace("solid white studio background", replacement)
        .replace("solid white background", replacement)
        .replace("plain white background", replacement)
        .replace("white studio background", replacement)
        .replace("white background", replacement);
    if updated == prompt && !updated.to_ascii_lowercase().contains("no background") {
        updated.push_str(", ");
        updated.push_str(replacement);
    } else if cleanup_friendly
        && !updated.to_ascii_lowercase().contains("flat pure white")
        && updated.to_ascii_lowercase().contains("no background")
    {
        updated.push_str(". If transparent output is unsupported, use a perfectly flat pure white #ffffff background with no shadows, gradients, scenery, floor line, or texture behind the character");
    }
    updated
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
    let (width, height) = rgba.dimensions();
    if width == 0 || height == 0 {
        return DynamicImage::ImageRgba8(rgba);
    }

    let matte = estimate_border_matte(&rgba, strength);
    let pixel_count = (width as usize).saturating_mul(height as usize);
    let mut matte_mask = vec![false; pixel_count];
    let mut queue = VecDeque::new();
    let strength_f = strength.min(100) as f32;
    let hard_cutoff = 14.0 + (strength_f / 100.0) * 32.0;
    let soft_cutoff = hard_cutoff + 30.0 + (strength_f / 100.0) * 42.0;
    let halo_cutoff = soft_cutoff + 12.0 + (strength_f / 100.0) * 18.0;
    let matte_luma = rgb_luma(matte);
    let luma_floor = 178.0_f32.max(matte_luma - (30.0 + strength_f * 0.46));
    let spread_limit = 18.0 + (strength_f / 100.0) * 38.0;

    for x in 0..width {
        enqueue_matte_candidate(
            &rgba,
            &mut matte_mask,
            &mut queue,
            width,
            x,
            0,
            matte,
            soft_cutoff,
            luma_floor,
            spread_limit,
        );
        enqueue_matte_candidate(
            &rgba,
            &mut matte_mask,
            &mut queue,
            width,
            x,
            height - 1,
            matte,
            soft_cutoff,
            luma_floor,
            spread_limit,
        );
    }
    for y in 0..height {
        enqueue_matte_candidate(
            &rgba,
            &mut matte_mask,
            &mut queue,
            width,
            0,
            y,
            matte,
            soft_cutoff,
            luma_floor,
            spread_limit,
        );
        enqueue_matte_candidate(
            &rgba,
            &mut matte_mask,
            &mut queue,
            width,
            width - 1,
            y,
            matte,
            soft_cutoff,
            luma_floor,
            spread_limit,
        );
    }
    drain_matte_queue(
        &rgba,
        &mut matte_mask,
        &mut queue,
        width,
        height,
        matte,
        soft_cutoff,
        luma_floor,
        spread_limit,
    );

    let strict_cutoff = (hard_cutoff + 22.0).min(soft_cutoff * 0.88);
    let mut row_counts = vec![0u32; height as usize];
    let mut col_counts = vec![0u32; width as usize];
    for y in 0..height {
        for x in 0..width {
            let index = pixel_index(width, x, y);
            if matte_mask[index] {
                continue;
            }
            if is_matte_candidate(
                &rgba,
                x,
                y,
                matte,
                strict_cutoff,
                luma_floor + 10.0,
                spread_limit - 8.0,
            ) {
                row_counts[y as usize] += 1;
                col_counts[x as usize] += 1;
            }
        }
    }
    let broad_row_threshold = width as f32 * 0.34;
    let broad_col_threshold = height as f32 * 0.34;
    for y in 0..height {
        for x in 0..width {
            let index = pixel_index(width, x, y);
            if matte_mask[index] {
                continue;
            }
            if row_counts[y as usize] as f32 >= broad_row_threshold
                || col_counts[x as usize] as f32 >= broad_col_threshold
            {
                if is_matte_candidate(
                    &rgba,
                    x,
                    y,
                    matte,
                    strict_cutoff,
                    luma_floor + 10.0,
                    spread_limit - 8.0,
                ) {
                    matte_mask[index] = true;
                    queue.push_back(index);
                }
            }
        }
    }
    drain_matte_queue(
        &rgba,
        &mut matte_mask,
        &mut queue,
        width,
        height,
        matte,
        soft_cutoff,
        luma_floor,
        spread_limit,
    );

    let original = rgba.clone();
    for y in 0..height {
        for x in 0..width {
            let index = pixel_index(width, x, y);
            let pixel = rgba.get_pixel_mut(x, y);
            let Rgba([r, g, b, a]) = *pixel;
            if matte_mask[index] {
                *pixel = Rgba([r, g, b, 0]);
                continue;
            }
            let matte_neighbors = matte_neighbor_weight(&matte_mask, width, height, x, y);
            if matte_neighbors <= 0.0 {
                continue;
            }
            let distance = rgb_distance(read_rgb(&original, x, y), matte);
            if distance > halo_cutoff {
                continue;
            }
            let fade = 1.0
                - ((distance - hard_cutoff) / (halo_cutoff - hard_cutoff).max(1.0)).clamp(0.0, 1.0);
            let cleanup_weight = fade * (matte_neighbors / 3.2).clamp(0.0, 1.0);
            if cleanup_weight <= 0.0 {
                continue;
            }
            if let Some(neighbor) =
                foreground_neighbor_color(&original, &matte_mask, width, height, x, y)
            {
                let blend = (cleanup_weight * (0.55 + strength_f / 400.0)).clamp(0.0, 1.0);
                *pixel = Rgba([
                    lerp_u8(r, neighbor.0, blend),
                    lerp_u8(g, neighbor.1, blend),
                    lerp_u8(b, neighbor.2, blend),
                    ((a as f32) * (1.0 - cleanup_weight * (0.18 + strength_f / 280.0))).round()
                        as u8,
                ]);
            }
        }
    }
    DynamicImage::ImageRgba8(rgba)
}

fn pixel_index(width: u32, x: u32, y: u32) -> usize {
    (y as usize * width as usize) + x as usize
}

fn read_rgb(image: &RgbaImage, x: u32, y: u32) -> (u8, u8, u8) {
    let Rgba([r, g, b, _]) = *image.get_pixel(x, y);
    (r, g, b)
}

fn rgb_luma(color: (u8, u8, u8)) -> f32 {
    color.0 as f32 * 0.2126 + color.1 as f32 * 0.7152 + color.2 as f32 * 0.0722
}

fn rgb_spread(color: (u8, u8, u8)) -> f32 {
    let max = color.0.max(color.1).max(color.2) as f32;
    let min = color.0.min(color.1).min(color.2) as f32;
    max - min
}

fn rgb_distance(a: (u8, u8, u8), b: (u8, u8, u8)) -> f32 {
    ((a.0 as f32 - b.0 as f32).powi(2)
        + (a.1 as f32 - b.1 as f32).powi(2)
        + (a.2 as f32 - b.2 as f32).powi(2))
    .sqrt()
}

fn estimate_border_matte(image: &RgbaImage, strength: u8) -> (u8, u8, u8) {
    let (width, height) = image.dimensions();
    let step = (width.min(height) / 96).max(1);
    let mut red = Vec::new();
    let mut green = Vec::new();
    let mut blue = Vec::new();
    let mut accept = |x: u32, y: u32| {
        let Rgba([r, g, b, a]) = *image.get_pixel(x, y);
        if a <= 4 {
            return;
        }
        let color = (r, g, b);
        if rgb_luma(color) < 172.0 - strength as f32 * 0.18
            || rgb_spread(color) > 38.0 + strength as f32 * 0.3
        {
            return;
        }
        red.push(r);
        green.push(g);
        blue.push(b);
    };
    let mut x = 0;
    while x < width {
        accept(x, 0);
        accept(x, height - 1);
        x = x.saturating_add(step);
    }
    let mut y = 0;
    while y < height {
        accept(0, y);
        accept(width - 1, y);
        y = y.saturating_add(step);
    }
    (
        median_u8(red, 255),
        median_u8(green, 255),
        median_u8(blue, 255),
    )
}

fn median_u8(mut values: Vec<u8>, fallback: u8) -> u8 {
    if values.is_empty() {
        return fallback;
    }
    values.sort_unstable();
    values[values.len() / 2]
}

fn is_matte_candidate(
    image: &RgbaImage,
    x: u32,
    y: u32,
    matte: (u8, u8, u8),
    cutoff: f32,
    luma_floor: f32,
    spread_limit: f32,
) -> bool {
    let Rgba([r, g, b, a]) = *image.get_pixel(x, y);
    if a <= 4 {
        return true;
    }
    let color = (r, g, b);
    rgb_luma(color) >= luma_floor
        && rgb_spread(color) <= spread_limit.max(0.0)
        && rgb_distance(color, matte) <= cutoff
}

#[allow(clippy::too_many_arguments)]
fn enqueue_matte_candidate(
    image: &RgbaImage,
    matte_mask: &mut [bool],
    queue: &mut VecDeque<usize>,
    width: u32,
    x: u32,
    y: u32,
    matte: (u8, u8, u8),
    cutoff: f32,
    luma_floor: f32,
    spread_limit: f32,
) {
    let index = pixel_index(width, x, y);
    if matte_mask[index]
        || !is_matte_candidate(image, x, y, matte, cutoff, luma_floor, spread_limit)
    {
        return;
    }
    matte_mask[index] = true;
    queue.push_back(index);
}

#[allow(clippy::too_many_arguments)]
fn drain_matte_queue(
    image: &RgbaImage,
    matte_mask: &mut [bool],
    queue: &mut VecDeque<usize>,
    width: u32,
    height: u32,
    matte: (u8, u8, u8),
    cutoff: f32,
    luma_floor: f32,
    spread_limit: f32,
) {
    while let Some(index) = queue.pop_front() {
        let x = (index % width as usize) as u32;
        let y = (index / width as usize) as u32;
        if x > 0 {
            enqueue_matte_candidate(
                image,
                matte_mask,
                queue,
                width,
                x - 1,
                y,
                matte,
                cutoff,
                luma_floor,
                spread_limit,
            );
        }
        if x + 1 < width {
            enqueue_matte_candidate(
                image,
                matte_mask,
                queue,
                width,
                x + 1,
                y,
                matte,
                cutoff,
                luma_floor,
                spread_limit,
            );
        }
        if y > 0 {
            enqueue_matte_candidate(
                image,
                matte_mask,
                queue,
                width,
                x,
                y - 1,
                matte,
                cutoff,
                luma_floor,
                spread_limit,
            );
        }
        if y + 1 < height {
            enqueue_matte_candidate(
                image,
                matte_mask,
                queue,
                width,
                x,
                y + 1,
                matte,
                cutoff,
                luma_floor,
                spread_limit,
            );
        }
    }
}

fn matte_neighbor_weight(matte_mask: &[bool], width: u32, height: u32, x: u32, y: u32) -> f32 {
    let mut weight = 0.0;
    for y_offset in -1..=1 {
        for x_offset in -1..=1 {
            if x_offset == 0 && y_offset == 0 {
                continue;
            }
            let Some(sample_x) = x.checked_add_signed(x_offset) else {
                continue;
            };
            let Some(sample_y) = y.checked_add_signed(y_offset) else {
                continue;
            };
            if sample_x >= width || sample_y >= height {
                continue;
            }
            if matte_mask[pixel_index(width, sample_x, sample_y)] {
                weight += if x_offset == 0 || y_offset == 0 {
                    1.0
                } else {
                    0.7
                };
            }
        }
    }
    weight
}

fn foreground_neighbor_color(
    image: &RgbaImage,
    matte_mask: &[bool],
    width: u32,
    height: u32,
    x: u32,
    y: u32,
) -> Option<(u8, u8, u8)> {
    let mut red = 0.0;
    let mut green = 0.0;
    let mut blue = 0.0;
    let mut total = 0.0;
    for radius in 1..=4 {
        for y_offset in -(radius as i32)..=(radius as i32) {
            for x_offset in -(radius as i32)..=(radius as i32) {
                let Some(sample_x) = x.checked_add_signed(x_offset) else {
                    continue;
                };
                let Some(sample_y) = y.checked_add_signed(y_offset) else {
                    continue;
                };
                if sample_x >= width || sample_y >= height {
                    continue;
                }
                if matte_mask[pixel_index(width, sample_x, sample_y)] {
                    continue;
                }
                let Rgba([r, g, b, a]) = *image.get_pixel(sample_x, sample_y);
                if a <= 16 {
                    continue;
                }
                let distance = ((x_offset * x_offset + y_offset * y_offset) as f32)
                    .sqrt()
                    .max(1.0);
                let weight = (a as f32 / 255.0) / distance;
                red += r as f32 * weight;
                green += g as f32 * weight;
                blue += b as f32 * weight;
                total += weight;
            }
        }
        if total > 0.0 {
            break;
        }
    }
    (total > 0.0).then(|| {
        (
            (red / total).round().clamp(0.0, 255.0) as u8,
            (green / total).round().clamp(0.0, 255.0) as u8,
            (blue / total).round().clamp(0.0, 255.0) as u8,
        )
    })
}

fn lerp_u8(from: u8, to: u8, amount: f32) -> u8 {
    (from as f32 + (to as f32 - from as f32) * amount)
        .round()
        .clamp(0.0, 255.0) as u8
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
