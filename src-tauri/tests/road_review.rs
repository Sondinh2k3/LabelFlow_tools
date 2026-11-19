use labelflow_lib::{
    domain::{annotation::*, review::*, ErrorCode},
    services::{
        annotation as a, backup, library, project, quality,
        review::{export, metadata, metrics, predictions, queue},
    },
};
use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn setup() -> (tempfile::TempDir, project::ProjectSession, String) {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source.png");
    image::RgbImage::from_pixel(4, 2, image::Rgb([120, 90, 60]))
        .save(&source)
        .unwrap();
    let mut s =
        project::create_with_profile(tmp.path(), "M4 road", "road-encroachment-v1").unwrap();
    library::import_paths(&mut s, &[source], false, &AtomicBool::new(false), |_| {}).unwrap();
    let media = library::all(&s).unwrap()[0].id.clone();
    (tmp, s, media)
}
fn truth(s: &mut project::ProjectSession, media: &str, event: &str, status: &str) -> Document {
    let mut doc = a::load(s, media).unwrap();
    doc.event_label = Some(event.into());
    doc.review_status = status.into();
    doc.masks = vec![
        a::pack(
            "cityscapes_semantic".into(),
            4,
            2,
            &[0, 0, 255, 1, 1, 1, 255, 2],
        ),
        a::pack("road_extent".into(), 4, 2, &[1, 1, 1, 1, 0, 0, 0, 0]),
        a::pack("traffic_exclusion".into(), 4, 2, &[0, 0, 1, 0, 0, 0, 0, 0]),
    ];
    a::save(s, doc, &id()).unwrap();
    a::load(s, media).unwrap()
}
fn prediction(s: &project::ProjectSession, media: &str, event: &str) -> Prediction {
    let mut p: Prediction = serde_json::from_str(include_str!(
        "../../tests/fixtures/review/road-prediction.json"
    ))
    .unwrap();
    p.media_id = media.into();
    p.source_sha256 = library::record(s, media).unwrap().sha256;
    p.event_label = Some(event.into());
    p
}
fn strip_m4(s: &project::ProjectSession) {
    s.connection.execute_batch("DROP TABLE propagation_links; DROP TABLE media_fingerprints; DROP TABLE dataset_snapshots; DROP TABLE snapshot_objects; DROP VIEW review_prediction_index; DROP TABLE review_comparisons; DROP TABLE review_predictions; DROP TABLE media_review_metadata; ALTER TABLE videos DROP COLUMN trip_revision; ALTER TABLE videos DROP COLUMN trip_id; DROP TABLE trips; DROP INDEX media_tags_lookup; DROP INDEX ai_review_listing; ALTER TABLE ai_suggestions DROP COLUMN model_name; ALTER TABLE ai_suggestions DROP COLUMN model_version; ALTER TABLE ai_suggestions DROP COLUMN confidence; UPDATE projects SET schema_version=4; PRAGMA user_version=4;").unwrap();
}

#[test]
fn road_metrics_exclude_ignore_preserve_missing_gt_and_separate_event() {
    let (_tmp, mut s, media) = setup();
    let doc = truth(&mut s, &media, "uncertain", "approved");
    let p = prediction(&s, &media, "positive");
    let report = metrics::compare(&doc, &p, true).unwrap();
    let semantic = report
        .layers
        .iter()
        .find(|l| l.layer_id == "cityscapes_semantic")
        .unwrap();
    assert_eq!(
        (
            semantic.evaluated_pixels,
            semantic.ignored_pixels,
            semantic.disagreement_pixels
        ),
        (6, 2, 2)
    );
    assert!((semantic.mean_iou.unwrap() - 2. / 3.).abs() < 1e-10);
    let road = report
        .layers
        .iter()
        .find(|l| l.layer_id == "road_extent")
        .unwrap();
    assert_eq!(road.mean_iou, Some(0.6));
    assert_eq!(report.missing_gt_layers, ["suspected_region"]);
    assert_eq!(report.event_mismatch, Some(true));
    assert_eq!(
        a::unpack(
            report
                .differences
                .iter()
                .find(|m| m.layer_id == "cityscapes_semantic")
                .unwrap()
        )
        .unwrap(),
        [0, 1, 0, 0, 1, 0, 0, 0]
    );
    assert_eq!(a::load(&s, &media).unwrap(), doc);
}
#[test]
fn prediction_file_import_is_scoped_bounded_and_supports_utf8_bom() {
    let (tmp, s, media) = setup();
    let before = a::load(&s, &media).unwrap();
    let path = tmp.path().join("prediction.json");
    let mut bytes = vec![0xef, 0xbb, 0xbf];
    bytes.extend(
        a::json(&prediction(&s, &media, "uncertain"))
            .unwrap()
            .as_bytes(),
    );
    fs::write(&path, bytes).unwrap();
    let key = predictions::import_file(&s, &path, &media).unwrap();
    assert_eq!(
        predictions::load(&s, &key).unwrap().1.event_label,
        Some("uncertain".into())
    );
    assert!(predictions::import_file(&s, &path, "different-image").is_err());
    fs::File::create(&path)
        .unwrap()
        .set_len(32 * 1024 * 1024 + 1)
        .unwrap();
    assert!(predictions::import_file(&s, &path, &media).is_err());
    assert_eq!(a::load(&s, &media).unwrap(), before);
}
#[test]
fn exported_semantic_ignore_and_road_regions_remain_separate_lossless_pngs() {
    let (tmp, mut s, media) = setup();
    let before = truth(&mut s, &media, "uncertain", "approved");
    let options = ExportOptions {
        format: "semantic".into(),
        approved_only: true,
        seed: "M4".into(),
        train_percent: 100,
        val_percent: 0,
        skip_unsupported: false,
    };
    let report =
        labelflow_lib::export::export(&s, tmp.path(), &options, &AtomicBool::new(false), |_| {})
            .unwrap();
    for mask in &before.masks {
        let file =
            Path::new(&report.path).join(format!("masks/{}/train/{media}.png", mask.layer_id));
        let (w, h, pixels) = a::decode_png(&fs::read(file).unwrap()).unwrap();
        assert_eq!((w, h), (4, 2));
        assert_eq!(pixels, a::unpack(mask).unwrap());
    }
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(Path::new(&report.path).join("manifest.json")).unwrap())
            .unwrap();
    assert!(manifest.to_string().contains("uncertain"));
    assert_eq!(
        manifest["images"][0]["sourceSha256"],
        library::record(&s, &media).unwrap().sha256
    );
    assert_eq!(a::load(&s, &media).unwrap(), before);
}
#[test]
fn predicted_regions_are_independent_and_accept_is_atomic_non_destructive() {
    let (_tmp, mut s, media) = setup();
    let before = truth(&mut s, &media, "negative", "in_progress");
    let key = predictions::import(&s, prediction(&s, &media, "positive")).unwrap();
    assert_eq!(a::load(&s, &media).unwrap(), before);
    let accepted = predictions::approve(&mut s, &key, before.revision).unwrap();
    assert_eq!(accepted.event_label, before.event_label);
    assert_eq!(accepted.review_status, "needs_review");
    assert_eq!(
        a::unpack(
            accepted
                .masks
                .iter()
                .find(|m| m.layer_id == "road_extent")
                .unwrap()
        )
        .unwrap(),
        [1, 1, 1, 1, 1, 0, 0, 0]
    );
    assert_eq!(
        a::unpack(
            accepted
                .masks
                .iter()
                .find(|m| m.layer_id == "cityscapes_semantic")
                .unwrap()
        )
        .unwrap(),
        [0, 0, 2, 1, 1, 1, 0, 2]
    );
    assert_eq!(
        accepted
            .masks
            .iter()
            .find(|m| m.layer_id == "traffic_exclusion"),
        before
            .masks
            .iter()
            .find(|m| m.layer_id == "traffic_exclusion")
    );
    assert_eq!(predictions::load(&s, &key).unwrap().0.status, "accepted");
    assert!(predictions::approve(&mut s, &key, accepted.revision).is_err());
    let key = predictions::import(&s, prediction(&s, &media, "positive")).unwrap();
    s.connection.execute_batch("CREATE TRIGGER abort_review BEFORE UPDATE ON review_predictions BEGIN SELECT RAISE(ABORT,'atomic test');END;").unwrap();
    assert!(predictions::approve(&mut s, &key, accepted.revision).is_err());
    assert_eq!(a::load(&s, &media).unwrap(), accepted);
    assert_eq!(predictions::load(&s, &key).unwrap().0.status, "pending");
}
#[test]
fn protected_stale_invalid_and_rejected_predictions_never_change_ground_truth() {
    let (_tmp, mut s, media) = setup();
    let before = truth(&mut s, &media, "positive", "approved");
    let p = prediction(&s, &media, "negative");
    let key = predictions::import(&s, p.clone()).unwrap();
    assert!(predictions::approve(&mut s, &key, before.revision).is_err());
    predictions::reject(&s, &key).unwrap();
    predictions::reject(&s, &key).unwrap();
    assert!(predictions::approve(&mut s, &key, before.revision).is_err());
    for index in 0..5 {
        let mut bad = p.clone();
        match index {
            0 => bad.width = 5,
            1 => bad.profile_id = "generic-v1".into(),
            2 => bad.source_sha256 = "0".repeat(64),
            3 => bad.masks[1].runs = vec![255, 8],
            _ => bad.masks[0].runs = vec![20, 8],
        };
        assert!(predictions::import(&s, bad).is_err());
    }
    assert_eq!(a::load(&s, &media).unwrap(), before);
    let key = predictions::import(&s, p).unwrap();
    let mut doc = before.clone();
    doc.review_status = "in_progress".into();
    a::save(&mut s, doc, &id()).unwrap();
    assert_eq!(
        predictions::approve(&mut s, &key, before.revision + 1)
            .unwrap_err()
            .code,
        ErrorCode::Conflict
    );
}
#[test]
fn hard_negatives_trips_gps_and_filters_survive_backup_and_do_not_infer_events() {
    let (tmp, mut s, media) = setup();
    let before = truth(&mut s, &media, "uncertain", "approved");
    let old_group = before.split_group.clone();
    let mut meta = metadata::load(&s, &media).unwrap();
    meta.tags = vec!["hard_negative:stopped_vehicle".into(), "light:night".into()];
    meta.latitude = Some(10.8);
    meta.longitude = Some(106.6);
    let saved = metadata::save(&mut s, meta.clone()).unwrap();
    assert_eq!(quality::group(&s, &media).unwrap(), old_group);
    assert!(metadata::save(&mut s, meta).is_err());
    let trip = metadata::save_trip(
        &s,
        Trip {
            id: String::new(),
            name: "Trip A".into(),
            route: "Route 1".into(),
            camera_id: "cam1".into(),
            distance_km: Some(2.),
            notes: String::new(),
            revision: 0,
        },
    )
    .unwrap();
    let mut meta = saved;
    meta.trip_id = Some(trip.id.clone());
    let saved = metadata::save(&mut s, meta).unwrap();
    let mut doc = a::load(&s, &media).unwrap();
    assert_eq!(doc.event_label, Some("uncertain".into()));
    doc.split_group = "attempted-unrelated-group".into();
    a::save(&mut s, doc, &id()).unwrap();
    assert_eq!(
        quality::group(&s, &media).unwrap(),
        format!("trip:{}", trip.id)
    );
    let filter = ReviewFilter {
        tag: "hard_negative:stopped_vehicle".into(),
        trip_id: trip.id.clone(),
        event: "uncertain".into(),
        ..Default::default()
    };
    assert_eq!(queue::page(&s, &filter, 0, 40).unwrap().total, 1);
    let snapshot = a::load(&s, &media).unwrap();
    let key = predictions::import(&s, prediction(&s, &media, "positive")).unwrap();
    let backup = backup::backup(&s, tmp.path()).unwrap();
    let restored = tmp.path().join("restored");
    fs::create_dir(&restored).unwrap();
    backup::restore_backup(&backup, &restored).unwrap();
    let restored = project::open(&restored).unwrap();
    assert_eq!(metadata::load(&restored, &media).unwrap(), saved);
    assert_eq!(metadata::trips(&restored).unwrap()[0], trip);
    assert_eq!(a::load(&restored, &media).unwrap(), snapshot);
    assert_eq!(
        predictions::load(&restored, &key).unwrap().1.event_label,
        Some("positive".into())
    );
}
#[test]
fn video_trip_assignment_covers_every_frame_and_future_frames_cannot_leak() {
    let (_tmp, mut s, media) = setup();
    let video = id();
    s.connection.execute("INSERT INTO videos(id,project_id,path,content_sha256,codec,width,height,rotation,time_base_num,time_base_den,start_pts,duration_ms,average_fps,frame_count,split_group_id) VALUES(?1,?2,'fixture.mp4',?3,'test',4,2,0,1,1000,0,100,10,2,'old-group')",rusqlite::params![video,s.project.id,"0".repeat(64)]).unwrap();
    s.connection.execute("UPDATE media SET source_video_id=?1,frame_index=0,frame_pts=0,frame_time_ms=0 WHERE id=?2",rusqlite::params![video,media]).unwrap();
    let trip = metadata::save_trip(
        &s,
        Trip {
            id: String::new(),
            name: "Continuous clip".into(),
            route: String::new(),
            camera_id: String::new(),
            distance_km: None,
            notes: String::new(),
            revision: 0,
        },
    )
    .unwrap();
    let mut meta = metadata::load(&s, &media).unwrap();
    meta.trip_id = Some(trip.id.clone());
    metadata::save(&mut s, meta).unwrap();
    let frame = id();
    s.connection.execute("INSERT INTO media(id,project_id,type,path,import_mode,content_sha256,width,height,orientation,source_video_id,frame_index,frame_pts,frame_time_ms,split_group_id) VALUES(?1,?2,'image','future.png','linked',?3,4,2,1,?4,1,100,100,'wrong-group')",rusqlite::params![frame,s.project.id,"1".repeat(64),video]).unwrap();
    assert_eq!(
        metadata::load(&s, &frame).unwrap().trip_id,
        Some(trip.id.clone())
    );
    assert_eq!(
        quality::group(&s, &frame).unwrap(),
        format!("trip:{}", trip.id)
    );
    let stale = metadata::load(&s, &frame).unwrap();
    let mut remove = metadata::load(&s, &media).unwrap();
    remove.trip_id = None;
    metadata::save(&mut s, remove).unwrap();
    assert_eq!(
        metadata::save(&mut s, stale).unwrap_err().code,
        ErrorCode::Conflict
    );
}
#[test]
fn comparison_cache_invalidates_after_edits_and_cancel_keeps_manual_state() {
    let (_tmp, mut s, media) = setup();
    let before = truth(&mut s, &media, "negative", "approved");
    predictions::import(&s, prediction(&s, &media, "positive")).unwrap();
    let cancel = AtomicBool::new(false);
    assert_eq!(queue::analyze(&s, &cancel, |_| {}).unwrap(), 1);
    let filter = ReviewFilter {
        disagreements_only: true,
        ..Default::default()
    };
    assert_eq!(queue::page(&s, &filter, 0, 40).unwrap().total, 1);
    let mut next = before;
    next.notes = "human revision".into();
    a::save(&mut s, next, &id()).unwrap();
    assert_eq!(queue::page(&s, &filter, 0, 40).unwrap().total, 0);
    let before = a::load(&s, &media).unwrap();
    cancel.store(true, Ordering::Relaxed);
    assert_eq!(
        queue::analyze(&s, &cancel, |_| {}).unwrap_err().code,
        ErrorCode::Cancelled
    );
    assert_eq!(a::load(&s, &media).unwrap(), before);
    let mut empty = before;
    empty.masks.clear();
    empty.annotations.clear();
    empty.event_label = None;
    a::save(&mut s, empty, &id()).unwrap();
    cancel.store(false, Ordering::Relaxed);
    queue::analyze(&s, &cancel, |_| {}).unwrap();
    assert!(queue::page(&s, &ReviewFilter::default(), 0, 40)
        .unwrap()
        .items[0]
        .disagreement
        .is_none());
}
#[test]
fn metrics_export_is_lossless_reimportable_and_uncertain_events_are_unscored() {
    let (tmp, mut s, media) = setup();
    let before = truth(&mut s, &media, "uncertain", "approved");
    let p = prediction(&s, &media, "positive");
    predictions::import(&s, p.clone()).unwrap();
    let path = export::export(&s, tmp.path(), &AtomicBool::new(false), |_| {}).unwrap();
    let metrics: serde_json::Value =
        serde_json::from_slice(&fs::read(path.join("metrics.json")).unwrap()).unwrap();
    let row = metrics["models"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap();
    assert_eq!(row["excludedEventSamples"], 1);
    assert!(row["eventRecall"].is_null());
    assert!(row["falseTriggerEpisodesPerKm"].is_null());
    let file = fs::read_dir(path.join("predictions"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let imported: Prediction = serde_json::from_slice(&fs::read(file).unwrap()).unwrap();
    assert_eq!(imported.masks, p.masks);
    assert_eq!(imported.event_label, p.event_label);
    predictions::import(&s, imported).unwrap();
    assert_eq!(a::load(&s, &media).unwrap(), before);
    let gt = fs::read_dir(path.join("ground-truth"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let gt: serde_json::Value = serde_json::from_slice(&fs::read(gt).unwrap()).unwrap();
    assert_eq!(
        serde_json::from_value::<Document>(gt["document"].clone()).unwrap(),
        before
    );
    assert_eq!(
        export::export(&s, tmp.path(), &AtomicBool::new(true), |_| {})
            .unwrap_err()
            .code,
        ErrorCode::Cancelled
    );
}
#[test]
fn schema_four_migration_preserves_original_masks_events_and_ai_metadata() {
    let (tmp, mut s, media) = setup();
    let before = truth(&mut s, &media, "positive", "approved");
    strip_m4(&s);
    drop(s);
    let upgraded = project::open(tmp.path()).unwrap();
    assert_eq!(
        upgraded.project.schema_version,
        labelflow_lib::domain::SCHEMA_VERSION
    );
    assert_eq!(a::load(&upgraded, &media).unwrap(), before);
    assert!(metadata::trips(&upgraded).unwrap().is_empty());
    let old = fs::read_dir(tmp.path().join(".labelflow/backups"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let db = rusqlite::Connection::open(old).unwrap();
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        4
    );
}
#[test]
fn event_report_groups_models_deduplicates_images_and_uses_only_binary_approved_labels() {
    let (tmp, mut s, first) = setup();
    let mut ids = vec![first];
    for i in 1..4 {
        let path = tmp.path().join(format!("sample-{i}.png"));
        image::RgbImage::from_pixel(4, 2, image::Rgb([i, 50, 90]))
            .save(&path)
            .unwrap();
        library::import_paths(
            &mut s,
            std::slice::from_ref(&path),
            false,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        ids.push(
            library::all(&s)
                .unwrap()
                .iter()
                .find(|r| Path::new(&r.path).file_name() == path.file_name())
                .unwrap()
                .id
                .clone(),
        );
    }
    let trip = metadata::save_trip(
        &s,
        Trip {
            id: String::new(),
            name: "Measured route".into(),
            route: String::new(),
            camera_id: String::new(),
            distance_km: Some(2.),
            notes: String::new(),
            revision: 0,
        },
    )
    .unwrap();
    for (i, media) in ids.iter().enumerate() {
        truth(
            &mut s,
            media,
            if i == 1 {
                "negative"
            } else if i == 2 {
                "uncertain"
            } else {
                "positive"
            },
            if i == 3 { "needs_review" } else { "approved" },
        );
        if i < 2 {
            let mut meta = metadata::load(&s, media).unwrap();
            meta.trip_id = Some(trip.id.clone());
            metadata::save(&mut s, meta).unwrap();
        }
        predictions::import(
            &s,
            prediction(&s, media, if i == 3 { "negative" } else { "positive" }),
        )
        .unwrap();
    }
    predictions::import(&s, prediction(&s, &ids[0], "positive")).unwrap();
    let mut version2 = prediction(&s, &ids[0], "negative");
    version2.model.version = "2".into();
    predictions::import(&s, version2).unwrap();
    let mut other = prediction(&s, &ids[0], "negative");
    other.model.id = "different-model-same-display-name".into();
    predictions::import(&s, other).unwrap();
    let path = export::export(&s, tmp.path(), &AtomicBool::new(false), |_| {}).unwrap();
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(path.join("metrics.json")).unwrap()).unwrap();
    let models = report["models"].as_object().unwrap();
    assert_eq!(models.len(), 3);
    let first = models
        .values()
        .find(|r| r["model"]["id"] == "road-fixture" && r["model"]["version"] == "1")
        .unwrap();
    assert_eq!(first["samples"], 4);
    assert_eq!(first["approvedSamples"], 3);
    assert_eq!(first["truePositive"], 1);
    assert_eq!(first["falsePositive"], 1);
    assert_eq!(first["excludedEventSamples"], 2);
    assert_eq!(first["eventRecall"], 1.);
    assert_eq!(first["eventPrecision"], 0.5);
    assert_eq!(first["falsePositiveFramesPerKm"], 0.5);
    assert!(first["falseTriggerEpisodesPerKm"].is_null());
}

#[test]
#[ignore = "Measures M3 vs M4 SQLite suggestion-list queries"]
fn benchmark_m4_suggestion_listing() {
    use labelflow_lib::ai::suggestions::{self, Summary};
    use std::time::Instant;
    let (_tmp, s, media) = setup();
    let mut runs = Vec::new();
    for i in 0..40_000 {
        runs.extend([i % 2, 1]);
    }
    runs.extend([0, 2_073_600 - 40_000]);
    let payload=a::json(&serde_json::json!({"model":{"name":"Benchmark model","version":"1"},"confidence":0.8,"mask":runs})).unwrap();
    let tx = s.connection.unchecked_transaction().unwrap();
    for _ in 0..128 {
        tx.execute("INSERT INTO ai_suggestions(id,media_id,base_revision,source_sha256,prediction_json,status,model_name,model_version,confidence) VALUES(?1,?2,0,?3,?4,'rejected','Benchmark model','1',0.8)",rusqlite::params![id(),media,"0".repeat(64),payload]).unwrap();
    }
    tx.commit().unwrap();
    let old = || {
        library::record(&s, &media).unwrap();
        let mut q=s.connection.prepare("SELECT id,media_id,base_revision,status,json_extract(prediction_json,'$.model.name'),json_extract(prediction_json,'$.model.version'),json_extract(prediction_json,'$.confidence'),created_at,accepted_revision FROM ai_suggestions WHERE media_id=?1 ORDER BY (status='pending') DESC,created_at DESC,id DESC LIMIT 100").unwrap();
        q.query_map([&media], |r| {
            Ok(Summary {
                id: r.get(0)?,
                media_id: r.get(1)?,
                base_revision: r.get(2)?,
                status: r.get(3)?,
                model_name: r.get(4)?,
                model_version: r.get(5)?,
                confidence: r.get(6)?,
                created_at: r.get(7)?,
                accepted_revision: r.get(8)?,
            })
        })
        .unwrap()
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap()
    };
    assert_eq!(
        a::json(&old()).unwrap(),
        a::json(&suggestions::list(&s, &media).unwrap()).unwrap()
    );
    let mut baseline = Vec::new();
    let mut optimized = Vec::new();
    for i in 0..55 {
        let at = Instant::now();
        old();
        if i >= 5 {
            baseline.push(at.elapsed().as_secs_f64() * 1000.);
        }
        let at = Instant::now();
        suggestions::list(&s, &media).unwrap();
        if i >= 5 {
            optimized.push(at.elapsed().as_secs_f64() * 1000.);
        }
    }
    let stats = |mut times: Vec<f64>| {
        times.sort_by(f64::total_cmp);
        serde_json::json!({"p50Ms":times[times.len()/2],"p95Ms":times[times.len()*95/100],"samples":times.len()})
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.cache/m4-benchmark");
    fs::create_dir_all(&root).unwrap();
    let tx = s.connection.unchecked_transaction().unwrap();
    for i in 1..10_000 {
        tx.execute("INSERT INTO media(id,project_id,type,path,import_mode,content_sha256,width,height,orientation) VALUES(?1,?2,'image',?3,'linked',?4,4,2,1)",rusqlite::params![id(),s.project.id,format!("record-{i}.png"),format!("{i:064x}")]).unwrap();
    }
    tx.commit().unwrap();
    let mut queue_times = Vec::new();
    for i in 0..25 {
        let at = Instant::now();
        let page = queue::page(&s, &ReviewFilter::default(), 0, 40).unwrap();
        assert_eq!(page.total, 10_000);
        assert_eq!(page.items.len(), 40);
        if i >= 5 {
            queue_times.push(at.elapsed().as_secs_f64() * 1000.);
        }
    }
    fs::write(root.join("sqlite.json"),a::json(&serde_json::json!({"baseline":stats(baseline),"optimized":stats(optimized),"rows":128,"pageSize":100,"payloadBytesPerRow":payload.len(),"resultExact":true,"queue10k":stats(queue_times),"scope":"M3 json_extract query versus M4 normalized summary columns on same database; queue uses 10000 metadata-only records"})).unwrap()).unwrap();
}

#[test]
#[ignore = "Creates isolated M4 native acceptance fixtures"]
fn prepare_m4_desktop_project() {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.cache/m4-acceptance");
    fs::create_dir_all(&parent).unwrap();
    let root = parent.join(id());
    fs::create_dir(&root).unwrap();
    let mut s =
        project::create_with_profile(&root, "M4 road acceptance", "road-encroachment-v1").unwrap();
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/calibration-1920x1080.png");
    library::import_paths(&mut s, &[source], false, &AtomicBool::new(false), |_| {}).unwrap();
    let record = library::all(&s).unwrap()[0].clone();
    let mut doc = a::load(&s, &record.id).unwrap();
    doc.event_label = Some("negative".into());
    doc.review_status = "approved".into();
    doc.notes = "Synthetic plumbing labels; not trained ground truth".into();
    let size = 1920 * 1080;
    let mut semantic = vec![255; size];
    let mut extent = vec![0; size];
    for y in 400..1000 {
        for x in 300..1600 {
            let at = y * 1920 + x;
            semantic[at] = 0;
            extent[at] = 1;
        }
    }
    doc.masks = vec![
        a::pack("cityscapes_semantic".into(), 1920, 1080, &semantic),
        a::pack("road_extent".into(), 1920, 1080, &extent),
        a::pack("traffic_exclusion".into(), 1920, 1080, &vec![0; size]),
        a::pack("suspected_region".into(), 1920, 1080, &vec![0; size]),
    ];
    a::save(&mut s, doc, &id()).unwrap();
    let mut predicted = extent.clone();
    for y in 500..700 {
        for x in 600..900 {
            predicted[y * 1920 + x] = 0;
        }
    }
    let p = Prediction {
        schema_version: 1,
        profile_id: s.project.profile_id.clone(),
        media_id: record.id.clone(),
        source_sha256: record.sha256.clone(),
        width: 1920,
        height: 1080,
        propagation: None,
        model: PredictionModel {
            id: "m4-fixture".into(),
            name: "M4 fixture (not trained)".into(),
            version: "1".into(),
            sha256: None,
            confidence: Some(0.8),
        },
        event_label: Some("positive".into()),
        annotations: vec![],
        masks: vec![
            a::pack("cityscapes_semantic".into(), 1920, 1080, &semantic),
            a::pack("road_extent".into(), 1920, 1080, &predicted),
            a::pack("suspected_region".into(), 1920, 1080, &predicted),
        ],
    };
    let key = predictions::import(&s, p).unwrap();
    fs::write(parent.join("latest.json"),a::json(&serde_json::json!({"project":s.root,"mediaId":record.id,"source":record.path,"sha256":record.sha256,"predictionKey":key})).unwrap()).unwrap();
}
