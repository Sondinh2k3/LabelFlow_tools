use image::{DynamicImage, GrayImage};
use labelflow_lib::{
    domain::{advanced::*, annotation::*, ErrorCode, SCHEMA_VERSION},
    services::{
        advanced::{fingerprints, propagation, snapshots},
        annotation as a, backup, library, project,
        review::{metadata, predictions},
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
const W: u32 = 128;
const H: u32 = 96;
fn texture(dx: i32, dy: i32) -> GrayImage {
    GrayImage::from_fn(W, H, |x, y| {
        let (u, v) = (x as i32 - dx, y as i32 - dy);
        let k = if u >= 0 && v >= 0 && u < W as i32 && v < H as i32 {
            let n = (u as u32).wrapping_mul(73856093) ^ (v as u32).wrapping_mul(19349663);
            (n ^ (n >> 8) ^ (n >> 16)) as u8
        } else {
            0
        };
        image::Luma([k])
    })
}
fn setup() -> (tempfile::TempDir, project::ProjectSession, Vec<String>) {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("project");
    fs::create_dir(&project_dir).unwrap();
    let mut s = project::create_with_profile(&project_dir, "M5 acceptance", "road-encroachment-v1")
        .unwrap();
    let mut paths = vec![];
    for (name, im) in [
        ("anchor", texture(0, 0)),
        ("target", texture(3, 2)),
        ("next", texture(-2, 1)),
        ("flat", GrayImage::from_pixel(W, H, image::Luma([100]))),
    ] {
        let p = tmp.path().join(format!("{name}.png"));
        im.save(&p).unwrap();
        paths.push(p);
    }
    library::import_paths(&mut s, &paths, false, &AtomicBool::new(false), |_| {}).unwrap();
    let all = library::all(&s).unwrap();
    let ids = paths
        .iter()
        .map(|p| {
            all.iter()
                .find(|r| Some(std::ffi::OsStr::new(&r.name)) == p.file_name())
                .unwrap()
                .id
                .clone()
        })
        .collect::<Vec<_>>();
    let video = id();
    s.connection.execute("INSERT INTO videos(id,project_id,path,content_sha256,codec,width,height,rotation,time_base_num,time_base_den,start_pts,duration_ms,average_fps,frame_count,split_group_id) VALUES(?1,?2,'fixture.mp4',?3,'test',128,96,0,1,1000,0,3000,10,4,'m5-video')",rusqlite::params![video,s.project.id,"0".repeat(64)]).unwrap();
    for (i, media) in ids.iter().enumerate() {
        s.connection.execute("UPDATE media SET source_video_id=?1,frame_index=?2,frame_pts=?3,frame_time_ms=?3,split_group_id='m5-video' WHERE id=?4",rusqlite::params![video,i,i*100,media]).unwrap();
    }
    let mut doc = a::load(&s, &ids[0]).unwrap();
    doc.review_status = "approved".into();
    doc.event_label = Some("positive".into());
    let mut semantic = vec![255; (W * H) as usize];
    let mut extent = vec![0; semantic.len()];
    for y in 20..50 {
        for x in 20..70 {
            semantic[(y * W + x) as usize] = 0;
            extent[(y * W + x) as usize] = 1;
        }
    }
    doc.masks = vec![
        a::pack("cityscapes_semantic".into(), W, H, &semantic),
        a::pack("road_extent".into(), W, H, &extent),
        a::pack("traffic_exclusion".into(), W, H, &vec![0; (W * H) as usize]),
    ];
    doc.annotations = vec![Annotation {
        id: id(),
        layer_id: "candidate_objects".into(),
        class_id: Some("candidate-0".into()),
        geometry: Geometry::Bbox {
            x: 30.,
            y: 25.,
            width: 15.,
            height: 20.,
        },
        source: "manual".into(),
    }];
    a::save(&mut s, doc, &id()).unwrap();
    (tmp, s, ids)
}
fn opts() -> SnapshotOptions {
    SnapshotOptions {
        name: "Version 1".into(),
        seed: "m5-seed".into(),
        train_percent: 80,
        val_percent: 10,
        approved_only: false,
    }
}
#[test]
fn registration_is_exact_and_flat_ambiguous_or_changed_images_fail() {
    let a = DynamicImage::ImageLuma8(texture(0, 0));
    let b = DynamicImage::ImageLuma8(texture(3, 2));
    let (dx, dy, error, margin) = propagation::register(&a, &b, &AtomicBool::new(false)).unwrap();
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/advanced/translation.json"
    ))
    .unwrap();
    assert_eq!(
        (i64::from(dx), i64::from(dy)),
        (
            expected["dx"].as_i64().unwrap(),
            expected["dy"].as_i64().unwrap()
        )
    );
    assert_eq!(error, 0.);
    assert!(margin > 0.5);
    let flat = DynamicImage::ImageLuma8(GrayImage::from_pixel(W, H, image::Luma([100])));
    assert!(propagation::register(&flat, &flat, &AtomicBool::new(false)).is_err());
    assert!(propagation::register(&a, &flat, &AtomicBool::new(false)).is_err());
    assert_eq!(
        propagation::register(&a, &b, &AtomicBool::new(true))
            .unwrap_err()
            .code,
        ErrorCode::Cancelled
    );
}
#[test]
fn mask_translation_keeps_ignore_holes_and_clips_without_wrapping() {
    let m = a::pack("semantic".into(), 4, 2, &[255, 0, 1, 2, 3, 4, 255, 5]);
    let shifted = propagation::shift_mask(&m, 1, 1, 255).unwrap();
    assert_eq!(
        a::unpack(&shifted).unwrap(),
        [255, 255, 255, 255, 255, 255, 0, 1]
    );
    assert_eq!(
        a::unpack(&propagation::shift_mask(&m, -1, 0, 0).unwrap()).unwrap(),
        [0, 1, 2, 0, 4, 255, 5, 0]
    );
    assert!(
        a::unpack(&propagation::shift_mask(&m, i32::MIN, i32::MAX, 255).unwrap())
            .unwrap()
            .iter()
            .all(|&v| v == 255)
    );
}
#[test]
fn propagation_requires_visual_verification_preserves_gt_and_independent_event() {
    let (_tmp, mut s, ids) = setup();
    let mut target = a::load(&s, &ids[1]).unwrap();
    target.event_label = Some("negative".into());
    let mut manual = vec![255; (W * H) as usize];
    manual[0] = 7;
    target.masks = vec![a::pack("cityscapes_semantic".into(), W, H, &manual)];
    a::save(&mut s, target, &id()).unwrap();
    let before = a::load(&s, &ids[1]).unwrap();
    let anchor = a::load(&s, &ids[0]).unwrap();
    let report =
        propagation::generate(&s, &ids[0], &ids[1..2], &AtomicBool::new(false), |_| {}).unwrap();
    assert!(report.skipped.is_empty());
    let p = &report.created[0];
    assert_eq!((p.dx, p.dy), (3, 2));
    assert_eq!(a::load(&s, &ids[1]).unwrap(), before);
    assert!(p.verified_at.is_none());
    assert!(predictions::approve(&mut s, &p.key, before.revision).is_err());
    assert!(propagation::verify(&s, &p.key, before.revision, false).is_err());
    propagation::verify(&s, &p.key, before.revision, true).unwrap();
    let accepted = predictions::approve(&mut s, &p.key, before.revision).unwrap();
    assert_eq!(accepted.review_status, "needs_review");
    assert_eq!(accepted.event_label, before.event_label);
    let pixels = a::unpack(
        accepted
            .masks
            .iter()
            .find(|m| m.layer_id == "cityscapes_semantic")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(pixels[0], 7);
    assert_eq!(pixels[(22 * W + 23) as usize], 0);
    assert_eq!(pixels[1], 255);
    assert_eq!(a::load(&s, &ids[0]).unwrap(), anchor);
    assert_eq!(accepted.annotations.len(), 1);
    assert_eq!(accepted.annotations[0].source, "model_approved");
    assert!(predictions::approve(&mut s, &p.key, accepted.revision).is_err());
}
#[test]
fn source_target_revisions_changed_files_and_approved_targets_block_acceptance() {
    let (_tmp, mut s, ids) = setup();
    let key = propagation::generate(&s, &ids[0], &ids[1..2], &AtomicBool::new(false), |_| {})
        .unwrap()
        .created[0]
        .key
        .clone();
    propagation::verify(&s, &key, 0, true).unwrap();
    let mut anchor = a::load(&s, &ids[0]).unwrap();
    anchor.notes = "Changed anchor".into();
    a::save(&mut s, anchor, &id()).unwrap();
    assert!(predictions::approve(&mut s, &key, 0).is_err());
    let key = propagation::generate(&s, &ids[0], &ids[1..2], &AtomicBool::new(false), |_| {})
        .unwrap()
        .created[0]
        .key
        .clone();
    propagation::verify(&s, &key, 0, true).unwrap();
    let mut target = a::load(&s, &ids[1]).unwrap();
    target.review_status = "approved".into();
    a::save(&mut s, target, &id()).unwrap();
    assert!(predictions::approve(&mut s, &key, 1).is_err());
    let report =
        propagation::generate(&s, &ids[0], &ids[1..2], &AtomicBool::new(false), |_| {}).unwrap();
    assert!(report.created.is_empty());
    assert_eq!(report.skipped.len(), 1);
    let key = propagation::generate(&s, &ids[0], &ids[2..3], &AtomicBool::new(false), |_| {})
        .unwrap()
        .created[0]
        .key
        .clone();
    let path = library::resolve(&s, &library::record(&s, &ids[2]).unwrap().path).unwrap();
    fs::write(path, b"changed").unwrap();
    assert!(propagation::verify(&s, &key, 0, true).is_err());
}
#[test]
fn propagation_cancellation_and_bad_targets_never_change_documents() {
    let (_tmp, s, ids) = setup();
    let before = ids
        .iter()
        .map(|id| a::load(&s, id).unwrap())
        .collect::<Vec<_>>();
    let cancel = AtomicBool::new(false);
    let e = propagation::generate(&s, &ids[0], &ids[1..3], &cancel, |_| {
        cancel.store(true, Ordering::Relaxed)
    })
    .unwrap_err();
    assert_eq!(e.code, ErrorCode::Cancelled);
    assert_eq!(
        s.connection
            .query_row("SELECT count(*) FROM propagation_links", [], |r| r
                .get::<_, u32>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        before,
        ids.iter()
            .map(|id| a::load(&s, id).unwrap())
            .collect::<Vec<_>>()
    );
    assert!(propagation::generate(
        &s,
        &ids[0],
        &[ids[1].clone(), ids[1].clone()],
        &AtomicBool::new(false),
        |_| {}
    )
    .is_err());
    s.connection
        .execute("UPDATE media SET frame_time_ms=3000 WHERE id=?1", [&ids[2]])
        .unwrap();
    assert!(
        propagation::generate(&s, &ids[0], &ids[2..3], &AtomicBool::new(false), |_| {}).is_err()
    );
    let report =
        propagation::generate(&s, &ids[0], &ids[3..4], &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(report.skipped.len(), 1);
    assert!(report.created.is_empty());
}
#[test]
fn propagation_import_export_keeps_origin_and_requires_fresh_verification() {
    let (_tmp, s, ids) = setup();
    let key = propagation::generate(&s, &ids[0], &ids[1..2], &AtomicBool::new(false), |_| {})
        .unwrap()
        .created[0]
        .key
        .clone();
    propagation::verify(&s, &key, 0, true).unwrap();
    let (_, p) = predictions::load(&s, &key).unwrap();
    let copied = predictions::import(&s, p).unwrap();
    assert!(propagation::info(&s, &copied)
        .unwrap()
        .unwrap()
        .verified_at
        .is_none());
    assert!(propagation::check_approval(&s, &copied, 0).is_err());
    propagation::verify(&s, &copied, 0, true).unwrap();
    propagation::check_approval(&s, &copied, 0).unwrap();
    predictions::reject(&s, &copied).unwrap();
    assert!(propagation::check_approval(&s, &copied, 0).is_err());
}
#[test]
fn fingerprints_rank_candidates_only_and_detect_source_changes() {
    let (_tmp, s, ids) = setup();
    let before = a::load(&s, &ids[0]).unwrap();
    let r = fingerprints::index(&s, false, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(r.indexed, 4);
    assert!(r.errors.is_empty());
    let r = fingerprints::index(&s, false, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(r.reused, 4);
    let ranked = fingerprints::rank(&s, &ids[0], "group", 64).unwrap();
    assert_eq!(ranked.len(), 3);
    assert!(ranked.windows(2).all(|w| w[0].distance <= w[1].distance));
    assert!(ranked.iter().all(|r| r.same_group));
    assert_eq!(a::load(&s, &ids[0]).unwrap(), before);
    let dash = fingerprints::dashboard(&s).unwrap();
    assert_eq!(dash.images, 4);
    assert_eq!(dash.indexed, 4);
    assert_eq!(dash.group_count, 1);
    assert_eq!(dash.event_counts["positive"], 1);
    assert!(dash.low_detail_candidates >= 1);
    fs::write(
        library::resolve(&s, &library::record(&s, &ids[1]).unwrap().path).unwrap(),
        b"changed",
    )
    .unwrap();
    let r = fingerprints::index(&s, false, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(r.errors.len(), 1);
    assert_eq!(fingerprints::dashboard(&s).unwrap().index_errors, 1);
    assert_eq!(
        fingerprints::rank(&s, &ids[0], "project", 64)
            .unwrap()
            .len(),
        2
    );
    assert!(fingerprints::rank(&s, &ids[0], "invalid", 8).is_err());
    assert!(fingerprints::index(&s, true, &AtomicBool::new(true), |_| {}).is_err());
}
#[test]
fn snapshot_identity_is_reproducible_and_freezes_labels_metadata_and_video_splits() {
    let (_tmp, mut s, ids) = setup();
    let trip = metadata::save_trip(
        &s,
        labelflow_lib::domain::review::Trip {
            id: String::new(),
            name: "M5 trip".into(),
            route: "Route".into(),
            camera_id: "cam".into(),
            distance_km: Some(2.),
            notes: String::new(),
            revision: 0,
        },
    )
    .unwrap();
    let mut metadata = metadata::load(&s, &ids[0]).unwrap();
    metadata.trip_id = Some(trip.id.clone());
    metadata.tags = vec!["hard_negative:median".into()];
    metadata::save(&mut s, metadata).unwrap();
    let first = snapshots::create(&s, &opts(), &AtomicBool::new(false), |_| {}).unwrap();
    let mut renamed = opts();
    renamed.name = "Another name".into();
    let again = snapshots::create(&s, &renamed, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(first.id, again.id);
    assert_eq!(first.name, again.name);
    assert_eq!(snapshots::list(&s).unwrap().len(), 1);
    let check = snapshots::verify(&s, &first.id, true, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(check.images, 4);
    assert_eq!(check.split_counts.len(), 1);
    let manifest = snapshots::manifest(&s, &first.id).unwrap();
    assert!(manifest
        .images
        .iter()
        .all(|i| i.group == format!("trip:{}", trip.id)));
    assert_eq!(manifest.trips[0], trip);
    let original = manifest.images.iter().find(|i| i.id == ids[0]).unwrap();
    let frozen = snapshots::document(&s, original).unwrap();
    assert_eq!(original.metadata.tags, ["hard_negative:median"]);
    let mut current = a::load(&s, &ids[0]).unwrap();
    current.event_label = Some("uncertain".into());
    current.masks[0] = a::pack(
        "cityscapes_semantic".into(),
        W,
        H,
        &vec![255; (W * H) as usize],
    );
    a::save(&mut s, current, &id()).unwrap();
    assert_eq!(snapshots::document(&s, original).unwrap(), frozen);
    assert_eq!(frozen.event_label.as_deref(), Some("positive"));
    let second = snapshots::create(&s, &opts(), &AtomicBool::new(false), |_| {}).unwrap();
    assert_ne!(first.id, second.id);
    assert!(s
        .connection
        .execute("UPDATE dataset_snapshots SET name='changed'", [])
        .is_err());
    assert!(s
        .connection
        .execute("DELETE FROM snapshot_objects", [])
        .is_err());
}
#[test]
fn cancelled_failed_or_invalid_snapshots_publish_nothing() {
    let (_tmp, s, ids) = setup();
    let cancel = AtomicBool::new(false);
    assert_eq!(
        snapshots::create(&s, &opts(), &cancel, |_| cancel
            .store(true, Ordering::Relaxed))
        .unwrap_err()
        .code,
        ErrorCode::Cancelled
    );
    assert!(snapshots::list(&s).unwrap().is_empty());
    assert_eq!(
        s.connection
            .query_row("SELECT count(*) FROM snapshot_objects", [], |r| r
                .get::<_, u32>(0))
            .unwrap(),
        0
    );
    let mut bad = opts();
    bad.train_percent = 95;
    bad.val_percent = 10;
    assert!(snapshots::create(&s, &bad, &AtomicBool::new(false), |_| {}).is_err());
    fs::remove_file(library::resolve(&s, &library::record(&s, &ids[1]).unwrap().path).unwrap())
        .unwrap();
    assert!(snapshots::create(&s, &opts(), &AtomicBool::new(false), |_| {}).is_err());
    assert!(snapshots::list(&s).unwrap().is_empty());
}
#[test]
fn frozen_export_uses_old_gt_lossless_masks_checksums_and_never_overwrites() {
    let (tmp, mut s, ids) = setup();
    let first = snapshots::create(&s, &opts(), &AtomicBool::new(false), |_| {}).unwrap();
    let manifest = snapshots::manifest(&s, &first.id).unwrap();
    let frozen_image = manifest.images.iter().find(|i| i.id == ids[0]).unwrap();
    let frozen = snapshots::document(&s, frozen_image).unwrap();
    let mut live = a::load(&s, &ids[0]).unwrap();
    live.event_label = Some("uncertain".into());
    live.masks.clear();
    live.annotations.clear();
    a::save(&mut s, live, &id()).unwrap();
    let before = a::load(&s, &ids[0]).unwrap();
    let out =
        snapshots::export(&s, &first.id, tmp.path(), &AtomicBool::new(false), |_| {}).unwrap();
    let doc: Document = serde_json::from_slice(
        &fs::read(out.join(format!("annotations/{}.json", ids[0]))).unwrap(),
    )
    .unwrap();
    assert_eq!(doc, frozen);
    assert_eq!(
        a::sha(&fs::read(out.join("manifest.json")).unwrap()),
        first.id
    );
    for mask in &frozen.masks {
        let (_, _, pixels) = a::decode_png(
            &fs::read(out.join(format!(
                "masks/{}/{}/{}.png",
                mask.layer_id, frozen_image.split, ids[0]
            )))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(pixels, a::unpack(mask).unwrap());
    }
    let hashes: std::collections::BTreeMap<String, String> =
        serde_json::from_slice(&fs::read(out.join("checksums.json")).unwrap()).unwrap();
    for (path, hash) in hashes {
        assert_eq!(a::sha(&fs::read(out.join(path)).unwrap()), hash);
    }
    let other =
        snapshots::export(&s, &first.id, tmp.path(), &AtomicBool::new(false), |_| {}).unwrap();
    assert_ne!(out, other);
    assert_eq!(a::load(&s, &ids[0]).unwrap(), before);
    let cancel = AtomicBool::new(false);
    assert!(
        snapshots::export(&s, &first.id, tmp.path(), &cancel, |_| cancel
            .store(true, Ordering::Relaxed))
        .is_err()
    );
    assert!(out.is_dir());
    fs::write(
        library::resolve(&s, &library::record(&s, &ids[1]).unwrap().path).unwrap(),
        b"changed",
    )
    .unwrap();
    assert!(snapshots::verify(&s, &first.id, true, &AtomicBool::new(false), |_| {}).is_err());
    assert!(snapshots::export(&s, &first.id, tmp.path(), &AtomicBool::new(false), |_| {}).is_err());
}
#[test]
fn backup_restores_frozen_objects_fingerprints_and_propagation_provenance() {
    let (tmp, s, ids) = setup();
    let version = snapshots::create(&s, &opts(), &AtomicBool::new(false), |_| {}).unwrap();
    fingerprints::index(&s, false, &AtomicBool::new(false), |_| {}).unwrap();
    let key = propagation::generate(&s, &ids[0], &ids[1..2], &AtomicBool::new(false), |_| {})
        .unwrap()
        .created[0]
        .key
        .clone();
    propagation::verify(&s, &key, 0, true).unwrap();
    let backup = backup::backup(&s, tmp.path()).unwrap();
    let restored = tmp.path().join("restored");
    fs::create_dir(&restored).unwrap();
    backup::restore_backup(&backup, &restored).unwrap();
    let restored = project::open(&restored).unwrap();
    snapshots::verify(
        &restored,
        &version.id,
        true,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert!(propagation::info(&restored, &key)
        .unwrap()
        .unwrap()
        .verified_at
        .is_some());
    assert_eq!(fingerprints::dashboard(&restored).unwrap().indexed, 4);
}
#[test]
fn schema_five_upgrade_backs_up_and_preserves_gt() {
    let (_tmp, s, ids) = setup();
    let before = a::load(&s, &ids[0]).unwrap();
    let root = s.root.clone();
    s.connection.execute_batch("DROP TABLE propagation_links; DROP TABLE media_fingerprints; DROP TABLE dataset_snapshots; DROP TABLE snapshot_objects; UPDATE projects SET schema_version=5; PRAGMA user_version=5;").unwrap();
    drop(s);
    let upgraded = project::open(&root).unwrap();
    assert_eq!(upgraded.project.schema_version, SCHEMA_VERSION);
    assert_eq!(a::load(&upgraded, &ids[0]).unwrap(), before);
    assert!(fs::read_dir(root.join(".labelflow/backups"))
        .unwrap()
        .any(|p| p
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("schema-5-")));
}
#[test]
fn failed_provenance_insert_rolls_back_prediction_and_never_touches_gt() {
    let (_tmp, s, ids) = setup();
    let before = a::load(&s, &ids[1]).unwrap();
    s.connection.execute_batch("CREATE TRIGGER fail_link BEFORE INSERT ON propagation_links BEGIN SELECT RAISE(ABORT,'injected audit failure');END;").unwrap();
    let report =
        propagation::generate(&s, &ids[0], &ids[1..2], &AtomicBool::new(false), |_| {}).unwrap();
    assert!(report.created.is_empty());
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(
        s.connection
            .query_row("SELECT count(*) FROM review_predictions", [], |r| r
                .get::<_, u32>(0))
            .unwrap(),
        0
    );
    assert_eq!(a::load(&s, &ids[1]).unwrap(), before);
}
#[test]
fn corrupt_snapshot_objects_fail_verification_before_export_publication() {
    let (tmp, s, _ids) = setup();
    let version = snapshots::create(&s, &opts(), &AtomicBool::new(false), |_| {}).unwrap();
    let mut forged = snapshots::manifest(&s, &version.id).unwrap();
    forged.profile.layers[0].id = "../../escape".into();
    let json = a::json(&forged).unwrap();
    let fake_id = a::sha(json.as_bytes());
    s.connection.execute("INSERT INTO dataset_snapshots(id,name,manifest_json,image_count) VALUES(?1,'forged',?2,4)",rusqlite::params![fake_id,json]).unwrap();
    assert!(snapshots::verify(&s, &fake_id, false, &AtomicBool::new(false), |_| {}).is_err());
    assert!(snapshots::export(&s, &fake_id, tmp.path(), &AtomicBool::new(false), |_| {}).is_err());
    s.connection
        .execute_batch(
            "DROP TRIGGER snapshot_object_update; UPDATE snapshot_objects SET document_json='{}';",
        )
        .unwrap();
    assert!(snapshots::verify(&s, &version.id, false, &AtomicBool::new(false), |_| {}).is_err());
    assert!(snapshots::create(&s, &opts(), &AtomicBool::new(false), |_| {}).is_err());
    assert!(
        snapshots::export(&s, &version.id, tmp.path(), &AtomicBool::new(false), |_| {}).is_err()
    );
    assert!(!fs::read_dir(tmp.path()).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with("LabelFlow-snapshot-")));
}
#[test]
#[ignore = "Creates an isolated M5 native desktop project"]
fn prepare_m5_desktop_project() {
    let (_tmp, s, ids) = setup();
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.cache/m5-acceptance");
    fs::create_dir_all(&parent).unwrap();
    // Persist fixtures only in the explicitly named workspace acceptance folder.
    let destination = parent.join(id());
    fs::create_dir(&destination).unwrap();
    let backup = backup::backup(&s, &destination).unwrap();
    drop(s);
    let out = destination.join("project");
    fs::create_dir(&out).unwrap();
    backup::restore_backup(&backup, &out).unwrap();
    let s = project::open(&out).unwrap();
    for media in &ids {
        let r = library::record(&s, media).unwrap();
        let p = destination.join(&r.name);
        fs::copy(&r.path, &p).unwrap();
        s.connection
            .execute(
                "UPDATE media SET path=?1 WHERE id=?2",
                rusqlite::params![p.to_string_lossy(), media],
            )
            .unwrap();
    }
    fs::write(parent.join("latest.json"),a::json(&serde_json::json!({"project":s.root,"anchorId":ids[0],"targetId":ids[1],"nextId":ids[2],"flatId":ids[3]})).unwrap()).unwrap();
}
