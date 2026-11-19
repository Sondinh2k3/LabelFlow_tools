use super::{blocking, require_project, AppState};
use crate::{
    domain::{advanced::*, annotation::Progress, Result},
    services::{
        advanced::{fingerprints, propagation, snapshots},
        review::queue,
    },
};
use tauri::{AppHandle, Emitter, State};
fn notify(app: &AppHandle, p: Progress) {
    let _ = app.emit("job-progress", p);
}
#[tauri::command]
pub async fn quality_dashboard(state: State<'_, AppState>) -> Result<Dashboard> {
    blocking(&state, |s| fingerprints::dashboard(require_project(s)?)).await
}
#[tauri::command]
pub async fn index_fingerprints(
    state: State<'_, AppState>,
    app: AppHandle,
    force: bool,
) -> Result<IndexReport> {
    super::m2::job(&state, move |s, c| {
        fingerprints::index(s, force, c, |p| notify(&app, p))
    })
    .await
}
#[tauri::command]
pub async fn rank_similar_media(
    state: State<'_, AppState>,
    media_id: String,
    scope: String,
    max_distance: u32,
) -> Result<Vec<DuplicateCandidate>> {
    blocking(&state, move |s| {
        fingerprints::rank(require_project(s)?, &media_id, &scope, max_distance)
    })
    .await
}
#[tauri::command]
pub async fn propagation_targets(
    state: State<'_, AppState>,
    anchor_id: String,
) -> Result<Vec<FrameCandidate>> {
    blocking(&state, move |s| {
        propagation::targets(require_project(s)?, &anchor_id)
    })
    .await
}
#[tauri::command]
pub async fn propagate_frames(
    state: State<'_, AppState>,
    app: AppHandle,
    anchor_id: String,
    target_ids: Vec<String>,
) -> Result<PropagationReport> {
    super::m2::job(&state, move |s, c| {
        propagation::generate(s, &anchor_id, &target_ids, c, |p| notify(&app, p))
    })
    .await
}
#[tauri::command]
pub async fn propagation_info(
    state: State<'_, AppState>,
    key: String,
) -> Result<Option<PropagationInfo>> {
    blocking(&state, move |s| {
        propagation::info(require_project(s)?, &key)
    })
    .await
}
#[tauri::command]
pub async fn verify_propagation(
    state: State<'_, AppState>,
    key: String,
    expected_revision: u32,
    confirmed: bool,
) -> Result<PropagationInfo> {
    blocking(&state, move |s| {
        propagation::verify(require_project(s)?, &key, expected_revision, confirmed)
    })
    .await
}
#[tauri::command]
pub async fn list_snapshots(state: State<'_, AppState>) -> Result<Vec<SnapshotSummary>> {
    blocking(&state, |s| snapshots::list(require_project(s)?)).await
}
#[tauri::command]
pub async fn create_snapshot(
    state: State<'_, AppState>,
    app: AppHandle,
    options: SnapshotOptions,
) -> Result<SnapshotSummary> {
    super::m2::job(&state, move |s, c| {
        snapshots::create(s, &options, c, |p| notify(&app, p))
    })
    .await
}
#[tauri::command]
pub async fn verify_snapshot(
    state: State<'_, AppState>,
    app: AppHandle,
    id: String,
    check_sources: bool,
) -> Result<SnapshotCheck> {
    super::m2::job(&state, move |s, c| {
        let job = crate::jobs::start(
            &s.connection,
            "verify_snapshot",
            &crate::services::annotation::json(&id)?,
        )?;
        let result = snapshots::verify(s, &id, check_sources, c, |p| notify(&app, p));
        queue::finish(s, &job, &result);
        result
    })
    .await
}
#[tauri::command]
pub async fn export_snapshot(
    state: State<'_, AppState>,
    app: AppHandle,
    id: String,
) -> Result<Option<String>> {
    super::m2::job(&state, move |s, c| {
        let Some(parent) = rfd::FileDialog::new()
            .set_title("Export frozen snapshot into a new folder")
            .pick_folder()
        else {
            return Ok(None);
        };
        snapshots::export(s, &id, &parent, c, |p| notify(&app, p))
            .map(|p| Some(p.to_string_lossy().into()))
    })
    .await
}
