//! IPC for team discussions. Shared-folder IO runs outside the project mutex so a slow network
//! drive never blocks autosave; only short SQLite lookups take the project lock.
use super::{blocking, require_project, AppState};
use crate::{
    domain::{annotation::Point, AppError, ErrorCode, Result},
    services::{
        annotation, library, profile,
        session_settings::config_dir,
        team::{self, MediaRef, NewComment, TeamConfig},
    },
};
use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, State};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamStatus {
    pub user_id: String,
    pub user_name: String,
    pub folder: Option<String>,
}

fn settings_dir(app: &AppHandle) -> Result<PathBuf> {
    config_dir(app).ok_or_else(|| {
        AppError::new(
            ErrorCode::Io,
            "The user settings folder is unavailable on this computer.",
        )
    })
}
async fn project_id(state: &AppState) -> Result<String> {
    blocking(state, |session| {
        Ok(require_project(session)?.project.id.clone())
    })
    .await
}
async fn off_thread<T: Send + 'static>(
    action: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tauri::async_runtime::spawn_blocking(action)
        .await
        .map_err(|e| AppError::new(ErrorCode::Internal, format!("Background task failed: {e}")))?
}
/// Current config plus the team folder linked to the open project.
async fn context(state: &AppState, app: &AppHandle) -> Result<(PathBuf, TeamConfig, String)> {
    let id = project_id(state).await?;
    let dir = settings_dir(app)?;
    let config = off_thread({
        let dir = dir.clone();
        move || Ok(team::load_config(&dir))
    })
    .await?;
    Ok((dir, config, id))
}
fn folder_of(config: &TeamConfig, project: &str) -> Result<PathBuf> {
    config
        .folders
        .get(project)
        .map(PathBuf::from)
        .ok_or_else(|| annotation::invalid("Choose a team folder first."))
}
fn status(config: &TeamConfig, project: &str) -> TeamStatus {
    TeamStatus {
        user_id: config.user_id.clone(),
        user_name: config.user_name.clone(),
        folder: config.folders.get(project).cloned(),
    }
}

#[tauri::command]
pub async fn team_status(state: State<'_, AppState>, app: AppHandle) -> Result<TeamStatus> {
    let (_, config, project) = context(&state, &app).await?;
    Ok(status(&config, &project))
}

/// `folder`: "keep", "pick" (native dialog) or "clear".
#[tauri::command]
pub async fn team_configure(
    state: State<'_, AppState>,
    app: AppHandle,
    user_name: String,
    folder: String,
) -> Result<Option<TeamStatus>> {
    let (dir, mut config, project) = context(&state, &app).await?;
    team::set_user_name(&mut config, &user_name)?;
    match folder.as_str() {
        "keep" => {}
        "clear" => {
            config.folders.remove(&project);
        }
        "pick" => {
            let chosen = off_thread(|| {
                Ok(rfd::FileDialog::new()
                    .set_title("Choose the shared team folder (LAN share, OneDrive, Google Drive, Syncthing…)")
                    .pick_folder())
            })
            .await?;
            let Some(chosen) = chosen else {
                return Ok(None);
            };
            let prepared = off_thread(move || team::prepare_folder(&chosen)).await?;
            let text = prepared
                .to_str()
                .ok_or_else(|| annotation::invalid("Team folder path must be valid UTF-8."))?
                .to_owned();
            config.folders.insert(project.clone(), text);
        }
        _ => return Err(annotation::invalid("Unknown team folder action.")),
    }
    let saved = config.clone();
    off_thread(move || team::save_config(&dir, &saved)).await?;
    Ok(Some(status(&config, &project)))
}

#[tauri::command]
pub async fn team_threads(state: State<'_, AppState>, app: AppHandle) -> Result<team::Feed> {
    let (_, config, project) = context(&state, &app).await?;
    let Some(folder) = config.folders.get(&project).map(PathBuf::from) else {
        return Ok(team::Feed::default());
    };
    let cache = state.team.clone();
    let (entries, warnings) = off_thread(move || cache.scan(&folder)).await?;
    let mut threads = team::threads(entries);
    threads.truncate(5000);
    let shas = threads
        .iter()
        .map(|t| t.media.sha256.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let local = blocking(&state, move |session| {
        let s = require_project(session)?;
        let mut statement = s.connection.prepare_cached(
            "SELECT id FROM media WHERE project_id=?1 AND lower(content_sha256)=?2 ORDER BY created_at,id LIMIT 1",
        )?;
        let mut found = Vec::with_capacity(shas.len());
        for sha in shas {
            use rusqlite::OptionalExtension;
            found.push(
                statement
                    .query_row([&s.project.id, &sha], |r| r.get::<_, String>(0))
                    .optional()?,
            );
        }
        Ok(found)
    })
    .await?;
    for (thread, id) in threads.iter_mut().zip(local) {
        thread.local_media_id = id;
    }
    Ok(team::Feed { threads, warnings })
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PostInput {
    pub thread_id: Option<String>,
    pub media_id: Option<String>,
    pub body: String,
    pub pin: Option<Point>,
    pub include_labels: bool,
}

#[tauri::command]
pub async fn team_post(
    state: State<'_, AppState>,
    app: AppHandle,
    input: PostInput,
) -> Result<team::Entry> {
    let (_, config, project) = context(&state, &app).await?;
    let folder = folder_of(&config, &project)?;
    let new_thread = input.thread_id.is_none();
    let media_id = input.media_id.clone();
    let include_labels = input.include_labels;
    // Short project lock: image identity and vector labels.
    let (media, labels, project_name) = blocking(&state, move |session| {
        let s = require_project(session)?;
        let Some(id) = media_id else {
            return Ok((None, Vec::new(), s.project.name.clone()));
        };
        let record = library::record(s, &id)?;
        let labels = if include_labels {
            team::label_shapes(&annotation::load(s, &id)?, &profile::current(s)?)
        } else {
            Vec::new()
        };
        Ok((
            Some((
                id,
                MediaRef {
                    sha256: record.sha256.to_ascii_lowercase(),
                    name: record.name.clone(),
                    width: record.width,
                    height: record.height,
                },
            )),
            labels,
            s.project.name.clone(),
        ))
    })
    .await?;
    // Decode a preview only once per image, checking the (possibly slow) share without the lock.
    let preview = match (&media, new_thread) {
        (Some((id, m)), true) => {
            let path = folder.join("previews").join(format!("{}.jpg", m.sha256));
            if off_thread(move || Ok(path.is_file())).await? {
                None
            } else {
                let id = id.clone();
                blocking(&state, move |session| {
                    let s = require_project(session)?;
                    let record = library::record(s, &id)?;
                    Ok(library::checked_load(s, &record)
                        .ok()
                        .map(|(_, image)| image))
                })
                .await?
            }
        }
        _ => None,
    };
    let media = media.map(|(_, m)| m);
    off_thread(move || {
        if let (Some(media), Some(image)) = (&media, &preview) {
            // A missing preview only limits what teammates see; the question is still posted.
            if let Err(e) = team::write_preview(&folder, &media.sha256, image) {
                eprintln!("Could not write team preview: {e}");
            }
        }
        team::comment(
            &folder,
            &config,
            NewComment {
                thread_id: input.thread_id,
                media: if new_thread { media } else { None },
                body: input.body,
                pin: if new_thread { input.pin } else { None },
                labels,
                project: project_name,
            },
        )
    })
    .await
}

#[tauri::command]
pub async fn team_set_status(
    state: State<'_, AppState>,
    app: AppHandle,
    thread_id: String,
    resolved: bool,
) -> Result<team::Entry> {
    let (_, config, project) = context(&state, &app).await?;
    let folder = folder_of(&config, &project)?;
    off_thread(move || team::set_status(&folder, &config, &thread_id, resolved)).await
}

#[tauri::command]
pub async fn team_preview(
    state: State<'_, AppState>,
    app: AppHandle,
    sha256: String,
) -> Result<Option<String>> {
    let (_, config, project) = context(&state, &app).await?;
    let folder = folder_of(&config, &project)?;
    off_thread(move || team::read_preview(&folder, &sha256)).await
}
