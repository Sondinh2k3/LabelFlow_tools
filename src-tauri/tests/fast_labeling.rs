use labelflow_lib::{
    domain::annotation::*,
    import::masks::{self, MergeMode},
    services::{annotation as a, library, profile, project},
};
use std::{fs, path::Path, sync::atomic::AtomicBool};

const W: u32 = 64;
const H: u32 = 48;

fn png(path: &Path, pixels: &[u8]) {
    fs::write(path, a::encode_png(W, H, pixels).unwrap()).unwrap();
}
fn image(path: &Path, shade: u8) {
    let rgb = image::RgbImage::from_fn(W, H, |x, y| {
        image::Rgb([shade, (x * 3) as u8, (y * 5) as u8])
    });
    rgb.save(path).unwrap();
}
/// Project with 3 custom semantic classes and three distinct images.
fn setup() -> (tempfile::TempDir, project::ProjectSession, Vec<MediaRecord>) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("p");
    let sources = tmp.path().join("src");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&sources).unwrap();
    for (i, name) in ["frame_a", "frame_b", "frame_c"].iter().enumerate() {
        image(&sources.join(format!("{name}.png")), 40 * i as u8 + 10);
    }
    let classes: Vec<String> = ["Road", "Vehicle", "Background"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut s = project::create_with_classes(&root, "Fast", "generic-v1", Some(&classes)).unwrap();
    library::import_paths(&mut s, &[sources], false, &AtomicBool::new(false), |_| {}).unwrap();
    let mut records = library::all(&s).unwrap();
    records.sort_by(|x, y| x.name.cmp(&y.name));
    (tmp, s, records)
}
fn semantic(s: &project::ProjectSession, media: &str) -> Option<Vec<u8>> {
    a::load(s, media)
        .unwrap()
        .masks
        .iter()
        .find(|m| m.layer_id == profile::SEMANTIC_LAYER)
        .map(|m| a::unpack(m).unwrap())
}

#[test]
fn mask_folder_prelabels_many_images_and_keeps_human_and_approved_work() {
    let (tmp, mut s, records) = setup();
    // Human label on image A (top row = Vehicle); image C approved.
    let mut human = vec![255u8; (W * H) as usize];
    human[..W as usize].fill(1);
    let mut doc = a::load(&s, &records[0].id).unwrap();
    doc.masks = vec![a::pack(profile::SEMANTIC_LAYER.into(), W, H, &human)];
    a::save(&mut s, doc, &uuid::Uuid::new_v4().to_string()).unwrap();
    let mut approved = a::load(&s, &records[2].id).unwrap();
    approved.review_status = "approved".into();
    a::save(&mut s, approved, &uuid::Uuid::new_v4().to_string()).unwrap();

    // Model output uses its own IDs: 7 = road, 9 = car, 0 = other.
    let folder = tmp.path().join("prelabels");
    fs::create_dir_all(folder.join("nested")).unwrap();
    let mut model = vec![7u8; (W * H) as usize];
    model[(W * 40) as usize..].fill(9);
    model[0] = 0;
    png(&folder.join("frame_a.png"), &model);
    png(&folder.join("nested").join("frame_b.png"), &model);
    png(&folder.join("frame_c.png"), &model);
    png(&folder.join("unknown.png"), &model);
    fs::write(
        folder.join("wrong_size.png"),
        a::encode_png(8, 8, &[0; 64]).unwrap(),
    )
    .unwrap();
    fs::rename(folder.join("wrong_size.png"), folder.join("frame_x.png")).unwrap();

    let (prepared, inspection) = masks::inspect_folder(
        &s,
        &folder,
        profile::SEMANTIC_LAYER,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(inspection.matched, 2);
    assert_eq!(inspection.approved_skipped, 1);
    assert_eq!(inspection.values, vec![0, 7, 9]);
    assert!(inspection
        .warnings
        .iter()
        .any(|w| w.contains("unknown.png")));

    // Every value needs a mapping.
    let partial = [PixelMapping {
        source: 7,
        target: 0,
    }];
    let (again, _) = masks::inspect_folder(
        &s,
        &folder,
        profile::SEMANTIC_LAYER,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert!(masks::apply_folder(
        &mut s,
        again,
        &partial,
        MergeMode::FillUnlabeled,
        &AtomicBool::new(false),
        |_| {}
    )
    .is_err());

    let mapping = [
        PixelMapping {
            source: 7,
            target: 0,
        },
        PixelMapping {
            source: 9,
            target: 1,
        },
        PixelMapping {
            source: 0,
            target: 255,
        },
    ];
    let report = masks::apply_folder(
        &mut s,
        prepared,
        &mapping,
        MergeMode::FillUnlabeled,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(report.imported, 2, "{:?}", report.warnings);

    let a_pixels = semantic(&s, &records[0].id).unwrap();
    assert!(
        a_pixels[..W as usize].iter().all(|p| *p == 1),
        "human row kept"
    );
    assert_eq!(a_pixels[(W * 10) as usize], 0);
    assert_eq!(a_pixels[(W * 45) as usize], 1);
    let b_pixels = semantic(&s, &records[1].id).unwrap();
    assert_eq!(b_pixels[0], 255);
    assert_eq!(b_pixels[1], 0);
    assert_eq!(
        a::load(&s, &records[1].id).unwrap().review_status,
        "in_progress"
    );
    assert!(semantic(&s, &records[2].id).is_none(), "approved untouched");

    // Replace mode overwrites the human row.
    let (prepared, _) = masks::inspect_folder(
        &s,
        &folder,
        profile::SEMANTIC_LAYER,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    masks::apply_folder(
        &mut s,
        prepared,
        &mapping,
        MergeMode::Replace,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(semantic(&s, &records[0].id).unwrap()[5], 0);
}

#[test]
fn ignore_template_forces_255_on_same_size_unapproved_images() {
    let (tmp, mut s, records) = setup();
    let full = vec![0u8; (W * H) as usize];
    for r in &records[..2] {
        let mut doc = a::load(&s, &r.id).unwrap();
        doc.masks = vec![a::pack(profile::SEMANTIC_LAYER.into(), W, H, &full)];
        a::save(&mut s, doc, &uuid::Uuid::new_v4().to_string()).unwrap();
    }
    let mut approved = a::load(&s, &records[1].id).unwrap();
    approved.review_status = "approved".into();
    a::save(&mut s, approved, &uuid::Uuid::new_v4().to_string()).unwrap();

    let mut hood = vec![0u8; (W * H) as usize];
    hood[(W * (H - 8)) as usize..].fill(255);
    let path = tmp.path().join("hood.png");
    png(&path, &hood);
    let (prepared, inspection) = masks::inspect_template(&s, &path, None).unwrap();
    assert_eq!(inspection.images, 2);
    assert_eq!(inspection.approved_skipped, 1);
    assert_eq!(inspection.ignore_pixels, u64::from(W * 8));
    let report = masks::apply_template(&mut s, prepared, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(report.imported, 1);
    assert_eq!(report.duplicates, 1, "image without mask is already ignore");
    let pixels = semantic(&s, &records[0].id).unwrap();
    assert_eq!(pixels[0], 0);
    assert_eq!(pixels[(W * H - 1) as usize], 255);
    assert_eq!(
        semantic(&s, &records[1].id).unwrap()[(W * H - 1) as usize],
        0
    );

    let empty = tmp.path().join("empty.png");
    png(&empty, &vec![0u8; (W * H) as usize]);
    assert!(masks::inspect_template(&s, &empty, None).is_err());
}

#[test]
#[ignore = "Requires LABELFLOW_TEST_PYTHON with worker requirements installed"]
fn real_sam_worker_masks_clicks_at_canonical_resolution() {
    use labelflow_lib::ai::worker::{Config, Runtime};
    use serde_json::json;
    let python = std::path::PathBuf::from(
        std::env::var_os("LABELFLOW_TEST_PYTHON").expect("Set LABELFLOW_TEST_PYTHON"),
    );
    let (tmp, s, records) = setup();
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/ai/sam.json");
    let runtime = Runtime::default();
    assert!(
        runtime.sam(json!({}), runtime.token()).is_err(),
        "not started"
    );
    let model = runtime
        .start_sam(&Config { python, manifest }, tmp.path(), runtime.token())
        .unwrap();
    assert_eq!(model["kind"], "sam_prompt");
    assert!(runtime.sam_model().is_some());
    let (info, path) = labelflow_lib::services::preview::cached(&s, &records[0].id).unwrap();
    let base = json!({"imagePath": path, "width": info.width, "height": info.height,
        "sourceSha256": info.sha256});
    let mut encode = base.clone();
    encode["encodeOnly"] = json!(true);
    assert_eq!(
        runtime.sam(encode, runtime.token()).unwrap()["cached"],
        false
    );
    let mut click = base.clone();
    click["encodeOnly"] = json!(false);
    click["points"] = json!([{"x": 32, "y": 24, "positive": true}]);
    let value = runtime.sam(click, runtime.token()).unwrap();
    assert_eq!(value["cached"], true);
    let runs = value["runs"].as_array().unwrap();
    let total: u64 = runs
        .iter()
        .skip(1)
        .step_by(2)
        .map(|v| v.as_u64().unwrap())
        .sum();
    assert_eq!(total, u64::from(W * H));
    // Fixture: +-20 encoder px at scale 256/64 = +-5 image px around the click.
    assert!(value["area"].as_u64().unwrap() > 50);
    runtime.stop().unwrap();
    assert!(runtime.sam_model().is_none());
}
