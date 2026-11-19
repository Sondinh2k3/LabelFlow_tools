use super::*;
use crate::{
    domain::video::*,
    video::{self, tools::Toolchain},
};
use serde::Serialize;
pub(super) async fn job<T: Send + 'static>(
    state: &AppState,
    action: impl FnOnce(&mut ProjectSession, &AtomicBool) -> Result<T> + Send + 'static,
) -> Result<T> {
    if state.job_active.swap(true, Ordering::SeqCst) {
        return Err(annotation::invalid("Another job is active."));
    }
    state.cancel.store(false, Ordering::Relaxed);
    let cancel = state.cancel.clone();
    let result = blocking(state, move |s| action(require_project(s)?, &cancel)).await;
    state.job_active.store(false, Ordering::SeqCst);
    result
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    available: bool,
    tools: Option<Toolchain>,
    version: Option<String>,
    error: Option<String>,
}
#[tauri::command]
pub async fn video_tools(app: AppHandle) -> Result<ToolStatus> {
    tauri::async_runtime::spawn_blocking(move || {
        let result =
            video::tools::for_app(&app).and_then(|tools| Ok((tools.clone(), tools.validate()?)));
        Ok(match result {
            Ok((tools, version)) => ToolStatus {
                available: true,
                tools: Some(tools),
                version: Some(version),
                error: None,
            },
            Err(e) => ToolStatus {
                available: false,
                tools: None,
                version: None,
                error: Some(e.to_string()),
            },
        })
    })
    .await
    .map_err(|e| annotation::invalid(e.to_string()))?
}
#[tauri::command]
pub async fn configure_video_tools(app: AppHandle) -> Result<Option<Toolchain>> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Choose folder containing ffmpeg and ffprobe executables")
            .pick_folder()
        else {
            return Ok(None);
        };
        let tools = Toolchain::from_folder(&path)?;
        tools.validate()?;
        video::tools::remember(&app, &tools)?;
        Ok(Some(tools))
    })
    .await
    .map_err(|e| annotation::invalid(e.to_string()))?
}
#[tauri::command]
pub async fn open_video(state: State<'_, AppState>, app: AppHandle) -> Result<Option<VideoInfo>> {
    job(&state, move |s, cancel| {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Video", &["mp4", "mov", "mkv", "webm", "avi"])
            .pick_file()
        else {
            return Ok(None);
        };
        video::import(s, &video::tools::for_app(&app)?, &path, cancel).map(Some)
    })
    .await
}
#[tauri::command]
pub async fn list_videos(state: State<'_, AppState>) -> Result<Vec<VideoInfo>> {
    blocking(&state, |s| video::list(require_project(s)?)).await
}
#[tauri::command]
pub async fn video_frames(
    state: State<'_, AppState>,
    id: String,
    offset: u32,
    limit: u32,
) -> Result<VideoPage> {
    blocking(&state, move |s| {
        video::frames(require_project(s)?, &id, offset, limit)
    })
    .await
}
#[tauri::command]
pub async fn seek_video(state: State<'_, AppState>, id: String, time_ms: f64) -> Result<FrameInfo> {
    blocking(&state, move |s| {
        video::seek(require_project(s)?, &id, time_ms)
    })
    .await
}
#[tauri::command]
pub async fn video_stream(state: State<'_, AppState>, id: String) -> Result<String> {
    let resources = state.resources.clone();
    blocking(&state, move |s| {
        let video = video::get(require_project(s)?, &id)?;
        if video::source_sha(std::path::Path::new(&video.path))? != video.sha256 {
            return Err(annotation::invalid("Video source changed."));
        }
        let mime = if video.path.ends_with(".webm") {
            "video/webm"
        } else {
            "video/mp4"
        };
        Ok(resources.register(video.path.into(), mime))
    })
    .await
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FramePreview {
    frame: FrameInfo,
    data_url: String,
}
#[tauri::command]
pub async fn preview_frame(
    state: State<'_, AppState>,
    app: AppHandle,
    id: String,
    index: u32,
) -> Result<FramePreview> {
    let cancel = Arc::new(AtomicBool::new(false));
    if let Some(old) = state
        .preview_cancel
        .lock()
        .map_err(|_| annotation::invalid("Preview state unavailable."))?
        .replace(cancel.clone())
    {
        old.store(true, Ordering::Relaxed);
    }
    let resources = state.resources.clone();
    blocking(&state, move |s| {
        let s = require_project(s)?;
        let video = video::get(s, &id)?;
        let frame = video::frame(s, &id, index)?;
        let directory = crate::services::storage::CacheKind::Video.directory(s)?;
        crate::security::reject_symlink(&directory)?;
        std::fs::create_dir_all(&directory)?;
        let path = directory.join(format!("{id}-{index}.png"));
        crate::security::reject_symlink(&path)?;
        let mut added = 0;
        if !path.is_file() {
            let bytes = video::decode(
                &video::tools::for_app(&app)?,
                &video,
                &frame,
                "png",
                100,
                &cancel,
            )?;
            annotation::atomic_file(&path, &bytes)?;
            added = bytes.len() as u64;
        }
        if cancel.load(Ordering::Relaxed) {
            return Err(AppError::new(ErrorCode::Cancelled, "Preview cancelled."));
        }
        crate::services::storage::trim_cache(
            s,
            crate::services::storage::CacheKind::Video,
            &path,
            added,
        )?;
        Ok(FramePreview {
            frame,
            data_url: resources.register(path, "image/png"),
        })
    })
    .await
}
#[tauri::command]
pub async fn extract_frames(
    state: State<'_, AppState>,
    app: AppHandle,
    options: ExtractionOptions,
) -> Result<ImportReport> {
    job(&state, move |s, cancel| {
        video::extract(s, &video::tools::for_app(&app)?, &options, cancel, |p| {
            let _ = app.emit("job-progress", p);
        })
    })
    .await
}
#[tauri::command]
pub async fn set_video_group(state: State<'_, AppState>, id: String, group: String) -> Result<()> {
    if group.trim().is_empty() || group.len() > 512 {
        return Err(annotation::invalid(
            "Video/trip group must contain 1–512 UTF-8 bytes.",
        ));
    }
    blocking(&state, move |s| {
        let s = require_project(s)?;
        video::get(s, &id)?;
        let assigned:bool=s.connection.query_row("SELECT trip_id IS NOT NULL FROM videos WHERE id=?1",[&id],|r|r.get(0))?;
        if assigned {return Err(annotation::invalid("This video's split group is managed by its assigned trip. Use Road / dataset review to change the trip."));}
        let tx = s.connection.transaction()?;
        tx.execute(
            "UPDATE videos SET split_group_id=?1 WHERE id=?2",
            rusqlite::params![group, id],
        )?;
        tx.execute(
            "UPDATE media SET split_group_id=?1 WHERE source_video_id=?2",
            rusqlite::params![group, id],
        )?;
        tx.commit()?;
        Ok(())
    })
    .await
}
#[tauri::command]
pub async fn inspect_instances(
    state: State<'_, AppState>,
    format: String,
) -> Result<Option<crate::import::instances::Inspection>> {
    if !["coco", "yoloSeg"].contains(&format.as_str()) {
        return Err(annotation::invalid("Unknown instance import format."));
    }
    let prepared = blocking(&state, move |s| {
        let s = require_project(s)?;
        if format == "coco" {
            let Some(path) = rfd::FileDialog::new()
                .add_filter("COCO JSON", &["json"])
                .pick_file()
            else {
                return Ok(None);
            };
            crate::import::instances::inspect_coco(s, &path).map(Some)
        } else {
            let Some(path) = rfd::FileDialog::new()
                .set_title("Choose YOLO segmentation labels folder")
                .pick_folder()
            else {
                return Ok(None);
            };
            crate::import::instances::inspect_yolo_seg(s, &path).map(Some)
        }
    })
    .await?;
    let inspection = prepared.as_ref().map(|p| p.inspection());
    *state
        .instance_import
        .lock()
        .map_err(|_| annotation::invalid("Import state unavailable."))? = prepared;
    Ok(inspection)
}
#[tauri::command]
pub async fn apply_instances(
    state: State<'_, AppState>,
    app: AppHandle,
    token: String,
    mapping: Vec<crate::import::instances::CategoryMapping>,
) -> Result<ImportReport> {
    let prepared = state
        .instance_import
        .lock()
        .map_err(|_| annotation::invalid("Import state unavailable."))?
        .take()
        .ok_or_else(|| annotation::invalid("Inspect the dataset first."))?;
    if prepared.token != token {
        return Err(annotation::invalid("Stale import preview."));
    }
    job(&state, move |s, cancel| {
        crate::import::instances::apply(s, prepared, &mapping, cancel, |p| {
            let _ = app.emit("job-progress", p);
        })
    })
    .await
}
#[tauri::command]
pub async fn dataset_quality(
    state: State<'_, AppState>,
    app: AppHandle,
    options: ExportOptions,
    deep: bool,
) -> Result<crate::services::quality::QualityReport> {
    job(&state, move |s, cancel| {
        crate::services::quality::inspect(s, &options, deep, cancel, |p| {
            if deep {
                let _ = app.emit("job-progress", p);
            }
        })
    })
    .await
}
#[tauri::command]
pub async fn repair_media(state: State<'_, AppState>, id: String) -> Result<bool> {
    blocking(&state, move |s| {
        let s = require_project(s)?;
        let Some(path) = rfd::FileDialog::new()
            .set_title("Locate the exact original image (SHA-256 must match)")
            .add_filter("Image", &["png", "jpg", "jpeg", "webp"])
            .pick_file()
        else {
            return Ok(false);
        };
        crate::services::quality::repair(s, &id, &path)?;
        Ok(true)
    })
    .await
}
#[tauri::command]
pub async fn media_origin(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<serde_json::Value>> {
    blocking(&state, move |s| {
        crate::services::quality::origin(require_project(s)?, &id)
    })
    .await
}
#[tauri::command]
pub async fn preload_media(state: State<'_, AppState>, id: String) -> Result<ImagePreview> {
    let resources = state.resources.clone();
    blocking(&state, move |s| {
        let (info, path) = crate::services::preview::cached(require_project(s)?, &id)?;
        Ok(ImagePreview {
            info,
            data_url: resources.register(path, "image/png"),
        })
    })
    .await
}

#[tauri::command]
pub async fn repair_video(state: State<'_, AppState>, id: String) -> Result<bool> {
    blocking(&state, move |s| {
        let s = require_project(s)?;
        let Some(path) = rfd::FileDialog::new()
            .set_title("Locate the exact original video (SHA-256 must match)")
            .add_filter("Video", &["mp4", "mov", "mkv", "webm", "avi"])
            .pick_file()
        else {
            return Ok(false);
        };
        video::repair(s, &id, &path)?;
        Ok(true)
    })
    .await
}
