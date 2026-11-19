use labelflow_lib::{
    domain::{annotation::*, ErrorCode},
    services::{annotation as a, interchange as io, library, profile, project},
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/calibration-1920x1080.png")
}
fn setup() -> (tempfile::TempDir, project::ProjectSession, String) {
    let tmp = tempfile::tempdir().unwrap();
    let mut s =
        project::create_with_profile(tmp.path(), "Road M1", "road-encroachment-v1").unwrap();
    library::import_paths(&mut s, &[fixture()], false, &AtomicBool::new(false), |_| {}).unwrap();
    let media = library::all(&s).unwrap()[0].id.clone();
    (tmp, s, media)
}
fn document(s: &project::ProjectSession, media: &str) -> Document {
    let mut d = a::load(s, media).unwrap();
    d.annotations.push(Annotation {
        id: id(),
        layer_id: "candidate_objects".into(),
        class_id: Some("candidate-2".into()),
        geometry: Geometry::Bbox {
            x: 100.25,
            y: 200.75,
            width: 350.5,
            height: 150.25,
        },
        source: "manual".into(),
    });
    let record = library::record(s, media).unwrap();
    let mut semantic = vec![255; record.width as usize * record.height as usize];
    semantic[0] = 0;
    semantic[1] = 13;
    *semantic.last_mut().unwrap() = 18;
    d.masks.push(a::pack(
        "cityscapes_semantic".into(),
        record.width,
        record.height,
        &semantic,
    ));
    for (i, layer) in ["road_extent", "traffic_exclusion", "suspected_region"]
        .iter()
        .enumerate()
    {
        let mut pixels = vec![0; semantic.len()];
        pixels[i + 7] = 1;
        d.masks.push(a::pack(
            layer.to_string(),
            record.width,
            record.height,
            &pixels,
        ));
    }
    d.review_status = "approved".into();
    d.event_label = Some("uncertain".into());
    d.notes = "Smoke test labels, not training ground truth".into();
    d
}
#[test]
fn profile_ids_are_train_ids_and_regions_are_independent() {
    let p = profile::definition("road-encroachment-v1");
    assert_eq!(
        p.classes
            .iter()
            .filter(|c| c.namespace == "cityscapes" && !c.is_ignore)
            .map(|c| c.export_id)
            .collect::<Vec<_>>(),
        (0..19).collect::<Vec<_>>()
    );
    assert_eq!(
        p.classes.iter().find(|c| c.is_ignore).unwrap().export_id,
        255
    );
    assert_eq!(
        p.layers.iter().filter(|l| l.layer_type == "region").count(),
        3
    );
    assert_eq!(
        p.classes
            .iter()
            .filter(|c| c.namespace == "candidate")
            .count(),
        5
    );
}

#[test]
fn library_pagination_filters_and_literal_wildcard_search_use_valid_sql() {
    let (_tmp, mut s, media) = setup();
    let page = library::page(&s, 0, 40, "1920", "").unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].id, media);
    assert!(library::page(&s, 1, 1, "", "").unwrap().items.is_empty());
    for query in ["%", "!", "calibration_"] {
        assert_eq!(library::page(&s, 0, 40, query, "").unwrap().total, 0);
    }
    assert_eq!(library::page(&s, 0, 40, "", "approved").unwrap().total, 0);
    let doc = document(&s, &media);
    a::save(&mut s, doc, &id()).unwrap();
    assert_eq!(library::page(&s, 0, 40, "", "approved").unwrap().total, 1);
    assert!(library::page(&s, 0, 201, "", "").is_err());
    assert!(library::page(&s, 0, 40, "", "unknown").is_err());
}

#[test]
fn indexed_png_import_preserves_indices_instead_of_palette_colors() {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 3, 1);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::Eight);
        let palette = (0..256)
            .flat_map(|i| [255 - i as u8, 32, 99])
            .collect::<Vec<_>>();
        encoder.set_palette(palette);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[0, 13, 255])
            .unwrap();
    }
    let (w, h, pixels) = a::decode_png(&bytes).unwrap();
    assert_eq!((w, h), (3, 1));
    assert_eq!(pixels, [0, 13, 255]);
}
#[test]
fn document_masks_log_and_revisions_survive_reopen_and_backup() {
    let (tmp, mut s, media) = setup();
    let mut doc = document(&s, &media);
    let operation = id();
    assert_eq!(a::save(&mut s, doc.clone(), &operation).unwrap(), 1);
    assert_eq!(a::save(&mut s, doc.clone(), &operation).unwrap(), 1);
    assert_eq!(
        a::save(&mut s, doc.clone(), &id()).unwrap_err().code,
        ErrorCode::Conflict
    );
    doc.revision = 1;
    doc.notes = "updated".into();
    a::save(&mut s, doc, &id()).unwrap();
    let saved = a::load(&s, &media).unwrap();
    let output = tempfile::tempdir().unwrap();
    let backup = io::backup(&s, output.path()).unwrap();
    drop(s);
    let reopened = project::open(tmp.path()).unwrap();
    assert_eq!(a::load(&reopened, &media).unwrap(), saved);
    let restore = tempfile::tempdir().unwrap();
    io::restore_backup(&backup, restore.path()).unwrap();
    let restored = project::open(restore.path()).unwrap();
    assert_eq!(a::load(&restored, &media).unwrap(), saved);
    assert!(io::restore_backup(&backup, restore.path()).is_err());
}
#[test]
fn validation_prevents_cross_namespace_invalid_masks_and_geometry() {
    let (_tmp, mut s, media) = setup();
    let doc = document(&s, &media);
    let mut bad = doc.clone();
    bad.annotations[0].class_id = Some("cityscapes-0".into());
    assert!(a::save(&mut s, bad, &id()).is_err());
    let mut bad = doc.clone();
    bad.masks[0].width = 100;
    assert!(a::save(&mut s, bad, &id()).is_err());
    let mut bad = doc.clone();
    bad.masks[1].runs = vec![255, 1920 * 1080];
    assert!(a::save(&mut s, bad, &id()).is_err());
    let mut bad = doc.clone();
    bad.annotations[0].geometry = Geometry::Bbox {
        x: -1.0,
        y: 0.0,
        width: 10.0,
        height: 10.0,
    };
    assert!(a::save(&mut s, bad, &id()).is_err());
    let mut bad = doc;
    bad.annotations[0].geometry = Geometry::Polygon {
        points: vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 10.0, y: 10.0 },
            Point { x: 0.0, y: 10.0 },
            Point { x: 10.0, y: 0.0 },
        ],
        holes: vec![],
    };
    assert!(a::save(&mut s, bad, &id()).is_err());
    assert_eq!(a::load(&s, &media).unwrap().revision, 0);
}
#[test]
fn masks_have_exact_pixel_ids_and_export_is_atomic_and_grouped() {
    let (_tmp, mut s, media) = setup();
    let doc = document(&s, &media);
    a::save(&mut s, doc.clone(), &id()).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let options = ExportOptions {
        format: "all".into(),
        approved_only: true,
        seed: "repeatable".into(),
        train_percent: 80,
        val_percent: 10,
        skip_unsupported: false,
    };
    let export = io::export(&s, temp.path(), &options, &AtomicBool::new(false), |_| {}).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(Path::new(&export.path).join("manifest.json")).unwrap())
            .unwrap();
    let split = manifest["images"][0]["split"].as_str().unwrap();
    let bytes = fs::read(
        Path::new(&export.path).join(format!("masks/cityscapes_semantic/{split}/{media}.png")),
    )
    .unwrap();
    let (w, h, pixels) = a::decode_png(&bytes).unwrap();
    assert_eq!((w, h), (1920, 1080));
    assert_eq!(pixels[0], 0);
    assert_eq!(pixels[1], 13);
    assert_eq!(pixels[2], 255);
    assert_eq!(*pixels.last().unwrap(), 18);
    let labels =
        fs::read_to_string(Path::new(&export.path).join(format!("labels/{split}/{media}.txt")))
            .unwrap();
    let parsed = io::parse_yolo(&labels, 1920, 1080).unwrap();
    assert_eq!(parsed[0].0, 2);
    if let (
        Geometry::Bbox {
            x,
            y,
            width,
            height,
        },
        Geometry::Bbox {
            x: rx,
            y: ry,
            width: rw,
            height: rh,
        },
    ) = (&doc.annotations[0].geometry, &parsed[0].1)
    {
        assert!(
            (x - rx).abs() < 1e-6
                && (y - ry).abs() < 1e-6
                && (width - rw).abs() < 1e-6
                && (height - rh).abs() < 1e-6
        );
    } else {
        panic!("expected bbox");
    }
    assert_eq!(
        io::split_for("trip", "seed", 80, 10).unwrap(),
        io::split_for("trip", "seed", 80, 10).unwrap()
    );
    let before = fs::read(Path::new(&export.path).join("manifest.json")).unwrap();
    assert!(io::export(&s, temp.path(), &options, &AtomicBool::new(true), |_| {}).is_err());
    assert_eq!(
        fs::read(Path::new(&export.path).join("manifest.json")).unwrap(),
        before
    );
}
#[test]
fn missing_and_corrupt_masks_are_reported_and_preserved() {
    let (_tmp, mut s, media) = setup();
    let doc = document(&s, &media);
    a::save(&mut s, doc, &id()).unwrap();
    let path: String = s
        .connection
        .query_row("SELECT relative_path FROM mask_assets LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    let path = s.root.join(".labelflow").join(path);
    let before = fs::read(&path).unwrap();
    fs::write(&path, b"corrupt").unwrap();
    assert!(a::load(&s, &media).is_err());
    assert!(labelflow_lib::services::recovery::inspect(&s)
        .unwrap()
        .iter()
        .any(|s| s.contains("checksum mismatch")));
    fs::write(&path, before).unwrap();
    assert!(a::load(&s, &media).is_ok());
    fs::remove_file(path).unwrap();
    assert!(a::load(&s, &media).is_err());
    assert!(labelflow_lib::services::recovery::inspect(&s)
        .unwrap()
        .iter()
        .any(|s| s.contains("Missing/unreadable")));
}

#[test]
fn reopen_flags_interrupted_jobs_and_preserves_orphan_masks() {
    let (tmp, s, _) = setup();
    let job = labelflow_lib::jobs::start(&s.connection, "import_media", "{}").unwrap();
    let orphan = tmp.path().join(".labelflow/masks/interrupted.tmp");
    fs::write(&orphan, b"recovery evidence").unwrap();
    drop(s);
    let reopened = project::open(tmp.path()).unwrap();
    let status: String = reopened
        .connection
        .query_row("SELECT status FROM jobs WHERE id=?1", [&job], |r| r.get(0))
        .unwrap();
    assert_eq!(status, "failed");
    let warnings = labelflow_lib::services::recovery::inspect(&reopened).unwrap();
    assert!(warnings.iter().any(|s| s.contains("Unreferenced")));
    assert!(warnings
        .iter()
        .any(|s| s.contains("Interrupted by process exit")));
    assert_eq!(fs::read(orphan).unwrap(), b"recovery evidence");
}
#[test]
fn import_deduplicates_cancels_rebuilds_thumbnails_and_preserves_sources() {
    let (tmp, mut s, media) = setup();
    let before = fs::read(fixture()).unwrap();
    let report =
        library::import_paths(&mut s, &[fixture()], false, &AtomicBool::new(false), |_| {})
            .unwrap();
    assert_eq!(report.duplicates, 1);
    let first = library::thumbnail(&s, &media).unwrap();
    fs::write(
        tmp.path()
            .join(format!(".labelflow/thumbnails/{media}.jpg")),
        b"broken",
    )
    .unwrap();
    assert_eq!(library::thumbnail(&s, &media).unwrap(), first);
    assert!(
        library::import_paths(&mut s, &[fixture()], false, &AtomicBool::new(true), |_| {})
            .unwrap()
            .cancelled
    );
    assert_eq!(fs::read(fixture()).unwrap(), before);
    let copydir = tempfile::tempdir().unwrap();
    let mut target = project::create(copydir.path(), "Managed").unwrap();
    let r = library::import_paths(
        &mut target,
        &[fixture()],
        true,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(r.imported, 1);
    assert_eq!(library::all(&target).unwrap()[0].import_mode, "copied");
    assert_eq!(
        library::import_paths(
            &mut target,
            &[fixture()],
            true,
            &AtomicBool::new(false),
            |_| {}
        )
        .unwrap()
        .duplicates,
        1
    );
}
#[test]
fn basic_yolo_and_semantic_import_require_explicit_mapping() {
    let (_tmp, mut s, media) = setup();
    let temp = tempfile::tempdir().unwrap();
    fs::write(
        temp.path().join(format!("{media}.txt")),
        "4 0.5 0.5 0.2 0.1\n",
    )
    .unwrap();
    let prepared = io::inspect_yolo(&s, temp.path()).unwrap();
    assert!(io::import_yolo(&mut s, prepared, &[]).is_err());
    let prepared = io::inspect_yolo(&s, temp.path()).unwrap();
    assert_eq!(
        io::import_yolo(
            &mut s,
            prepared,
            &[ClassMapping {
                source: 4,
                target: "candidate-2".into()
            }]
        )
        .unwrap()
        .imported,
        1
    );
    let doc = a::load(&s, &media).unwrap();
    assert_eq!(doc.annotations[0].class_id.as_deref(), Some("candidate-2"));
    let path = temp.path().join("mask.png");
    let mut values = vec![255; 1920 * 1080];
    values[0] = 7;
    fs::write(&path, a::encode_png(1920, 1080, &values).unwrap()).unwrap();
    let prepared = io::inspect_mask(&s, &path, &media, "cityscapes_semantic").unwrap();
    assert!(io::apply_mask(&mut s, prepared, &[]).is_err());
    let prepared = io::inspect_mask(&s, &path, &media, "cityscapes_semantic").unwrap();
    let doc = io::apply_mask(
        &mut s,
        prepared,
        &[
            PixelMapping {
                source: 7,
                target: 0,
            },
            PixelMapping {
                source: 255,
                target: 255,
            },
        ],
    )
    .unwrap();
    assert_eq!(a::unpack(&doc.masks[0]).unwrap()[0], 0);
}

#[test]
fn abrupt_exit_recovers_committed_wal_and_external_masks_exactly() {
    let tmp = tempfile::tempdir().unwrap();
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "annotation_crash_child", "--ignored"])
        .env("LABELFLOW_CRASH_TEST_ROOT", tmp.path())
        .status()
        .unwrap();
    assert!(status.success());
    let expected: Document =
        serde_json::from_slice(&fs::read(tmp.path().join("expected.json")).unwrap()).unwrap();
    let s = project::open(tmp.path()).unwrap();
    assert_eq!(a::load(&s, &expected.media_id).unwrap(), expected);
    assert_eq!(library::all(&s).unwrap().len(), 1);
}

#[test]
#[ignore = "Invoked by abrupt-exit parent test"]
fn annotation_crash_child() {
    let root = PathBuf::from(std::env::var_os("LABELFLOW_CRASH_TEST_ROOT").unwrap());
    let mut s =
        project::create_with_profile(&root, "Crash fixture", "road-encroachment-v1").unwrap();
    library::import_paths(&mut s, &[fixture()], false, &AtomicBool::new(false), |_| {}).unwrap();
    let media = library::all(&s).unwrap()[0].id.clone();
    let doc = document(&s, &media);
    a::save(&mut s, doc, &id()).unwrap();
    let expected = a::load(&s, &media).unwrap();
    a::atomic_file(
        &root.join("expected.json"),
        &serde_json::to_vec(&expected).unwrap(),
    )
    .unwrap();
    // Exit with an open SQLite/WAL connection and OS lock; destructors do not run.
    std::process::exit(0);
}

#[test]
#[ignore = "Requires the explicitly authorized real dataset and writes acceptance artifacts in workspace .cache"]
fn real_dataset_acceptance() {
    let dataset = PathBuf::from(
        std::env::var_os("LABELFLOW_REAL_DATASET")
            .expect("Set LABELFLOW_REAL_DATASET to the authorized train directory"),
    );
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.cache/m1-acceptance");
    fs::create_dir_all(&base).unwrap();
    let run = base.join(id());
    fs::create_dir(&run).unwrap();
    let mut dirs = vec![dataset];
    let mut paths = Vec::new();
    while let Some(dir) = dirs.pop() {
        let mut children = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect::<Vec<_>>();
        children.sort();
        for p in children {
            if p.is_dir() {
                dirs.push(p);
            } else if p.extension().is_some_and(|e| e == "png") {
                paths.push(p);
            }
        }
    }
    paths.sort_by_key(|p| {
        (
            !p.to_string_lossy().contains("14_Otto_Lilienthal_Str_24"),
            p.clone(),
        )
    });
    assert!(paths.len() >= 100);
    paths.truncate(100);
    let mut s = project::create_with_profile(
        &run,
        "100 real images — automated smoke labels",
        "road-encroachment-v1",
    )
    .unwrap();
    let start = std::time::Instant::now();
    let report = library::import_paths(&mut s, &paths, false, &AtomicBool::new(false), |p| {
        if p.completed.is_multiple_of(10) {
            println!("import {}/{}", p.completed, p.total);
        }
    })
    .unwrap();
    assert_eq!(report.imported, 100);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let records = library::all(&s).unwrap();
    assert_eq!(
        records
            .iter()
            .map(|r| &r.sha256)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        100,
        "Acceptance requires 100 distinct source image contents"
    );
    let mut expected = Vec::new();
    for (i, record) in records.iter().enumerate() {
        let doc = document(&s, &record.id);
        let r = a::save(&mut s, doc, &id()).unwrap();
        assert_eq!(r, 1);
        expected.push(a::load(&s, &record.id).unwrap());
        if i % 10 == 9 {
            println!("labeled {}/100", i + 1);
        }
    }
    let first = records[0].id.clone();
    library::select(&mut s, &first).unwrap();
    drop(s);
    let reopened = project::open(&run).unwrap();
    for doc in &expected {
        assert_eq!(a::load(&reopened, &doc.media_id).unwrap(), *doc);
    }
    let exports = base.join("exports");
    fs::create_dir_all(&exports).unwrap();
    let options = ExportOptions {
        format: "all".into(),
        approved_only: true,
        seed: "real-100".into(),
        train_percent: 80,
        val_percent: 10,
        skip_unsupported: false,
    };
    let result = io::export(
        &reopened,
        &exports,
        &options,
        &AtomicBool::new(false),
        |p| {
            if p.completed.is_multiple_of(10) {
                println!("export {}/{}", p.completed, p.total);
            }
        },
    )
    .unwrap();
    assert_eq!(result.images, 100);
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(Path::new(&result.path).join("manifest.json")).unwrap())
            .unwrap();
    let mut groups = std::collections::HashMap::new();
    for image in manifest["images"].as_array().unwrap() {
        let id = image["id"].as_str().unwrap();
        let split = image["split"].as_str().unwrap();
        let group = image["group"].as_str().unwrap();
        if let Some(previous) = groups.insert(group, split) {
            assert_eq!(previous, split);
        }
        let record = records.iter().find(|r| r.id == id).unwrap();
        let (_, _, pixels) = a::decode_png(
            &fs::read(
                Path::new(&result.path).join(format!("masks/cityscapes_semantic/{split}/{id}.png")),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(pixels.len(), record.width as usize * record.height as usize);
        assert_eq!((pixels[0], pixels[1], *pixels.last().unwrap()), (0, 13, 18));
        let text =
            fs::read_to_string(Path::new(&result.path).join(format!("labels/{split}/{id}.txt")))
                .unwrap();
        let parsed = io::parse_yolo(&text, record.width, record.height).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(
            a::sha(&fs::read(library::resolve(&reopened, &record.path).unwrap()).unwrap()),
            record.sha256
        );
    }
    let backup = io::backup(&reopened, &base).unwrap();
    let restored = base.join(format!("restored-{}", id()));
    fs::create_dir(&restored).unwrap();
    io::restore_backup(&backup, &restored).unwrap();
    let copy = project::open(&restored).unwrap();
    for doc in &expected {
        assert_eq!(a::load(&copy, &doc.media_id).unwrap(), *doc);
    }
    let data = serde_json::json!({"images":100,"project":run,"export":result.path,"backup":backup,"restored":restored,"elapsedSeconds":start.elapsed().as_secs_f64(),"sourceUnchanged":true,"labelType":"automatically generated smoke labels; not ground truth"});
    fs::write(
        base.join("latest.json"),
        serde_json::to_vec_pretty(&data).unwrap(),
    )
    .unwrap();
    println!("{}", serde_json::to_string_pretty(&data).unwrap());
}
