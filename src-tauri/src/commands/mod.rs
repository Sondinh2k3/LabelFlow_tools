use crate::{
    domain::annotation::*,
    services::{annotation, interchange, library, profile},
};
pub mod labeling;
pub mod m2;
pub mod m3;
pub mod m4;
pub mod m5;
pub mod resources;
pub mod team;
use crate::{
    domain::{AppError, ErrorCode, ImagePreview, Project, Result},
    services::{
        media,
        project::{self, ProjectSession},
    },
};
use std::sync::{Arc, Mutex};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};
use tauri::State;
use tauri::{AppHandle, Emitter};

#[derive(Default)]
pub struct AppState {
    ai: Arc<crate::ai::worker::Runtime>,
    sam: Arc<crate::ai::worker::Runtime>,
    session: Arc<Mutex<Option<ProjectSession>>>,
    pub dropped: Mutex<HashSet<PathBuf>>,
    cancel: Arc<AtomicBool>,
    job_active: Arc<AtomicBool>,
    startup_error: Mutex<Option<AppError>>,
    pub resources: Arc<crate::security::resources::Resources>,
    instance_import: Mutex<Option<crate::import::instances::Prepared>>,
    preview_cancel: Mutex<Option<Arc<AtomicBool>>>,
    pub team: Arc<crate::services::team::Cache>,
}
impl AppState {
    pub fn initialize(&self, path: Option<PathBuf>) {
        if let Some(path) = path {
            match project::open(&path) {
                Ok(opened) => {
                    if let Ok(mut current) = self.session.lock() {
                        *current = Some(opened);
                    }
                }
                Err(error) => {
                    if let Ok(mut slot) = self.startup_error.lock() {
                        *slot = Some(error);
                    }
                }
            }
        }
    }
}
#[tauri::command]
pub async fn current_project(state: State<'_, AppState>) -> Result<Option<Project>> {
    if let Some(error) = state
        .startup_error
        .lock()
        .map_err(|_| annotation::invalid("Startup state unavailable."))?
        .take()
    {
        return Err(error);
    }
    blocking(&state, |session| {
        Ok(session.as_ref().map(|s| s.project.clone()))
    })
    .await
}

async fn blocking<T, F>(state: &AppState, action: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(&mut Option<ProjectSession>) -> Result<T> + Send + 'static,
{
    let shared = Arc::clone(&state.session);
    tauri::async_runtime::spawn_blocking(move || {
        let mut session = shared.lock().map_err(|_| {
            AppError::new(
                ErrorCode::Internal,
                "Project state is unavailable. Restart LabelFlow.",
            )
        })?;
        action(&mut session)
    })
    .await
    .map_err(|e| {
        AppError::new(
            ErrorCode::Internal,
            format!("Background operation failed: {e}"),
        )
    })?
}
fn require_closed(session: &Option<ProjectSession>) -> Result<()> {
    if session.is_some() {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "Close the current project before opening another.",
        ));
    }
    Ok(())
}
fn require_project(session: &mut Option<ProjectSession>) -> Result<&mut ProjectSession> {
    session
        .as_mut()
        .ok_or_else(|| AppError::new(ErrorCode::NoProject, "Create or open a project first."))
}

#[tauri::command]
pub async fn create_project(
    state: State<'_, AppState>,
    app: AppHandle,
    name: String,
    profile_id: Option<String>,
    semantic_classes: Option<Vec<String>>,
) -> Result<Option<Project>> {
    let name = project::validate_name(&name)?;
    if let Some(lines) = &semantic_classes {
        crate::services::profile::custom_semantic(lines)?;
    }
    blocking(&state, move |session| {
        require_closed(session)?;
        let Some(root) = rfd::FileDialog::new()
            .set_title("Choose project folder")
            .pick_folder()
        else {
            return Ok(None);
        };
        let opened = project::create_with_classes(
            &root,
            &name,
            profile_id.as_deref().unwrap_or("generic-v1"),
            semantic_classes.as_deref(),
        )?;
        let result = opened.project.clone();
        crate::services::session_settings::remember(&app, Some(&opened.root));
        *session = Some(opened);
        Ok(Some(result))
    })
    .await
}

#[tauri::command]
pub async fn project_profile(state: State<'_, AppState>) -> Result<Profile> {
    blocking(&state, |session| {
        profile::current(require_project(session)?)
    })
    .await
}
#[tauri::command]
pub async fn save_class(
    state: State<'_, AppState>,
    id: Option<String>,
    name: String,
    color: String,
    namespace: Option<String>,
) -> Result<Profile> {
    blocking(&state, move |session| {
        profile::save_class(
            require_project(session)?,
            namespace.as_deref(),
            id.as_deref(),
            &name,
            &color,
        )
    })
    .await
}
#[tauri::command]
pub async fn delete_class(state: State<'_, AppState>, id: String) -> Result<Profile> {
    blocking(&state, move |session| {
        profile::delete_class(require_project(session)?, &id)
    })
    .await
}
#[tauri::command]
pub async fn project_health(state: State<'_, AppState>) -> Result<Vec<String>> {
    blocking(&state, |session| {
        crate::services::recovery::inspect(require_project(session)?)
    })
    .await
}
#[tauri::command]
pub async fn list_media(
    state: State<'_, AppState>,
    offset: u32,
    limit: u32,
    query: String,
    status: String,
    filter: Option<library::LibraryFilter>,
) -> Result<MediaPage> {
    blocking(&state, move |session| {
        library::filtered_page(
            require_project(session)?,
            offset,
            limit,
            &query,
            &status,
            &filter.unwrap_or_default(),
        )
    })
    .await
}
#[tauri::command]
pub async fn select_media(state: State<'_, AppState>, id: String) -> Result<ImagePreview> {
    let resources = state.resources.clone();
    blocking(&state, move |session| {
        let s = require_project(session)?;
        let (info, path) = crate::services::preview::cached(s, &id)?;
        s.connection.execute(
            "UPDATE workspace SET active_media_id=?1 WHERE singleton=1",
            [&id],
        )?;
        Ok(ImagePreview {
            info,
            data_url: resources.register(path, "image/png"),
        })
    })
    .await
}
#[tauri::command]
pub async fn media_thumbnail(state: State<'_, AppState>, id: String) -> Result<String> {
    blocking(&state, move |session| {
        library::thumbnail(require_project(session)?, &id)
    })
    .await
}
#[tauri::command]
pub async fn load_document(state: State<'_, AppState>, media_id: String) -> Result<Document> {
    blocking(&state, move |session| {
        annotation::load(require_project(session)?, &media_id)
    })
    .await
}
#[tauri::command]
pub async fn save_document(
    state: State<'_, AppState>,
    document: Document,
    operation_id: String,
) -> Result<u32> {
    blocking(&state, move |session| {
        annotation::save(require_project(session)?, document, &operation_id)
    })
    .await
}
#[tauri::command]
pub async fn cancel_job(state: State<'_, AppState>) -> Result<()> {
    state.ai.cancel();
    state.cancel.store(true, Ordering::Relaxed);
    if let Some(cancel) = state
        .preview_cancel
        .lock()
        .map_err(|_| annotation::invalid("Preview state unavailable."))?
        .as_ref()
    {
        cancel.store(true, Ordering::Relaxed);
    }
    Ok(())
}
#[tauri::command]
pub async fn import_media(
    state: State<'_, AppState>,
    app: AppHandle,
    folder: bool,
    copied: bool,
    paths: Option<Vec<String>>,
) -> Result<Option<ImportReport>> {
    if state.job_active.swap(true, Ordering::SeqCst) {
        return Err(annotation::invalid("Another background job is active."));
    }
    state.cancel.store(false, Ordering::Relaxed);
    let cancel = Arc::clone(&state.cancel);
    let approved = if let Some(paths) = paths {
        let mut allowed = state
            .dropped
            .lock()
            .map_err(|_| annotation::invalid("Drop state unavailable."))?;
        let mut approved = Vec::new();
        for path in paths {
            let path = PathBuf::from(path);
            if !allowed.remove(&path) {
                state.job_active.store(false, Ordering::SeqCst);
                return Err(annotation::invalid(
                    "Dropped paths were not authorized by the native window event.",
                ));
            }
            approved.push(path);
        }
        Some(approved)
    } else {
        None
    };
    let result = blocking(&state, move |session| {
        let current = require_project(session)?;
        let roots = if let Some(paths) = approved {
            paths
        } else if folder {
            match rfd::FileDialog::new()
                .set_title("Import image folder (sources remain unchanged)")
                .pick_folder()
            {
                Some(path) => vec![path],
                None => return Ok(None),
            }
        } else {
            match rfd::FileDialog::new()
                .add_filter("Images", &["jpg", "jpeg", "png", "webp"])
                .pick_files()
            {
                Some(paths) => paths,
                None => return Ok(None),
            }
        };
        library::import_paths(current, &roots, copied, &cancel, |p| {
            let _ = app.emit("job-progress", p);
        })
        .map(Some)
    })
    .await;
    state.job_active.store(false, Ordering::SeqCst);
    result
}
#[tauri::command]
pub async fn export_dataset(
    state: State<'_, AppState>,
    app: AppHandle,
    options: ExportOptions,
) -> Result<Option<ExportReport>> {
    if state.job_active.swap(true, Ordering::SeqCst) {
        return Err(annotation::invalid("Another job is active."));
    }
    state.cancel.store(false, Ordering::Relaxed);
    let cancel = Arc::clone(&state.cancel);
    let result = blocking(&state, move |session| {
        let current = require_project(session)?;
        let Some(parent) = rfd::FileDialog::new()
            .set_title("Choose export parent folder; a new directory will be created")
            .pick_folder()
        else {
            return Ok(None);
        };
        interchange::export(current, &parent, &options, &cancel, |p| {
            let _ = app.emit("job-progress", p);
        })
        .map(Some)
    })
    .await;
    state.job_active.store(false, Ordering::SeqCst);
    result
}
#[tauri::command]
pub async fn inspect_yolo(
    state: State<'_, AppState>,
) -> Result<Option<interchange::YoloInspection>> {
    blocking(&state, |session| {
        let current = require_project(session)?;
        let Some(folder) = rfd::FileDialog::new()
            .set_title("Choose YOLO detection dataset or labels folder")
            .pick_folder()
        else {
            return Ok(None);
        };
        let prepared = interchange::inspect_yolo(current, &folder)?;
        let result = interchange::YoloInspection {
            token: prepared.token.clone(),
            classes: prepared.classes.clone(),
            files: prepared.rows.len(),
            objects: prepared.rows.iter().map(|(_, r)| r.len()).sum(),
        };
        current.pending_yolo = Some(prepared);
        Ok(Some(result))
    })
    .await
}
#[tauri::command]
pub async fn import_yolo(
    state: State<'_, AppState>,
    token: String,
    mapping: Vec<ClassMapping>,
) -> Result<ImportReport> {
    blocking(&state, move |session| {
        let current = require_project(session)?;
        let prepared = current
            .pending_yolo
            .take()
            .ok_or_else(|| annotation::invalid("Inspect labels before importing."))?;
        if prepared.token != token {
            return Err(annotation::invalid("Stale import preview."));
        }
        interchange::import_yolo(current, prepared, &mapping)
    })
    .await
}
#[tauri::command]
pub async fn inspect_mask(
    state: State<'_, AppState>,
    media_id: String,
    layer_id: String,
) -> Result<Option<interchange::MaskInspection>> {
    blocking(&state, move |session| {
        let current = require_project(session)?;
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Class-ID mask", &["png"])
            .pick_file()
        else {
            return Ok(None);
        };
        let prepared = interchange::inspect_mask(current, &path, &media_id, &layer_id)?;
        let mut values = prepared
            .pixels
            .iter()
            .copied()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        values.sort();
        let result = interchange::MaskInspection {
            token: prepared.token.clone(),
            values,
            width: prepared.width,
            height: prepared.height,
        };
        current.pending_mask = Some(prepared);
        Ok(Some(result))
    })
    .await
}
#[tauri::command]
pub async fn apply_mask(
    state: State<'_, AppState>,
    token: String,
    mapping: Vec<PixelMapping>,
) -> Result<Document> {
    blocking(&state, move |session| {
        let current = require_project(session)?;
        let prepared = current
            .pending_mask
            .take()
            .ok_or_else(|| annotation::invalid("Inspect mask before importing."))?;
        if prepared.token != token {
            return Err(annotation::invalid("Stale mask preview."));
        }
        interchange::apply_mask(current, prepared, &mapping)
    })
    .await
}
#[tauri::command]
pub async fn backup_project(state: State<'_, AppState>) -> Result<Option<String>> {
    blocking(&state, |session| {
        let current = require_project(session)?;
        let Some(parent) = rfd::FileDialog::new()
            .set_title("Create a complete database and masks backup in a new folder")
            .pick_folder()
        else {
            return Ok(None);
        };
        Ok(Some(
            interchange::backup(current, &parent)?
                .to_string_lossy()
                .into_owned(),
        ))
    })
    .await
}
#[tauri::command]
pub async fn restore_backup(state: State<'_, AppState>, app: AppHandle) -> Result<Option<Project>> {
    blocking(&state, move |session| {
        require_closed(session)?;
        let Some(source) = rfd::FileDialog::new()
            .set_title("Choose LabelFlow backup folder")
            .pick_folder()
        else {
            return Ok(None);
        };
        let Some(destination) = rfd::FileDialog::new()
            .set_title("Restore into a folder without .labelflow metadata")
            .pick_folder()
        else {
            return Ok(None);
        };
        interchange::restore_backup(&source, &destination)?;
        let opened = project::open(&destination)?;
        let result = opened.project.clone();
        crate::services::session_settings::remember(&app, Some(&opened.root));
        *session = Some(opened);
        Ok(Some(result))
    })
    .await
}
#[tauri::command]
pub async fn open_project(state: State<'_, AppState>, app: AppHandle) -> Result<Option<Project>> {
    blocking(&state, move |session| {
        require_closed(session)?;
        let Some(root) = rfd::FileDialog::new()
            .set_title("Open LabelFlow project folder")
            .pick_folder()
        else {
            return Ok(None);
        };
        let opened = project::open(&root)?;
        let result = opened.project.clone();
        crate::services::session_settings::remember(&app, Some(&opened.root));
        *session = Some(opened);
        Ok(Some(result))
    })
    .await
}
#[tauri::command]
pub async fn close_project(state: State<'_, AppState>, app: AppHandle) -> Result<()> {
    state.ai.stop()?;
    let resources = state.resources.clone();
    blocking(&state, move |session| {
        *session = None;
        resources.clear();
        crate::services::session_settings::remember(&app, None);
        Ok(())
    })
    .await
}
#[tauri::command]
pub async fn open_image(state: State<'_, AppState>) -> Result<Option<ImagePreview>> {
    blocking(&state, |session| {
        let current = require_project(session)?;
        let Some(path) = rfd::FileDialog::new()
            .set_title("Open linked image")
            .add_filter("Images", &["jpg", "jpeg", "png", "webp"])
            .pick_file()
        else {
            return Ok(None);
        };
        media::open(current, &path).map(Some)
    })
    .await
}
#[tauri::command]
pub async fn restore_image(state: State<'_, AppState>) -> Result<Option<ImagePreview>> {
    let resources = state.resources.clone();
    blocking(&state, move |session| {
        let s = require_project(session)?;
        let Some(info) = crate::repositories::active_image(&s.connection)? else {
            return Ok(None);
        };
        let (info, path) = crate::services::preview::cached(s, &info.id)?;
        Ok(Some(ImagePreview {
            info,
            data_url: resources.register(path, "image/png"),
        }))
    })
    .await
}
