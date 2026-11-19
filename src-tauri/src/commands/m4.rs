use super::{blocking, require_project, AppState};
use crate::{
    domain::{annotation::Document, review::*, Result},
    services::review,
};
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub async fn review_metadata(state: State<'_, AppState>, media_id: String) -> Result<Metadata> {
    blocking(&state, move |session| {
        review::metadata::load(require_project(session)?, &media_id)
    })
    .await
}
#[tauri::command]
pub async fn save_review_metadata(
    state: State<'_, AppState>,
    metadata: Metadata,
) -> Result<Metadata> {
    blocking(&state, move |session| {
        review::metadata::save(require_project(session)?, metadata)
    })
    .await
}
#[tauri::command]
pub async fn list_trips(state: State<'_, AppState>) -> Result<Vec<Trip>> {
    blocking(&state, move |session| {
        review::metadata::trips(require_project(session)?)
    })
    .await
}
#[tauri::command]
pub async fn save_trip(state: State<'_, AppState>, trip: Trip) -> Result<Trip> {
    blocking(&state, move |session| {
        review::metadata::save_trip(require_project(session)?, trip)
    })
    .await
}
#[tauri::command]
pub async fn review_page(
    state: State<'_, AppState>,
    filter: ReviewFilter,
    offset: u32,
    limit: u32,
) -> Result<ReviewPage> {
    blocking(&state, move |session| {
        review::queue::page(require_project(session)?, &filter, offset, limit)
    })
    .await
}
#[tauri::command]
pub async fn review_predictions(
    state: State<'_, AppState>,
    media_id: String,
) -> Result<Vec<PredictionSummary>> {
    blocking(&state, move |session| {
        review::predictions::summaries(require_project(session)?, &media_id)
    })
    .await
}
#[tauri::command]
pub async fn review_snapshot(state: State<'_, AppState>, key: String) -> Result<Snapshot> {
    blocking(&state, move |session| {
        review::predictions::snapshot(require_project(session)?, &key)
    })
    .await
}
#[tauri::command]
pub async fn import_review_prediction(
    state: State<'_, AppState>,
    media_id: String,
) -> Result<Option<String>> {
    blocking(&state, move |session| {
        let s = require_project(session)?;
        let Some(path) = rfd::FileDialog::new()
            .set_title("Import independent model prediction (LabelFlow review JSON v1)")
            .add_filter("Review prediction JSON", &["json"])
            .pick_file()
        else {
            return Ok(None);
        };
        review::predictions::import_file(s, &path, &media_id).map(Some)
    })
    .await
}
#[tauri::command]
pub async fn approve_review_prediction(
    state: State<'_, AppState>,
    key: String,
    expected_revision: u32,
) -> Result<Document> {
    blocking(&state, move |session| {
        review::predictions::approve(require_project(session)?, &key, expected_revision)
    })
    .await
}
#[tauri::command]
pub async fn reject_review_prediction(state: State<'_, AppState>, key: String) -> Result<()> {
    blocking(&state, move |session| {
        review::predictions::reject(require_project(session)?, &key)
    })
    .await
}
#[tauri::command]
pub async fn analyze_review(state: State<'_, AppState>, app: AppHandle) -> Result<u32> {
    super::m2::job(&state, move |s, cancel| {
        review::queue::analyze(s, cancel, |p| {
            let _ = app.emit("job-progress", p);
        })
    })
    .await
}
#[tauri::command]
pub async fn export_review_metrics(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Option<String>> {
    super::m2::job(&state, move |s, cancel| {
        let Some(parent) = rfd::FileDialog::new()
            .set_title("Create a new review metrics export folder")
            .pick_folder()
        else {
            return Ok(None);
        };
        review::export::export(s, &parent, cancel, |p| {
            let _ = app.emit("job-progress", p);
        })
        .map(|p| Some(p.to_string_lossy().into_owned()))
    })
    .await
}
