use super::{blocking, require_project, AppState};
use crate::{
    domain::Result,
    services::storage::{self, Usage},
};
use tauri::State;

#[tauri::command]
pub async fn storage_usage(state: State<'_, AppState>) -> Result<Usage> {
    blocking(&state, |s| storage::usage(require_project(s)?)).await
}
#[tauri::command]
pub async fn clear_derived_cache(state: State<'_, AppState>, confirmed: bool) -> Result<Usage> {
    blocking(&state, move |s| {
        storage::clear(require_project(s)?, confirmed)
    })
    .await
}
