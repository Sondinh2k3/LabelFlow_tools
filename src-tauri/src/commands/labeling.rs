//! Fast-labeling commands: batch pre-label masks and fixed ignore templates.
use super::{blocking, m2::job, require_project, AppState};
use crate::{ai::worker::Config, services::preview};
use crate::{
    domain::{
        annotation::{ImportReport, PixelMapping, Progress},
        Result,
    },
    import::masks::{self, MaskFolderInspection, MergeMode, TemplateInspection},
    services::annotation,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use tauri::{AppHandle, Emitter, State};
fn notify(app: &AppHandle, p: Progress) {
    let _ = app.emit("job-progress", p);
}
#[tauri::command]
pub async fn inspect_mask_folder(
    state: State<'_, AppState>,
    app: AppHandle,
    layer_id: String,
) -> Result<Option<MaskFolderInspection>> {
    job(&state, move |s, cancel| {
        let Some(folder) = rfd::FileDialog::new()
            .set_title("Choose a folder of class-ID PNG masks named after the images")
            .pick_folder()
        else {
            return Ok(None);
        };
        let (prepared, inspection) =
            masks::inspect_folder(s, &folder, &layer_id, cancel, |p| notify(&app, p))?;
        s.pending_mask_folder = Some(prepared);
        Ok(Some(inspection))
    })
    .await
}
#[tauri::command]
pub async fn apply_mask_folder(
    state: State<'_, AppState>,
    app: AppHandle,
    token: String,
    mapping: Vec<PixelMapping>,
    mode: MergeMode,
) -> Result<ImportReport> {
    job(&state, move |s, cancel| {
        let prepared = s
            .pending_mask_folder
            .take()
            .filter(|p| p.token == token)
            .ok_or_else(|| annotation::invalid("Inspect the mask folder again."))?;
        masks::apply_folder(s, prepared, &mapping, mode, cancel, |p| notify(&app, p))
    })
    .await
}
#[tauri::command]
pub async fn inspect_ignore_template(
    state: State<'_, AppState>,
    trip_id: Option<String>,
) -> Result<Option<TemplateInspection>> {
    blocking(&state, move |session| {
        let s = require_project(session)?;
        let Some(path) = rfd::FileDialog::new()
            .set_title("Choose an ignore template PNG (white = ignore)")
            .add_filter("Template mask", &["png"])
            .pick_file()
        else {
            return Ok(None);
        };
        let (prepared, inspection) = masks::inspect_template(s, &path, trip_id.as_deref())?;
        s.pending_template = Some(prepared);
        Ok(Some(inspection))
    })
    .await
}
#[tauri::command]
pub async fn apply_ignore_template(
    state: State<'_, AppState>,
    app: AppHandle,
    token: String,
) -> Result<ImportReport> {
    job(&state, move |s, cancel| {
        let prepared = s
            .pending_template
            .take()
            .filter(|p| p.token == token)
            .ok_or_else(|| annotation::invalid("Choose the template again."))?;
        masks::apply_template(s, prepared, cancel, |p| notify(&app, p))
    })
    .await
}

// ---- Click-to-mask (SAM) -------------------------------------------------------------------

const SAM_CONFIG: &str = "sam.json";

#[tauri::command]
pub async fn sam_status(state: State<'_, AppState>, app: AppHandle) -> Result<Value> {
    Ok(json!({
        "config": super::m3::read_config(&app, SAM_CONFIG)?,
        "model": state.sam.sam_model(),
    }))
}
#[tauri::command]
pub async fn sam_configure(state: State<'_, AppState>, app: AppHandle) -> Result<Option<Config>> {
    let runtime = state.sam.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Reuse the Python already chosen for SAM or AI suggestions; ask only if none.
        let known = super::m3::read_config(&app, SAM_CONFIG)
            .ok()
            .flatten()
            .or_else(|| super::m3::read_config(&app, "ai.json").ok().flatten())
            .map(|c| c.python)
            .and_then(|p| crate::ai::worker::normalize_python_path(&p).ok());
        let python = match known {
            Some(p) => p,
            None => {
                let dialog = {
                    #[cfg(windows)]
                    {
                        rfd::FileDialog::new().add_filter("Python executable", &["exe"])
                    }
                    #[cfg(not(windows))]
                    {
                        rfd::FileDialog::new()
                    }
                };
                match dialog
                    .set_title("Choose Python executable from your AI environment (runs local code)")
                    .pick_file()
                {
                    Some(p) => crate::ai::worker::normalize_python_path(&p)?,
                    None => return Ok(None),
                }
            }
        };
        let Some(manifest) = rfd::FileDialog::new()
            .set_title("Choose SAM model manifest (kind: sam_prompt)")
            .add_filter("LabelFlow SAM manifest", &["json"])
            .pick_file()
        else {
            return Ok(None);
        };
        let value = Config {
            python,
            manifest: manifest.canonicalize()?,
        };
        runtime.stop()?;
        let root = super::m3::directory(&app)?;
        fs::create_dir_all(&root)?;
        let path = root.join(SAM_CONFIG);
        crate::security::reject_symlink(&path)?;
        let temporary = root.join(format!("sam-{}.tmp", uuid::Uuid::new_v4()));
        annotation::atomic_file(&temporary, annotation::json(&value)?.as_bytes())?;
        fs::rename(temporary, path)?;
        Ok(Some(value))
    })
    .await
    .map_err(|e| annotation::invalid(e.to_string()))?
}
#[tauri::command]
pub async fn sam_start(state: State<'_, AppState>, app: AppHandle) -> Result<Value> {
    let config = super::m3::read_config(&app, SAM_CONFIG)?
        .ok_or_else(|| annotation::invalid("Choose a SAM model first."))?;
    let root = super::m3::directory(&app)?;
    let runtime = state.sam.clone();
    let token = runtime.token();
    tauri::async_runtime::spawn_blocking(move || runtime.start_sam(&config, &root, token))
        .await
        .map_err(|e| annotation::invalid(e.to_string()))?
}
#[tauri::command]
pub async fn sam_stop(state: State<'_, AppState>) -> Result<()> {
    let runtime = state.sam.clone();
    runtime.cancel();
    tauri::async_runtime::spawn_blocking(move || runtime.stop())
        .await
        .map_err(|e| annotation::invalid(e.to_string()))?
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SamPoint {
    x: f64,
    y: f64,
    positive: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SamBox {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}
/// Encodes the image once (cached in the worker) and decodes the prompt into a binary mask
/// at canonical resolution, returned as value/count runs. Nothing is saved: the editor
/// applies the mask as an ordinary, undoable edit.
#[tauri::command]
pub async fn sam_segment(
    state: State<'_, AppState>,
    media_id: String,
    points: Vec<SamPoint>,
    region: Option<SamBox>,
    encode_only: bool,
) -> Result<Value> {
    if points.len() > 64 {
        return Err(annotation::invalid("At most 64 SAM points."));
    }
    let (info, path) = blocking(&state, move |session| {
        preview::cached(require_project(session)?, &media_id)
    })
    .await?;
    let (width, height) = (info.width, info.height);
    let inside = |v: f64, max: u32| v.is_finite() && (0.0..=f64::from(max)).contains(&v);
    if points
        .iter()
        .any(|p| !inside(p.x, width) || !inside(p.y, height))
        || region.as_ref().is_some_and(|b| {
            !inside(b.x, width)
                || !inside(b.y, height)
                || !inside(b.x + b.width, width)
                || !inside(b.y + b.height, height)
        })
    {
        return Err(annotation::invalid("SAM prompt is outside the image."));
    }
    let args = json!({
        "imagePath": path,
        "width": width,
        "height": height,
        "sourceSha256": info.sha256,
        "encodeOnly": encode_only,
        "points": points.iter().map(|p| json!({"x": p.x, "y": p.y, "positive": p.positive})).collect::<Vec<_>>(),
        "box": region.as_ref().map(|b| json!({"x": b.x, "y": b.y, "width": b.width, "height": b.height})),
    });
    let runtime = state.sam.clone();
    let token = runtime.token();
    let value = tauri::async_runtime::spawn_blocking(move || runtime.sam(args, token))
        .await
        .map_err(|e| annotation::invalid(e.to_string()))??;
    if encode_only {
        return Ok(json!({"cached": value["cached"]}));
    }
    // Validate the untrusted worker output before it reaches the editor.
    let runs = value["runs"]
        .as_array()
        .ok_or_else(|| annotation::invalid("SAM returned no mask."))?;
    let mut total = 0u64;
    let mut clean = Vec::with_capacity(runs.len());
    for pair in runs.chunks(2) {
        let (Some(v), Some(n)) = (
            pair.first().and_then(Value::as_u64),
            pair.get(1).and_then(Value::as_u64),
        ) else {
            return Err(annotation::invalid("Malformed SAM mask."));
        };
        if v > 1 || n == 0 {
            return Err(annotation::invalid("Malformed SAM mask."));
        }
        total += n;
        clean.push(v);
        clean.push(n);
    }
    if total != u64::from(width) * u64::from(height)
        || value["width"] != json!(width)
        || value["height"] != json!(height)
    {
        return Err(annotation::invalid(
            "SAM mask size disagrees with the image.",
        ));
    }
    Ok(json!({
        "width": width,
        "height": height,
        "runs": clean,
        "score": value["score"].as_f64(),
        "area": value["area"].as_u64(),
        "milliseconds": value["milliseconds"].as_f64(),
    }))
}
