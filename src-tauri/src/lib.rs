pub mod ai;
pub mod commands;
pub mod database;
pub mod domain;
pub mod export;
pub mod import;
pub mod jobs;
pub mod repositories;
pub mod security;
pub mod services;
pub mod video;

pub fn run() {
    use tauri::{Emitter, Manager};
    tauri::Builder::default()
        .manage(commands::AppState::default())
        .register_asynchronous_uri_scheme_protocol("lfmedia", |context, request, responder| {
            let resources = context
                .app_handle()
                .state::<commands::AppState>()
                .resources
                .clone();
            tauri::async_runtime::spawn_blocking(move || {
                responder.respond(resources.response(request))
            });
        })
        .setup(|app| {
            let args = std::env::args().skip(1).collect::<Vec<_>>();
            let path = if args.first().is_some_and(|s| s == "--home") {
                None
            } else if args.first().is_some_and(|s| s == "--project") {
                args.get(1).map(std::path::PathBuf::from)
            } else {
                services::session_settings::last_project(app.handle())
            };
            app.state::<commands::AppState>().initialize(path);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) = event {
                let state = window.state::<commands::AppState>();
                if let Ok(mut allowed) = state.dropped.lock() {
                    allowed.clear();
                    allowed.extend(paths.iter().take(128).cloned());
                }
                let accepted = paths.iter().take(128).cloned().collect::<Vec<_>>();
                let _ = window.emit("media-drop", accepted);
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::resources::storage_usage,
            commands::resources::clear_derived_cache,
            commands::create_project,
            commands::open_project,
            commands::close_project,
            commands::open_image,
            commands::restore_image,
            commands::project_profile,
            commands::list_media,
            commands::select_media,
            commands::media_thumbnail,
            commands::load_document,
            commands::save_document,
            commands::cancel_job,
            commands::import_media,
            commands::export_dataset,
            commands::inspect_yolo,
            commands::import_yolo,
            commands::inspect_mask,
            commands::apply_mask,
            commands::backup_project,
            commands::restore_backup,
            commands::current_project,
            commands::project_health,
            commands::save_class,
            commands::delete_class,
            commands::team::team_status,
            commands::team::team_configure,
            commands::team::team_threads,
            commands::team::team_post,
            commands::team::team_set_status,
            commands::team::team_preview,
            commands::m2::video_tools,
            commands::m2::configure_video_tools,
            commands::m2::open_video,
            commands::m2::list_videos,
            commands::m2::video_frames,
            commands::m2::seek_video,
            commands::m2::video_stream,
            commands::m2::preview_frame,
            commands::m2::extract_frames,
            commands::m2::set_video_group,
            commands::m2::inspect_instances,
            commands::m2::apply_instances,
            commands::m2::dataset_quality,
            commands::m2::repair_media,
            commands::m2::media_origin,
            commands::m2::repair_video,
            commands::m2::preload_media,
            commands::m3::ai_status,
            commands::m3::ai_configure,
            commands::m3::ai_start,
            commands::m3::ai_stop,
            commands::m3::ai_cancel,
            commands::m3::ai_predict,
            commands::m3::ai_suggestions,
            commands::m3::ai_preview,
            commands::m3::ai_reject,
            commands::m3::ai_approve,
            commands::m4::review_metadata,
            commands::m4::save_review_metadata,
            commands::m4::list_trips,
            commands::m4::save_trip,
            commands::m4::review_page,
            commands::m4::review_predictions,
            commands::m4::review_snapshot,
            commands::m4::import_review_prediction,
            commands::m4::approve_review_prediction,
            commands::m4::reject_review_prediction,
            commands::m4::analyze_review,
            commands::m4::export_review_metrics,
            commands::labeling::inspect_mask_folder,
            commands::labeling::apply_mask_folder,
            commands::labeling::inspect_ignore_template,
            commands::labeling::apply_ignore_template,
            commands::labeling::sam_status,
            commands::labeling::sam_configure,
            commands::labeling::sam_start,
            commands::labeling::sam_stop,
            commands::labeling::sam_segment,
            commands::m5::quality_dashboard,
            commands::m5::index_fingerprints,
            commands::m5::rank_similar_media,
            commands::m5::propagation_targets,
            commands::m5::propagate_frames,
            commands::m5::propagation_info,
            commands::m5::verify_propagation,
            commands::m5::list_snapshots,
            commands::m5::create_snapshot,
            commands::m5::verify_snapshot,
            commands::m5::export_snapshot
        ])
        .run(tauri::generate_context!())
        .expect("LabelFlow could not initialize the desktop runtime");
}
