use super::{blocking, require_project, AppState};
use crate::{
    ai::{protocol::Prediction, suggestions, worker::Config},
    domain::{annotation::Document, Result},
    services::{annotation as a, library, preview},
};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};
use tauri::{AppHandle, Manager, State};

pub(super) fn directory(app: &AppHandle) -> Result<PathBuf> {
    if let Some(root) = std::env::var_os("LABELFLOW_CONFIG_DIR") {
        return Ok(PathBuf::from(root));
    }
    app.path()
        .app_config_dir()
        .map_err(|e| a::invalid(e.to_string()))
}
fn config(app: &AppHandle) -> Result<Option<Config>> {
    read_config(app, "ai.json")
}
pub(super) fn read_config(app: &AppHandle, name: &str) -> Result<Option<Config>> {
    let path = directory(app)?.join(name);
    if !path.exists() {
        return Ok(None);
    }
    crate::security::reject_symlink(&path)?;
    if fs::metadata(&path)?.len() > 64 * 1024 {
        return Err(a::invalid("AI settings exceed size limit."));
    }
    Ok(Some(a::parse(&fs::read_to_string(path)?)?))
}
#[tauri::command]
pub async fn ai_status(state: State<'_, AppState>, app: AppHandle) -> Result<Value> {
    Ok(json!({"config":config(&app)?,"runtime":state.ai.status()}))
}
#[tauri::command]
pub async fn ai_configure(state: State<'_, AppState>, app: AppHandle) -> Result<Option<Config>> {
    let runtime = state.ai.clone();
    tauri::async_runtime::spawn_blocking(move || {
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
        let Some(python) = dialog
            .set_title("Choose Python executable from your AI environment (runs local code)")
            .pick_file()
        else {
            return Ok(None);
        };
        let Some(manifest) = rfd::FileDialog::new()
            .set_title("Choose local ONNX model manifest")
            .add_filter("LabelFlow model manifest", &["json"])
            .pick_file()
        else {
            return Ok(None);
        };
        let value = Config {
            python: crate::ai::worker::normalize_python_path(&python)?,
            manifest: manifest.canonicalize()?,
        };
        runtime.stop()?;
        let root = directory(&app)?;
        fs::create_dir_all(&root)?;
        let path = root.join("ai.json");
        crate::security::reject_symlink(&path)?;
        let temporary = root.join(format!("ai-{}.tmp", uuid::Uuid::new_v4()));
        a::atomic_file(&temporary, a::json(&value)?.as_bytes())?;
        fs::rename(temporary, path)?;
        Ok(Some(value))
    })
    .await
    .map_err(|e| a::invalid(e.to_string()))?
}
#[tauri::command]
pub async fn ai_start(state: State<'_, AppState>, app: AppHandle) -> Result<Value> {
    let value = config(&app)?.ok_or_else(|| {
        a::invalid("Choose Python and a model manifest first. Manual labeling does not require AI.")
    })?;
    let root = directory(&app)?;
    let runtime = state.ai.clone();
    let token = runtime.token();
    let result = tauri::async_runtime::spawn_blocking(move || runtime.start(&value, &root, token))
        .await
        .map_err(|e| a::invalid(e.to_string()))??;
    let model = state.ai.model()?;
    let validation = blocking(&state, move |session| {
        suggestions::validate_model(require_project(session)?, &model)
    })
    .await;
    if let Err(error) = validation {
        state.ai.stop()?;
        return Err(error);
    }
    Ok(result)
}
#[tauri::command]
pub async fn ai_stop(state: State<'_, AppState>) -> Result<()> {
    let runtime = state.ai.clone();
    runtime.cancel();
    tauri::async_runtime::spawn_blocking(move || runtime.stop())
        .await
        .map_err(|e| a::invalid(e.to_string()))?
}
#[tauri::command]
pub async fn ai_cancel(state: State<'_, AppState>) -> Result<()> {
    state.ai.cancel();
    Ok(())
}

#[tauri::command]
pub async fn ai_predict(
    state: State<'_, AppState>,
    media_id: String,
    threshold: f64,
) -> Result<String> {
    if !threshold.is_finite() || !(0. ..=1.).contains(&threshold) {
        return Err(a::invalid("Invalid AI score threshold."));
    }
    let runtime = state.ai.clone();
    let token = runtime.token();
    let model = runtime.model()?;
    let version = model.version.clone();
    let (project,revision,sha,args)=blocking(&state,move |session| {
        let s=require_project(session)?;
        suggestions::validate_model(s,&model)?;
        let doc=a::load(s,&media_id)?;
        let (info,path)=preview::cached(s,&media_id)?;
        Ok((s.project.id.clone(),doc.revision,info.sha256.clone(),json!({"mediaId":media_id,"imagePath":path,"width":info.width,"height":info.height,"orientation":1,"sourceOrientation":info.orientation,"sourceSha256":info.sha256,"modelVersion":version,"parameters":{"threshold":threshold}})))
    }).await?;
    let media = args["mediaId"]
        .as_str()
        .ok_or_else(|| a::invalid("Missing AI media ID."))?
        .to_owned();
    let result = tauri::async_runtime::spawn_blocking(move || runtime.predict(args, token))
        .await
        .map_err(|e| a::invalid(e.to_string()))??;
    let runtime = state.ai.clone();
    blocking(&state, move |session| {
        runtime.check(token)?;
        let s = require_project(session)?;
        if s.project.id != project {
            return Err(a::invalid("Project changed during AI inference."));
        }
        let record = library::record(s, &media)?;
        if crate::video::source_sha(&library::resolve(s, &record.path)?)? != sha {
            return Err(a::invalid("Original image changed during AI inference."));
        }
        let prediction: Prediction =
            serde_json::from_value(result.1).map_err(|e| a::invalid(e.to_string()))?;
        runtime.check(token)?;
        suggestions::store(s, &media, revision, &sha, prediction, &result.0)
    })
    .await
}
#[tauri::command]
pub async fn ai_suggestions(
    state: State<'_, AppState>,
    media_id: String,
) -> Result<Vec<suggestions::Summary>> {
    blocking(&state, move |session| {
        suggestions::list(require_project(session)?, &media_id)
    })
    .await
}
#[tauri::command]
pub async fn ai_preview(state: State<'_, AppState>, id: String) -> Result<suggestions::Preview> {
    blocking(&state, move |session| {
        suggestions::preview(require_project(session)?, &id)
    })
    .await
}
#[tauri::command]
pub async fn ai_reject(state: State<'_, AppState>, id: String) -> Result<()> {
    blocking(&state, move |session| {
        suggestions::reject(require_project(session)?, &id)
    })
    .await
}
#[tauri::command]
pub async fn ai_approve(
    state: State<'_, AppState>,
    id: String,
    expected_revision: u32,
) -> Result<Document> {
    blocking(&state, move |session| {
        suggestions::approve(require_project(session)?, &id, expected_revision)
    })
    .await
}
