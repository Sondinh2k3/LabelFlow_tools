use labelflow_lib::{
    ai::{
        protocol::{self, Model, Prediction, Processing},
        suggestions as ai,
        worker::{Config, Runtime, Worker},
    },
    domain::{annotation::*, ErrorCode},
    services::{annotation as a, backup, library, project},
};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../tests/fixtures/ai/{name}"))
}
fn model(name: &str) -> Model {
    let mut value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(fixture(&format!("{name}.json"))).unwrap())
            .unwrap();
    value["task"] = json!(if name == "boxes" { "boxes" } else { "semantic" });
    serde_json::from_value(value).unwrap()
}
fn setup() -> (tempfile::TempDir, project::ProjectSession, String) {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rgb.png");
    image::RgbImage::from_fn(4, 2, |x, _| {
        image::Rgb(if x % 3 == 0 {
            [255, 0, 0]
        } else if x % 3 == 1 {
            [0, 255, 0]
        } else {
            [0, 0, 255]
        })
    })
    .save(&path)
    .unwrap();
    let mut session =
        project::create_with_profile(tmp.path(), "M3 tests", "road-encroachment-v1").unwrap();
    library::import_paths(
        &mut session,
        &[path],
        false,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    let id = library::all(&session).unwrap()[0].id.clone();
    (tmp, session, id)
}
fn prediction(name: &str) -> Prediction {
    let model = model(name);
    Prediction {
        width: 4,
        height: 2,
        orientation: 1,
        coordinate_convention: "canonical-pixel-xyxy".into(),
        class_map: model.class_map.clone(),
        model,
        boxes: if name == "boxes" {
            vec![protocol::PredictedBox {
                class_index: 0,
                confidence: 0.9,
                x: 0.,
                y: 0.,
                width: 2.,
                height: 1.,
            }]
        } else {
            vec![]
        },
        mask: if name == "boxes" {
            None
        } else {
            Some(vec![1, 8])
        },
        confidence: Some(0.9),
        processing: Processing {
            milliseconds: 1.,
            provider: "CPUExecutionProvider".into(),
            device: "cpu".into(),
        },
    }
}
fn store(s: &project::ProjectSession, media: &str, revision: u32, name: &str) -> String {
    ai::store(
        s,
        media,
        revision,
        &library::record(s, media).unwrap().sha256,
        prediction(name),
        &model(name),
    )
    .unwrap()
}
fn manual(s: &mut project::ProjectSession, media: &str, status: &str) -> Document {
    let mut doc = a::load(s, media).unwrap();
    doc.review_status = status.into();
    doc.annotations.push(Annotation {
        id: uuid::Uuid::new_v4().to_string(),
        layer_id: "candidate_objects".into(),
        class_id: Some("candidate-2".into()),
        geometry: Geometry::Bbox {
            x: 2.,
            y: 0.,
            width: 1.,
            height: 1.,
        },
        source: "manual".into(),
    });
    doc.masks.push(a::pack(
        "cityscapes_semantic".into(),
        4,
        2,
        &[13, 255, 255, 255, 255, 255, 255, 255],
    ));
    a::save(s, doc, &uuid::Uuid::new_v4().to_string()).unwrap();
    a::load(s, media).unwrap()
}
#[test]
fn protocol_rejects_wrong_version_id_and_ambiguous_envelopes() {
    assert!(protocol::result(
        json!({"protocol_version":1,"requestId":"a","result":{}}),
        "a"
    )
    .is_ok());
    for value in [
        json!({"protocol_version":2,"requestId":"a","result":{}}),
        json!({"protocol_version":1,"requestId":"b","result":{}}),
        json!({"protocol_version":1,"requestId":"a","result":{},"error":{}}),
        json!({}),
    ] {
        assert!(protocol::result(value, "a").is_err());
    }
}
#[test]
fn app_without_ai_environment_can_edit_save_reopen_and_backup() {
    let (tmp, mut s, media) = setup();
    let doc = manual(&mut s, &media, "approved");
    let runtime = Runtime::default();
    assert!(!runtime.status()["running"].as_bool().unwrap());
    assert!(runtime.model().is_err());
    let config = Config {
        python: tmp.path().join("missing-python.exe"),
        manifest: fixture("semantic.json"),
    };
    assert!(runtime.start(&config, tmp.path(), runtime.token()).is_err());
    assert_eq!(a::load(&s, &media).unwrap(), doc);
    let saved = backup::backup(&s, tmp.path()).unwrap();
    let restored = tmp.path().join("restored");
    fs::create_dir(&restored).unwrap();
    backup::restore_backup(&saved, &restored).unwrap();
    let reopened = project::open(&restored).unwrap();
    assert_eq!(a::load(&reopened, &media).unwrap(), doc);
}
#[test]
fn suggestions_are_separate_reject_is_idempotent_and_no_pending_geometry_leaks() {
    let (_tmp, mut s, media) = setup();
    let before = manual(&mut s, &media, "in_progress");
    let id = store(&s, &media, before.revision, "boxes");
    assert_eq!(a::load(&s, &media).unwrap(), before);
    assert_eq!(
        ai::preview(&s, &id).unwrap().annotations[0].source,
        "model_suggestion"
    );
    let count: i64 = s
        .connection
        .query_row(
            "SELECT count(*) FROM annotations WHERE media_id=?1",
            [&media],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 2); // one manual box and its semantic raster
    ai::reject(&s, &id).unwrap();
    ai::reject(&s, &id).unwrap();
    assert!(ai::approve(&mut s, &id, before.revision).is_err());
    assert_eq!(a::load(&s, &media).unwrap(), before);
}
#[test]
fn approval_preserves_human_pixels_objects_and_atomic_provenance() {
    let (tmp, mut s, media) = setup();
    let before = manual(&mut s, &media, "in_progress");
    let id = store(&s, &media, before.revision, "semantic");
    let accepted = ai::approve(&mut s, &id, before.revision).unwrap();
    assert_eq!(accepted.annotations, before.annotations);
    assert_eq!(
        a::unpack(&accepted.masks[0]).unwrap(),
        [13, 1, 1, 1, 1, 1, 1, 1]
    );
    assert_eq!(accepted.review_status, "needs_review");
    assert_eq!(accepted.revision, before.revision + 1);
    let row = &ai::list(&s, &media).unwrap()[0];
    assert_eq!(row.status, "accepted");
    assert_eq!(row.accepted_revision, Some(accepted.revision));
    assert!(ai::approve(&mut s, &id, accepted.revision).is_err());
    assert!(ai::reject(&s, &id).is_err());
    let boxes = store(&s, &media, accepted.revision, "boxes");
    let appended = ai::approve(&mut s, &boxes, accepted.revision).unwrap();
    assert_eq!(appended.annotations.len(), 2);
    assert_eq!(appended.annotations[0], before.annotations[0]);
    assert_eq!(appended.annotations[1].source, "model_approved");
    let saved = backup::backup(&s, tmp.path()).unwrap();
    let restored = tmp.path().join("restored");
    fs::create_dir(&restored).unwrap();
    backup::restore_backup(&saved, &restored).unwrap();
    let restored = project::open(&restored).unwrap();
    assert_eq!(ai::list(&restored, &media).unwrap().len(), 2);
    assert_eq!(
        ai::preview(&restored, &id).unwrap().prediction.model.sha256,
        model("semantic").sha256
    );
    assert_eq!(a::load(&restored, &media).unwrap(), appended);
}
#[test]
fn stale_and_approved_documents_are_protected() {
    let (_tmp, mut s, media) = setup();
    let id = store(&s, &media, 0, "boxes");
    let before = manual(&mut s, &media, "in_progress");
    assert_eq!(
        ai::approve(&mut s, &id, before.revision).unwrap_err().code,
        ErrorCode::Conflict
    );
    let mut approved = before.clone();
    approved.review_status = "approved".into();
    a::save(&mut s, approved, &uuid::Uuid::new_v4().to_string()).unwrap();
    let before = a::load(&s, &media).unwrap();
    let id = store(&s, &media, before.revision, "boxes");
    assert!(ai::approve(&mut s, &id, before.revision)
        .unwrap_err()
        .message
        .contains("Approved labels"));
    assert_eq!(a::load(&s, &media).unwrap(), before);
}
#[test]
fn invalid_metadata_geometry_mapping_and_dimensions_never_persist() {
    let (_tmp, mut s, media) = setup();
    let sha = library::record(&s, &media).unwrap().sha256;
    for index in 0..8 {
        let mut value = prediction("semantic");
        match index {
            0 => value.width = 9,
            1 => value.class_map.reverse(),
            2 => value.model.version = "different".into(),
            3 => value.mask = Some(vec![90, 8]),
            4 => value.orientation = 6,
            5 => value.processing.provider = "GPU".into(),
            6 => value.confidence = Some(2.),
            _ => value.mask = Some(vec![0, 7]),
        }
        assert!(ai::store(&s, &media, 0, &sha, value, &model("semantic")).is_err());
    }
    let mut value = prediction("boxes");
    value.boxes[0].x = 100.;
    assert!(ai::store(&s, &media, 0, &sha, value, &model("boxes")).is_err());
    let mut bad = model("semantic");
    bad.class_map[0] = "candidate-0".into();
    assert!(ai::validate_model(&s, &bad).is_err());
    assert!(ai::list(&s, &media).unwrap().is_empty());
    let mut doc = a::load(&s, &media).unwrap();
    doc.annotations = ai::preview(&s, &store(&s, &media, 0, "boxes"))
        .unwrap()
        .annotations;
    assert!(a::save(&mut s, doc, &uuid::Uuid::new_v4().to_string()).is_err());
}
#[test]
fn approval_failure_rolls_back_document_operation_and_review_together() {
    let (_tmp, mut s, media) = setup();
    let before = manual(&mut s, &media, "in_progress");
    let id = store(&s, &media, before.revision, "semantic");
    s.connection.execute_batch("CREATE TRIGGER fail_review BEFORE UPDATE ON ai_suggestions BEGIN SELECT RAISE(ABORT,'test atomic failure'); END;").unwrap();
    assert!(ai::approve(&mut s, &id, before.revision).is_err());
    assert_eq!(a::load(&s, &media).unwrap(), before);
    assert_eq!(ai::list(&s, &media).unwrap()[0].status, "pending");
    let n: u32 = s
        .connection
        .query_row("SELECT count(*) FROM annotation_operations", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(n, 1);
}
#[test]
fn pending_and_rejected_predictions_are_excluded_from_coco_export() {
    let (tmp, mut s, media) = setup();
    let before = manual(&mut s, &media, "approved");
    let id = store(&s, &media, before.revision, "boxes");
    let options = ExportOptions {
        format: "coco".into(),
        approved_only: true,
        seed: "m3".into(),
        train_percent: 100,
        val_percent: 0,
        skip_unsupported: false,
    };
    for reject in [false, true] {
        if reject {
            ai::reject(&s, &id).unwrap();
        }
        let report = labelflow_lib::export::export(
            &s,
            tmp.path(),
            &options,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(
            &fs::read(Path::new(&report.path).join("annotations/instances_train.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(value["annotations"].as_array().unwrap().len(), 1);
        assert_eq!(a::load(&s, &media).unwrap(), before);
    }
}
#[test]
fn schema_three_upgrade_backs_up_and_preserves_manual_documents() {
    let (tmp, mut s, media) = setup();
    let before = manual(&mut s, &media, "approved");
    s.connection.execute_batch("DROP TABLE propagation_links; DROP TABLE media_fingerprints; DROP TABLE dataset_snapshots; DROP TABLE snapshot_objects; DROP VIEW review_prediction_index; DROP TABLE review_comparisons; DROP TABLE review_predictions; DROP TABLE media_review_metadata; ALTER TABLE videos DROP COLUMN trip_revision; ALTER TABLE videos DROP COLUMN trip_id; DROP TABLE trips; DROP INDEX media_tags_lookup; DROP TABLE ai_suggestions; UPDATE projects SET schema_version=3; PRAGMA user_version=3;").unwrap();
    drop(s);
    let upgraded = project::open(tmp.path()).unwrap();
    assert_eq!(
        upgraded.project.schema_version,
        labelflow_lib::domain::SCHEMA_VERSION
    );
    assert_eq!(a::load(&upgraded, &media).unwrap(), before);
    assert!(ai::list(&upgraded, &media).unwrap().is_empty());
    let backups = fs::read_dir(tmp.path().join(".labelflow/backups"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    let old = rusqlite::Connection::open(&backups[0]).unwrap();
    assert_eq!(
        old.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        3
    );
}
#[test]
#[ignore = "Creates an isolated native M3 smoke project"]
fn prepare_m3_desktop_project() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.cache/m3-acceptance");
    fs::create_dir_all(&root).unwrap();
    let folder = root.join(uuid::Uuid::new_v4().to_string());
    fs::create_dir(&folder).unwrap();
    let mut s =
        project::create_with_profile(&folder, "M3 AI acceptance", "road-encroachment-v1").unwrap();
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/calibration-1920x1080.png");
    library::import_paths(&mut s, &[source], false, &AtomicBool::new(false), |_| {}).unwrap();
    let record = library::all(&s).unwrap()[0].clone();
    let mut doc = a::load(&s, &record.id).unwrap();
    doc.review_status = "in_progress".into();
    doc.notes = "M3 synthetic model plumbing test; not trained ground truth".into();
    doc.annotations.push(Annotation {
        id: uuid::Uuid::new_v4().to_string(),
        layer_id: "candidate_objects".into(),
        class_id: Some("candidate-2".into()),
        geometry: Geometry::Bbox {
            x: 200.,
            y: 200.,
            width: 300.,
            height: 200.,
        },
        source: "manual".into(),
    });
    let mut pixels = vec![255; record.width as usize * record.height as usize];
    pixels[0] = 13;
    doc.masks.push(a::pack(
        "cityscapes_semantic".into(),
        record.width,
        record.height,
        &pixels,
    ));
    a::save(&mut s, doc, &uuid::Uuid::new_v4().to_string()).unwrap();
    fs::write(
        root.join("latest.json"),
        a::json(&json!({"project":s.root,"mediaId":record.id,"sha256":record.sha256})).unwrap(),
    )
    .unwrap();
}
#[test]
#[ignore = "Requires LABELFLOW_TEST_PYTHON with worker requirements installed"]
fn real_worker_cpu_inference_cancel_crash_limits_and_restart() {
    let python = PathBuf::from(
        std::env::var_os("LABELFLOW_TEST_PYTHON").expect("Set LABELFLOW_TEST_PYTHON"),
    );
    let (tmp, mut s, media) = setup();
    let before = manual(&mut s, &media, "approved");
    let runtime = Arc::new(Runtime::default());
    let config = Config {
        python: python.clone(),
        manifest: fixture("semantic.json"),
    };
    runtime.start(&config, tmp.path(), runtime.token()).unwrap();
    let (info, path) = labelflow_lib::services::preview::cached(&s, &media).unwrap();
    let (model,value)=runtime.predict(json!({"imagePath":path,"width":info.width,"height":info.height,"orientation":1,"modelVersion":"1.0.0","parameters":{}}),runtime.token()).unwrap();
    let prediction: Prediction = serde_json::from_value(value).unwrap();
    ai::validate(&s, &media, &prediction, &model).unwrap();
    assert_eq!(
        prediction.mask.unwrap(),
        vec![0, 1, 1, 1, 2, 1, 0, 2, 1, 1, 2, 1, 0, 1]
    );
    runtime.stop().unwrap();
    for (name, code) in [
        ("crash", "import os; os._exit(3)"),
        ("bad-json", "print('oops',flush=True)"),
        ("oversize", "print('x'*(17*1024*1024),flush=True)"),
        ("hang", "import time; time.sleep(30)"),
        ("timeout", "import time; time.sleep(30)"),
    ] {
        let script = tmp.path().join(format!("{name}.py"));
        fs::write(&script, code).unwrap();
        let mut worker = Worker::spawn(&python, &script).unwrap();
        let token = runtime.token();
        let cancel = if name == "hang" {
            let cloned = runtime.clone();
            Some(std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(100));
                cloned.cancel();
            }))
        } else {
            None
        };
        let started = Instant::now();
        assert!(worker
            .request("health", json!({}), &runtime, token, Duration::from_secs(2))
            .is_err());
        drop(worker);
        if let Some(thread) = cancel {
            thread.join().unwrap();
        }
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(ai::list(&s, &media).unwrap().is_empty());
        assert_eq!(a::load(&s, &media).unwrap(), before);
    }
    runtime.start(&config, tmp.path(), runtime.token()).unwrap();
    runtime.stop().unwrap();
}
