use labelflow_lib::{
    domain::annotation::*,
    services::{annotation as a, library, preview, project, storage},
};
use std::{fs, path::Path, sync::atomic::AtomicBool};
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn setup(
    extension: &str,
) -> (
    tempfile::TempDir,
    project::ProjectSession,
    String,
    std::path::PathBuf,
) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join(format!("source.{extension}"));
    image::RgbImage::from_pixel(128, 96, image::Rgb([50, 70, 90]))
        .save(&path)
        .unwrap();
    let mut session =
        project::create_with_profile(temp.path(), "Efficiency", "road-encroachment-v1").unwrap();
    library::import_paths(
        &mut session,
        std::slice::from_ref(&path),
        false,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    let media = library::all(&session).unwrap()[0].id.clone();
    (temp, session, media, path)
}
#[test]
fn canonical_png_is_served_without_a_duplicate_and_changed_sources_still_fail() {
    let (_temp, s, media, path) = setup("png");
    let before = fs::read(&path).unwrap();
    let (info, selected) = preview::cached(&s, &media).unwrap();
    assert_eq!(selected, path.canonicalize().unwrap());
    assert_eq!(info.sha256, a::sha(&before));
    assert!(!s.root.join(".labelflow/thumbnails/previews").exists());
    let thumbnail = library::thumbnail(&s, &media).unwrap();
    assert!(thumbnail.starts_with("data:image/jpeg;base64,"));
    assert_eq!(fs::read(&path).unwrap(), before);
    fs::write(path, b"changed linked content").unwrap();
    assert!(preview::cached(&s, &media).is_err());
}
#[test]
fn animated_png_uses_a_static_canonical_preview() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("animation.png");
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_animated(2, 0).unwrap();
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[20; 12]).unwrap();
        writer.write_image_data(&[220; 12]).unwrap();
    }
    fs::write(&path, bytes).unwrap();
    let mut s = project::create(temp.path(), "APNG").unwrap();
    library::import_paths(
        &mut s,
        std::slice::from_ref(&path),
        false,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    let media = library::all(&s).unwrap()[0].id.clone();
    let (_, selected) = preview::cached(&s, &media).unwrap();
    assert_ne!(selected, path.canonicalize().unwrap());
    assert_eq!(image::open(selected).unwrap().to_rgb8().as_raw(), &[20; 12]);
}
#[test]
fn cache_cleanup_preserves_sources_label_assets_history_and_unknown_files() {
    let (_temp, mut s, media, path) = setup("jpg");
    let original = fs::read(&path).unwrap();
    let mut doc = a::load(&s, &media).unwrap();
    doc.masks = vec![a::pack(
        "cityscapes_semantic".into(),
        128,
        96,
        &vec![3; 128 * 96],
    )];
    a::save(&mut s, doc, &id()).unwrap();
    let doc = a::load(&s, &media).unwrap();
    let expected = a::json(&doc).unwrap();
    let masks = fs::read_dir(s.root.join(".labelflow/masks"))
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            (p.clone(), fs::read(p).unwrap())
        })
        .collect::<Vec<_>>();
    fs::write(s.root.join(".labelflow/backups/keep.json"), b"backup").unwrap();
    fs::write(
        s.root.join(".labelflow/thumbnails/unknown.txt"),
        b"preserve",
    )
    .unwrap();
    fs::write(
        s.root.join(format!(".labelflow/thumbnails/{}.png", id())),
        b"legacy thumbnail",
    )
    .unwrap();
    let (_, cached) = preview::cached(&s, &media).unwrap();
    assert!(cached.is_file());
    assert!(storage::clear(&s, false).is_err());
    assert!(cached.is_file());
    let before = serde_json::to_value(storage::usage(&s).unwrap()).unwrap();
    assert!(before["removableBytes"].as_u64().unwrap() > 0);
    let result = serde_json::to_value(storage::clear(&s, true).unwrap()).unwrap();
    assert_eq!(result["removableBytes"], 0);
    assert!(result["removedFiles"].as_u64().unwrap() >= 4);
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(a::json(&a::load(&s, &media).unwrap()).unwrap(), expected);
    assert_eq!(
        s.connection
            .query_row("SELECT count(*) FROM annotation_operations", [], |r| r
                .get::<_, u32>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        fs::read(s.root.join(".labelflow/backups/keep.json")).unwrap(),
        b"backup"
    );
    assert_eq!(
        fs::read(s.root.join(".labelflow/thumbnails/unknown.txt")).unwrap(),
        b"preserve"
    );
    for (p, bytes) in masks {
        assert_eq!(fs::read(p).unwrap(), bytes);
    }
    assert!(library::thumbnail(&s, &media).is_ok());
    assert!(preview::cached(&s, &media).is_ok());
}
#[test]
fn cache_trimming_keeps_the_current_preview_and_caps_recognized_disposable_files() {
    let (_temp, s, _media, _path) = setup("png");
    let kind = storage::CacheKind::Thumbnails;
    let dir = kind.directory(&s).unwrap();
    let mut keep = std::path::PathBuf::new();
    for _ in 0..3 {
        keep = dir.join(format!("{}.jpg", id()));
        let file = fs::File::create(&keep).unwrap();
        file.set_len(12 * 1024 * 1024).unwrap();
    }
    storage::trim_cache(&s, kind, &keep, kind.budget()).unwrap();
    assert!(keep.exists());
    let usage = serde_json::to_value(storage::usage(&s).unwrap()).unwrap();
    let thumbnails = usage["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "thumbnails")
        .unwrap();
    assert!(thumbnails["bytes"].as_u64().unwrap() <= kind.budget());
}
#[test]
#[ignore = "Reads 20 authorized dataset photos and writes workspace-local thumbnail size evidence"]
fn benchmark_real_thumbnail_storage() {
    let dataset = std::path::PathBuf::from(
        std::env::var_os("LABELFLOW_RESOURCE_DATASET").expect("Set authorized dataset folder"),
    );
    let mut paths = fs::read_dir(dataset)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "png"))
        .collect::<Vec<_>>();
    paths.sort();
    paths.truncate(20);
    assert_eq!(paths.len(), 20);
    let output = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.cache/resource-benchmark");
    fs::create_dir_all(&output).unwrap();
    let (mut old_bytes, mut new_bytes, mut absolute_error, mut channels) = (0u64, 0u64, 0u64, 0u64);
    let mut samples = Vec::new();
    for (index, path) in paths.iter().enumerate() {
        let (info, decoded) = labelflow_lib::services::media::load(path).unwrap();
        let thumbnail = decoded.thumbnail(200, 140).to_rgb8();
        let mut old = std::io::Cursor::new(Vec::new());
        thumbnail
            .write_to(&mut old, image::ImageFormat::Png)
            .unwrap();
        let old = old.into_inner();
        let mut new = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut new, 78)
            .encode_image(&thumbnail)
            .unwrap();
        let jpeg = image::load_from_memory(&new).unwrap().to_rgb8();
        for (a, b) in thumbnail.as_raw().iter().zip(jpeg.as_raw()) {
            absolute_error += u64::from(a.abs_diff(*b));
            channels += 1;
        }
        old_bytes += old.len() as u64;
        new_bytes += new.len() as u64;
        assert_eq!(labelflow_lib::video::source_sha(path).unwrap(), info.sha256);
        samples.push(serde_json::json!({"name":info.name,"sha256":info.sha256,"pngBytes":old.len(),"jpegBytes":new.len()}));
        if index == 0 {
            fs::write(output.join("thumbnail-before.png"), old).unwrap();
            fs::write(output.join("thumbnail-after.jpg"), new).unwrap();
        }
    }
    assert!(new_bytes < old_bytes);
    let report = serde_json::json!({"images":paths.len(),"beforePngBytes":old_bytes,"afterJpegBytes":new_bytes,"meanAbsoluteChannelError":absolute_error as f64/channels as f64,"sourceUnchanged":true,"samples":samples,"scope":"First 20 sorted real PNG photos from the authorized dataset folder; 200x140 bounding box; JPEG quality 78. Loss applies only to library thumbnails, never source images, full previews or label assets. Size comparison, not a timing benchmark."});
    fs::write(
        output.join("thumbnails.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("{report}");
}
#[test]
#[ignore = "Creates workspace-local 1080p efficiency benchmark fixtures"]
fn prepare_resource_benchmark() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../.cache/resource-benchmark")
        .join(id());
    fs::create_dir_all(&root).unwrap();
    let (w, h) = (1920, 1080);
    let mut ids = Vec::new();
    let mut s =
        project::create_with_profile(&root, "Resource benchmark", "road-encroachment-v1").unwrap();
    for i in 0..4 {
        let image = image::RgbImage::from_fn(w, h, |x, y| {
            let r = ((x / 12 + y / 12 + i * 7) % 256) as u8;
            image::Rgb([
                r,
                ((x / 8 + i * 20) % 256) as u8,
                ((y / 8 + i * 30) % 256) as u8,
            ])
        });
        let path = root.join(format!("frame-{i}.png"));
        image.save(&path).unwrap();
        library::import_paths(&mut s, &[path], false, &AtomicBool::new(false), |_| {}).unwrap();
    }
    for r in library::all(&s).unwrap() {
        let mut doc = a::load(&s, &r.id).unwrap();
        doc.review_status = "in_progress".into();
        doc.event_label = Some("uncertain".into());
        doc.annotations = (0..20)
            .map(|i| Annotation {
                id: id(),
                layer_id: "candidate_objects".into(),
                class_id: Some("candidate-0".into()),
                geometry: Geometry::Bbox {
                    x: 50. + f64::from(i % 5) * 300.,
                    y: 80. + f64::from(i / 5) * 200.,
                    width: 120.,
                    height: 100.,
                },
                source: "manual".into(),
            })
            .collect();
        let semantic = (0..w * h)
            .map(|p| ((p % w / 24 + p / w / 12) % 19) as u8)
            .collect::<Vec<_>>();
        let road = (0..w * h)
            .map(|p| u8::from(p / w > 300 && p % w > 300 && p % w < 1500))
            .collect::<Vec<_>>();
        doc.masks = vec![
            a::pack("cityscapes_semantic".into(), w, h, &semantic),
            a::pack("road_extent".into(), w, h, &road),
        ];
        a::save(&mut s, doc, &id()).unwrap();
        ids.push(r.id);
    }
    ids.sort_by_key(|id| library::record(&s, id).unwrap().name);
    let parent = root.parent().unwrap();
    fs::write(
        parent.join("latest.json"),
        a::json(&serde_json::json!({"project":s.root,"ids":ids,"width":w,"height":h})).unwrap(),
    )
    .unwrap();
}
