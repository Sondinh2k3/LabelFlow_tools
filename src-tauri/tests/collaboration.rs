use labelflow_lib::{
    domain::{advanced::SnapshotOptions, annotation::*},
    services::{
        advanced::snapshots, annotation as a, interchange as export, library, profile, project,
        team,
    },
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};
fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/calibration-1920x1080.png")
}
fn setup() -> (tempfile::TempDir, project::ProjectSession, String) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("project");
    fs::create_dir(&root).unwrap();
    let mut s = project::create_with_profile(&root, "Generic", "generic-v1").unwrap();
    library::import_paths(&mut s, &[fixture()], false, &AtomicBool::new(false), |_| {}).unwrap();
    let media = library::all(&s).unwrap()[0].id.clone();
    (tmp, s, media)
}
fn objects(p: &Profile) -> Vec<&ClassDefinition> {
    p.classes
        .iter()
        .filter(|c| c.namespace == profile::OBJECT_NAMESPACE)
        .collect()
}

#[test]
fn custom_object_classes_are_saved_used_exported_and_protected() {
    let (tmp, mut s, media) = setup();
    assert_eq!(objects(&profile::current(&s).unwrap()).len(), 1);
    let p = profile::save_class(&mut s, None, None, "Xe ba gác", "#ff8800").unwrap();
    let p = profile::save_class(&mut s, None, None, "Helmet", "#00ff88")
        .map(|_| profile::current(&s).unwrap())
        .unwrap_or(p);
    let list = objects(&p);
    assert_eq!(list.len(), 3);
    assert_eq!(
        list.iter().map(|c| c.export_id).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    let cart = list.iter().find(|c| c.display_name == "Xe ba gác").unwrap();
    let cart_id = cart.id.clone();
    assert_eq!(cart.stable_key, "xe_ba_gác");
    // Duplicate names, bad colors and empty names are refused.
    assert!(profile::save_class(&mut s, None, None, "helmet", "#000000").is_err());
    assert!(profile::save_class(&mut s, None, None, "X", "red").is_err());
    assert!(profile::save_class(&mut s, None, None, "  ", "#000000").is_err());

    // Label with the new class; validation accepts it and export names it.
    let mut doc = a::load(&s, &media).unwrap();
    doc.annotations.push(Annotation {
        id: uuid::Uuid::new_v4().to_string(),
        layer_id: "candidate_objects".into(),
        class_id: Some(cart_id.clone()),
        geometry: Geometry::Bbox {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 50.0,
        },
        source: "manual".into(),
    });
    doc.review_status = "approved".into();
    a::save(&mut s, doc, &uuid::Uuid::new_v4().to_string()).unwrap();

    // Renaming keeps the export ID; deleting a used class is refused.
    profile::save_class(&mut s, None, Some(&cart_id), "Cargo tricycle", "#ff8801").unwrap();
    let renamed = profile::current(&s).unwrap();
    let renamed = objects(&renamed)
        .into_iter()
        .find(|c| c.id == cart_id)
        .cloned()
        .unwrap();
    assert_eq!(
        (renamed.export_id, renamed.display_name.as_str()),
        (1, "Cargo tricycle")
    );
    assert!(profile::delete_class(&mut s, &cart_id).is_err());
    let helmet = objects(&profile::current(&s).unwrap())
        .into_iter()
        .find(|c| c.display_name == "Helmet")
        .unwrap()
        .id
        .clone();
    profile::delete_class(&mut s, &helmet).unwrap();
    assert_eq!(objects(&profile::current(&s).unwrap()).len(), 2);

    let out = tmp.path().join("exports");
    fs::create_dir(&out).unwrap();
    let report = export::export(
        &s,
        &out,
        &ExportOptions {
            format: "yolo".into(),
            approved_only: true,
            seed: "s".into(),
            train_percent: 100,
            val_percent: 0,
            skip_unsupported: false,
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    let yaml = fs::read_to_string(Path::new(&report.path).join("data.yaml")).unwrap();
    assert!(yaml.contains("1: \"Cargo tricycle\""), "{yaml}");
    let label = fs::read_dir(Path::new(&report.path).join("labels/train"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert!(fs::read_to_string(label).unwrap().starts_with("1 "));

    // The unused built-in "object" class can be deleted and is not recreated on reopen.
    profile::delete_class(&mut s, "candidate-0").unwrap();
    let root = s.root.clone();
    drop(s);
    let s = project::open(&root).unwrap();
    let reopened = profile::current(&s).unwrap();
    let names = objects(&reopened)
        .iter()
        .map(|c| c.display_name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["Cargo tricycle"]);
}

#[test]
fn snapshots_stay_valid_after_classes_change() {
    let (_tmp, mut s, media) = setup();
    let mut doc = a::load(&s, &media).unwrap();
    doc.review_status = "approved".into();
    a::save(&mut s, doc, &uuid::Uuid::new_v4().to_string()).unwrap();
    let options = SnapshotOptions {
        name: "v1".into(),
        seed: "seed".into(),
        train_percent: 80,
        val_percent: 10,
        approved_only: true,
    };
    let version = snapshots::create(&s, &options, &AtomicBool::new(false), |_| {}).unwrap();
    profile::save_class(&mut s, None, None, "New class", "#123456").unwrap();
    assert!(snapshots::verify(&s, &version.id, false, &AtomicBool::new(false), |_| {}).is_ok());
}

#[test]
fn team_question_carries_labels_and_matches_by_content_hash() {
    let (tmp, mut s, media) = setup();
    let mut doc = a::load(&s, &media).unwrap();
    doc.annotations.push(Annotation {
        id: uuid::Uuid::new_v4().to_string(),
        layer_id: "candidate_objects".into(),
        class_id: Some(objects(&profile::current(&s).unwrap())[0].id.clone()),
        geometry: Geometry::Polygon {
            points: (0..500)
                .map(|i| {
                    let t = f64::from(i) / 500.0 * std::f64::consts::TAU;
                    Point {
                        x: 500.0 + 100.0 * t.cos(),
                        y: 500.0 + 100.0 * t.sin(),
                    }
                })
                .collect(),
            holes: vec![],
        },
        source: "manual".into(),
    });
    a::save(&mut s, doc, &uuid::Uuid::new_v4().to_string()).unwrap();
    let shapes = team::label_shapes(
        &a::load(&s, &media).unwrap(),
        &profile::current(&s).unwrap(),
    );
    assert_eq!(shapes.len(), 1);
    assert!(shapes[0].points.len() <= 64 && shapes[0].points.len() >= 50);

    let shared = tmp.path().join("shared");
    fs::create_dir(&shared).unwrap();
    let folder = team::prepare_folder(&shared).unwrap();
    let record = library::record(&s, &media).unwrap();
    let config = team::TeamConfig {
        user_id: uuid::Uuid::new_v4().to_string(),
        user_name: "Lan".into(),
        folders: Default::default(),
    };
    let (_, image) = library::checked_load(&s, &record).unwrap();
    team::write_preview(&folder, &record.sha256, &image).unwrap();
    team::comment(
        &folder,
        &config,
        team::NewComment {
            thread_id: None,
            media: Some(team::MediaRef {
                sha256: record.sha256.clone(),
                name: record.name.clone(),
                width: record.width,
                height: record.height,
            }),
            body: "Nên gán class nào?".into(),
            pin: Some(Point { x: 500.0, y: 500.0 }),
            labels: shapes,
            project: "Generic".into(),
        },
    )
    .unwrap();
    let threads = team::threads(team::Cache::default().scan(&folder).unwrap().0);
    assert_eq!(threads[0].media.sha256, record.sha256);
    assert_eq!(threads[0].comments[0].body, "Nên gán class nào?");
    assert!(team::read_preview(&folder, &record.sha256)
        .unwrap()
        .is_some());
}

fn semantic(p: &Profile) -> Vec<&ClassDefinition> {
    p.classes
        .iter()
        .filter(|c| c.namespace == profile::SEMANTIC_NAMESPACE)
        .collect()
}

#[test]
fn custom_semantic_classes_label_export_and_protect_ids() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("vn");
    fs::create_dir(&root).unwrap();
    let list: Vec<String> = [
        "Lòng đường #804080",
        "Bó vỉa",
        "Vỉa hè / lề đường",
        "Phương tiện",
        "Người",
        "Động vật",
        "Kết cấu cố định",
        "Nền",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    // A duplicate name is refused before the folder is touched.
    let mut bad = list.clone();
    bad.push("nền".into());
    assert!(project::create_with_classes(&root, "VN", "road-encroachment-v1", Some(&bad)).is_err());
    assert!(!root.join(".labelflow").exists());

    let mut s =
        project::create_with_classes(&root, "VN", "road-encroachment-v1", Some(&list)).unwrap();
    let p = profile::current(&s).unwrap();
    let classes = semantic(&p);
    assert_eq!(
        classes.iter().map(|c| c.export_id).collect::<Vec<_>>(),
        vec![0, 1, 2, 3, 4, 5, 6, 7, 255]
    );
    assert_eq!(classes[0].display_name, "Lòng đường");
    assert_eq!(classes[0].color, "#804080");
    assert!(classes[8].is_ignore);

    library::import_paths(&mut s, &[fixture()], false, &AtomicBool::new(false), |_| {}).unwrap();
    let media = library::all(&s).unwrap()[0].id.clone();
    let (w, h) = (1920u32, 1080u32);
    let mut pixels = vec![255u8; (w * h) as usize];
    pixels[..(w * 100) as usize].fill(7);
    pixels[(w * 500) as usize..].fill(0);
    let mut doc = a::load(&s, &media).unwrap();
    doc.masks = vec![a::pack(profile::SEMANTIC_LAYER.into(), w, h, &pixels)];
    doc.review_status = "approved".into();
    a::save(&mut s, doc.clone(), &uuid::Uuid::new_v4().to_string()).unwrap();

    // IDs outside the project's set are refused (Cityscapes 8..18 no longer exist).
    let mut wrong = a::load(&s, &media).unwrap();
    let mut other = pixels.clone();
    other[0] = 12;
    wrong.masks = vec![a::pack(profile::SEMANTIC_LAYER.into(), w, h, &other)];
    assert!(a::save(&mut s, wrong, &uuid::Uuid::new_v4().to_string()).is_err());

    // Used classes cannot be deleted; renaming keeps the ID; unused ones can go.
    let nen = classes[7].id.clone();
    let curb = classes[1].id.clone();
    assert!(profile::delete_class(&mut s, &nen).is_err());
    profile::save_class(&mut s, None, Some(&nen), "Nền / khác", "#222222").unwrap();
    let added = profile::save_class(
        &mut s,
        Some(profile::SEMANTIC_NAMESPACE),
        None,
        "Hố đào",
        "#ff0000",
    )
    .unwrap();
    let hole = semantic(&added)
        .into_iter()
        .find(|c| c.display_name == "Hố đào")
        .unwrap()
        .clone();
    assert_eq!(hole.export_id, 8);
    profile::delete_class(&mut s, &curb).unwrap();
    assert!(profile::delete_class(&mut s, "cityscapes-ignore").is_err());
    assert!(profile::save_class(&mut s, None, Some("cityscapes-ignore"), "X", "#000000").is_err());

    // Deletions survive reopen; the ignore class stays.
    drop(s);
    let s = project::open(&root).unwrap();
    let reopened = profile::current(&s).unwrap();
    let ids = semantic(&reopened)
        .iter()
        .map(|c| c.export_id)
        .collect::<Vec<_>>();
    assert_eq!(ids, vec![0, 2, 3, 4, 5, 6, 7, 8, 255]);

    let out = tmp.path().join("exports");
    fs::create_dir(&out).unwrap();
    let report = export::export(
        &s,
        &out,
        &ExportOptions {
            format: "semantic".into(),
            approved_only: true,
            seed: "s".into(),
            train_percent: 100,
            val_percent: 0,
            skip_unsupported: false,
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    let mask = Path::new(&report.path)
        .join("masks")
        .join(profile::SEMANTIC_LAYER)
        .join("train")
        .join(format!("{media}.png"));
    let (_, _, exported) = a::decode_png(&fs::read(mask).unwrap()).unwrap();
    assert_eq!(exported, pixels);
    let classes_json = fs::read_to_string(Path::new(&report.path).join("classes.json")).unwrap();
    assert!(classes_json.contains("Nền / khác"));
    // QC: pixel counts per class and mostly-ignore images (rows 100..500 are 255 = 37%).
    let stats: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(Path::new(&report.path).join("semantic-stats.json")).unwrap(),
    )
    .unwrap();
    let road = stats["classes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == 0)
        .unwrap();
    assert_eq!(road["pixels"], u64::from(w * (h - 500)));
    assert_eq!(road["images"], 1);
    assert_eq!(
        stats["imagesOverTenPercentIgnore"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(report.warnings.iter().any(|w| w.contains("10% Ignore")));
    assert!(report
        .warnings
        .iter()
        .any(|w| w.contains("no exported pixels") && w.contains("Hố đào")));
}

#[test]
fn default_projects_keep_cityscapes_semantic_classes() {
    let (_tmp, s, _) = setup();
    let p = profile::current(&s).unwrap();
    let classes = semantic(&p);
    assert_eq!(classes.len(), 20);
    assert_eq!(classes[0].id, "cityscapes-0");
    assert_eq!(classes[18].export_id, 18);
    assert_eq!(classes[19].export_id, 255);
}
