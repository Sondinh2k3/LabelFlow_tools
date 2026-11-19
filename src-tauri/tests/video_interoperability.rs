use labelflow_lib::{
    domain::{annotation::*, geometry as g, ErrorCode},
    export::{self, instances as out},
    import::instances as input,
    security::resources::Resources,
    services::{annotation as a, backup, library, project, quality},
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn ring(x: f64, y: f64, w: f64, h: f64) -> Vec<Point> {
    vec![
        Point { x, y },
        Point { x: x + w, y },
        Point { x: x + w, y: y + h },
        Point { x, y: y + h },
    ]
}
fn polygon() -> Geometry {
    Geometry::Polygon {
        points: ring(1.0, 1.0, 8.0, 6.0),
        holes: vec![],
    }
}
fn hole() -> Geometry {
    Geometry::Polygon {
        points: ring(0.0, 0.0, 12.0, 10.0),
        holes: vec![ring(3.0, 3.0, 4.0, 3.0)],
    }
}
fn mask() -> Geometry {
    let p = g::rasterize(&hole(), 16, 12).unwrap();
    Geometry::Mask {
        width: 16,
        height: 12,
        runs: a::pack(String::new(), 16, 12, &p).runs,
        iscrowd: true,
    }
}
fn multi() -> Geometry {
    Geometry::MultiPolygon {
        polygons: vec![
            PolygonRings {
                points: ring(0.0, 0.0, 3.0, 3.0),
                holes: vec![],
            },
            PolygonRings {
                points: ring(8.0, 7.0, 4.0, 3.0),
                holes: vec![],
            },
        ],
    }
}
fn annotation(geometry: Geometry) -> Annotation {
    Annotation {
        id: id(),
        layer_id: "candidate_objects".into(),
        class_id: Some("candidate-2".into()),
        geometry,
        source: "imported".into(),
    }
}
fn setup() -> (tempfile::TempDir, project::ProjectSession, String, PathBuf) {
    let t = tempfile::tempdir().unwrap();
    let p = t.path().join("ảnh nguồn.png");
    fs::write(&p, a::encode_png(16, 12, &[127; 192]).unwrap()).unwrap();
    let root = t.path().join("project");
    fs::create_dir(&root).unwrap();
    let mut s = project::create_with_profile(&root, "M2 test", "road-encroachment-v1").unwrap();
    library::import_paths(
        &mut s,
        std::slice::from_ref(&p),
        false,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    let media = library::all(&s).unwrap()[0].id.clone();
    (t, s, media, p)
}
fn options(format: &str) -> ExportOptions {
    ExportOptions {
        format: format.into(),
        skip_unsupported: false,
        approved_only: false,
        seed: "m2-fixture".into(),
        train_percent: 100,
        val_percent: 0,
    }
}
#[test]
fn coco_rle_preserves_every_pixel_column_order_and_signed_deltas() {
    for (w, h) in [(1, 1), (2, 5), (16, 12), (192, 108)] {
        for seed in 0..12 {
            let pixels = (0..w * h)
                .map(|i| u8::from(((i * 17 + seed * 11) % 23) < 8))
                .collect::<Vec<_>>();
            let counts = g::coco_counts(&pixels, w, h);
            assert_eq!(g::decode_coco_counts(&counts, w, h).unwrap(), pixels);
            assert_eq!(
                g::decompress_coco(&g::compress_coco(&counts), u64::from(w * h)).unwrap(),
                counts
            );
        }
    }
    assert!(g::decode_coco_counts(&[1], 2, 2).is_err());
    assert!(g::decode_coco_counts(&[u32::MAX], 1, 1).is_err());
    for text in ["/", "p", "o", "PPPPPPPPPPPPPPPPPPPP"] {
        assert!(g::decompress_coco(text, 100).is_err());
    }
}
#[test]
fn yolo_seg_roundtrip_is_subpixel_and_rejects_lossy_geometries() {
    let poly = polygon();
    let line = out::yolo_seg_row(&poly, 1920, 1080, 2).unwrap();
    let parsed = input::parse_yolo_seg(&line, 1920, 1080).unwrap();
    let Geometry::Polygon { points, .. } = &parsed[0].1 else {
        panic!()
    };
    let Geometry::Polygon {
        points: expected, ..
    } = poly
    else {
        panic!()
    };
    for (a, b) in points.iter().zip(expected) {
        assert!((a.x - b.x).abs() < 0.000001);
        assert!((a.y - b.y).abs() < 0.000001);
    }
    for shape in [
        hole(),
        mask(),
        multi(),
        Geometry::Bbox {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        },
    ] {
        assert!(out::yolo_seg_row(&shape, 16, 12, 0).is_err());
    }
    for text in [
        "0 0.5 0.5 0.2 0.2",
        "0 0 0 1 1 0 1 1 0",
        "0 NaN 0 1 0 1 1",
        "-1 0 0 1 0 1 1",
        "0 0 0 1.1 0 1 1",
    ] {
        assert!(input::parse_yolo_seg(text, 16, 12).is_err(), "{text}");
    }
}
#[test]
fn disconnected_components_are_a_union_and_holes_stay_background() {
    let pixels = g::rasterize(&multi(), 16, 12).unwrap();
    assert_eq!(pixels.iter().filter(|p| **p == 1).count(), 21);
    assert_eq!(g::bounds(&multi()).unwrap(), [0.0, 0.0, 12.0, 10.0]);
    let h = g::rasterize(&hole(), 16, 12).unwrap();
    assert_eq!(h.iter().filter(|p| **p == 1).count(), 108);
    assert_eq!(h[4 * 16 + 4], 0);
    assert!(a::validate_geometry(&Geometry::MultiPolygon { polygons: vec![] }, 16, 12).is_err());
    assert!(a::validate_geometry(
        &Geometry::Mask {
            width: 16,
            height: 12,
            runs: vec![2, 192],
            iscrowd: false
        },
        16,
        12
    )
    .is_err());
}
#[test]
fn instance_masks_use_external_assets_and_survive_reopen_backup() {
    let (tmp, mut s, media, _) = setup();
    let mut doc = a::load(&s, &media).unwrap();
    doc.annotations = vec![annotation(mask()), annotation(multi()), annotation(hole())];
    let expected = doc.annotations.clone();
    a::save(&mut s, doc, &id()).unwrap();
    let stored: String = s
        .connection
        .query_row(
            "SELECT state_json FROM annotation_documents WHERE media_id=?1",
            [&media],
            |r| r.get(0),
        )
        .unwrap();
    assert!(stored.contains("\"runs\":[]"));
    let count: i64 = s
        .connection
        .query_row(
            "SELECT count(*) FROM annotations WHERE mask_asset_id IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
    let root = s.root.clone();
    drop(s);
    let s = project::open(&root).unwrap();
    assert_eq!(a::load(&s, &media).unwrap().annotations, expected);
    let saved = backup::backup(&s, tmp.path()).unwrap();
    let restored_root = tmp.path().join("restored");
    fs::create_dir(&restored_root).unwrap();
    backup::restore_backup(&saved, &restored_root).unwrap();
    let restored = project::open(&restored_root).unwrap();
    assert_eq!(a::load(&restored, &media).unwrap().annotations, expected);
}
#[test]
fn coco_polygon_compressed_rle_and_detection_roundtrip_with_explicit_mapping() {
    let (tmp, mut s, media, _) = setup();
    let mut doc = a::load(&s, &media).unwrap();
    doc.annotations = vec![
        annotation(polygon()),
        annotation(mask()),
        annotation(multi()),
        annotation(hole()),
        annotation(Geometry::Bbox {
            x: 1.25,
            y: 2.5,
            width: 3.5,
            height: 4.25,
        }),
    ];
    let expected = doc
        .annotations
        .iter()
        .map(|a| g::rasterize(&a.geometry, 16, 12).unwrap())
        .collect::<Vec<_>>();
    a::save(&mut s, doc, &id()).unwrap();
    let report = export::export(
        &s,
        tmp.path(),
        &options("coco"),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    let json = Path::new(&report.path).join("annotations/instances_train.json");
    let prepared = input::inspect_coco(&s, &json).unwrap();
    assert_eq!(prepared.inspection().objects, 5);
    for ((_, shape), pixels) in prepared.rows[0].1.iter().zip(expected) {
        assert_eq!(g::rasterize(shape, 16, 12).unwrap(), pixels);
    }
    let mapping = [input::CategoryMapping {
        source: 2,
        target: "candidate-2".into(),
    }];
    let prepared = input::inspect_coco(&s, &json).unwrap();
    assert!(input::apply(&mut s, prepared, &[], &AtomicBool::new(false), |_| {}).is_err());
    let prepared = input::inspect_coco(&s, &json).unwrap();
    assert_eq!(
        input::apply(&mut s, prepared, &mapping, &AtomicBool::new(false), |_| {})
            .unwrap()
            .imported,
        1
    );
    let prepared = input::inspect_coco(&s, &json).unwrap();
    assert_eq!(
        input::apply(&mut s, prepared, &mapping, &AtomicBool::new(false), |_| {})
            .unwrap()
            .duplicates,
        1
    );
    let mut doc = a::load(&s, &media).unwrap();
    assert_eq!(doc.annotations.len(), 10);
    doc.review_status = "approved".into();
    a::save(&mut s, doc, &id()).unwrap();
    let prepared = input::inspect_coco(&s, &json).unwrap();
    let report = input::apply(&mut s, prepared, &mapping, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(report.imported, 0);
    assert!(report.warnings[0].contains("Approved"));
}
#[test]
fn uncompressed_coco_and_invalid_references_are_checked() {
    let (tmp, s, _media, path) = setup();
    let pixels = g::rasterize(&hole(), 16, 12).unwrap();
    let mut value = serde_json::json!({"images":[{"id":1,"file_name":path.file_name().unwrap().to_str().unwrap(),"width":16,"height":12}],"categories":[{"id":99,"name":"obstacle"}],"annotations":[{"id":1,"image_id":1,"category_id":99,"segmentation":{"size":[12,16],"counts":g::coco_counts(&pixels,16,12)},"iscrowd":1}]});
    let file = tmp.path().join("uncompressed.json");
    fs::write(&file, a::json(&value).unwrap()).unwrap();
    let prepared = input::inspect_coco(&s, &file).unwrap();
    assert_eq!(
        g::rasterize(&prepared.rows[0].1[0].1, 16, 12).unwrap(),
        pixels
    );
    for (key, bad) in [
        ("image_id", serde_json::json!(9)),
        ("category_id", serde_json::json!(5)),
        ("iscrowd", serde_json::json!(2)),
    ] {
        let old = value["annotations"][0][key].clone();
        value["annotations"][0][key] = bad;
        fs::write(&file, a::json(&value).unwrap()).unwrap();
        assert!(input::inspect_coco(&s, &file).is_err());
        value["annotations"][0][key] = old;
    }
}
#[test]
fn strict_export_refuses_holes_and_override_lists_skips_without_changing_labels() {
    let (tmp, mut s, media, _) = setup();
    let mut doc = a::load(&s, &media).unwrap();
    doc.annotations = vec![annotation(hole()), annotation(polygon())];
    a::save(&mut s, doc, &id()).unwrap();
    let before = a::load(&s, &media).unwrap();
    assert!(export::export(
        &s,
        tmp.path(),
        &options("yoloSeg"),
        &AtomicBool::new(false),
        |_| {}
    )
    .is_err());
    let mut opts = options("yoloSeg");
    opts.skip_unsupported = true;
    let report = export::export(&s, tmp.path(), &opts, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(report.annotations, 1);
    assert!(report.warnings.iter().any(|w| w.contains("holes")));
    assert_eq!(a::load(&s, &media).unwrap(), before);
}
#[test]
fn missing_source_repair_requires_exact_content_and_preserves_annotations() {
    let (tmp, mut s, media, path) = setup();
    let mut doc = a::load(&s, &media).unwrap();
    doc.annotations.push(annotation(polygon()));
    doc.event_label = Some("uncertain".into());
    a::save(&mut s, doc, &id()).unwrap();
    let expected = a::load(&s, &media).unwrap();
    let moved = tmp.path().join("renamed.png");
    fs::rename(path, &moved).unwrap();
    assert!(
        !quality::inspect(&s, &options("coco"), false, &AtomicBool::new(false), |_| {})
            .unwrap()
            .errors
            .is_empty()
    );
    let wrong = tmp.path().join("wrong.png");
    fs::write(&wrong, a::encode_png(16, 12, &[0; 192]).unwrap()).unwrap();
    assert!(quality::repair(&mut s, &media, &wrong).is_err());
    quality::repair(&mut s, &media, &moved).unwrap();
    assert_eq!(a::load(&s, &media).unwrap(), expected);
    assert!(
        quality::inspect(&s, &options("coco"), true, &AtomicBool::new(false), |_| {})
            .unwrap()
            .errors
            .is_empty()
    );
    let filter = library::LibraryFilter {
        event: "uncertain".into(),
        group: String::new(),
        annotated: Some(true),
    };
    assert_eq!(
        library::filtered_page(&s, 0, 40, "", "", &filter)
            .unwrap()
            .total,
        1
    );
    let filter = library::LibraryFilter {
        annotated: Some(false),
        ..filter
    };
    assert_eq!(
        library::filtered_page(&s, 0, 40, "", "", &filter)
            .unwrap()
            .total,
        0
    );
}
#[test]
fn resource_urls_require_opaque_tokens_support_ranges_and_revoke() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("source.mp4");
    fs::write(&path, b"0123456789").unwrap();
    let r = Resources::default();
    let url = r.register(path, "video/mp4");
    let req = |url: &str, range: Option<&str>| {
        let mut b = tauri::http::Request::builder().uri(url);
        if let Some(v) = range {
            b = b.header("Range", v);
        }
        b.body(vec![]).unwrap()
    };
    let response = r.response(req(&url, Some("bytes=3-6")));
    assert_eq!(response.status(), 206);
    assert_eq!(response.body(), b"3456");
    assert_eq!(response.headers()["Content-Range"], "bytes 3-6/10");
    assert_eq!(r.response(req(&url, Some("bytes=11-"))).status(), 416);
    assert_eq!(
        r.response(req("http://lfmedia.localhost/../../Windows/test", None))
            .status(),
        404
    );
    r.clear();
    assert_eq!(r.response(req(&url, None)).status(), 404);
}
#[test]
fn cancelled_import_preserves_document_and_video_tool_failure_is_typed() {
    let (tmp, mut s, media, _) = setup();
    let path = tmp.path().join("labels");
    fs::create_dir(&path).unwrap();
    fs::write(path.join(format!("{media}.txt")), "0 0 0 1 0 1 1").unwrap();
    let prepared = input::inspect_yolo_seg(&s, &path).unwrap();
    let report = input::apply(
        &mut s,
        prepared,
        &[input::CategoryMapping {
            source: 0,
            target: "candidate-0".into(),
        }],
        &AtomicBool::new(true),
        |_| {},
    )
    .unwrap();
    assert!(report.cancelled);
    assert!(a::load(&s, &media).unwrap().annotations.is_empty());
    let error = labelflow_lib::video::process::run(
        Path::new("not-present.exe"),
        &[],
        &AtomicBool::new(true),
        10,
        std::time::Duration::from_secs(1),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::Cancelled);
}

// Optional acceptance jobs write reproducible artifacts only under the workspace.
#[test]
#[ignore = "Requires generated FFmpeg video fixtures; run via npm run test:m2:acceptance"]
fn video_acceptance() {
    use labelflow_lib::domain::video::ExtractionOptions;
    use labelflow_lib::video::{self, tools::Toolchain};
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let directory = workspace.join(".cache/m2-acceptance");
    fs::create_dir_all(&directory).unwrap();
    let root = directory.join(id());
    fs::create_dir(&root).unwrap();
    let mut s =
        project::create_with_profile(&root, "M2 video acceptance", "road-encroachment-v1").unwrap();
    let tools = Toolchain::discover().unwrap();
    let active_cancel = AtomicBool::new(false);
    let started = std::time::Instant::now();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(100));
            active_cancel.store(true, Ordering::Relaxed);
        });
        let args = [
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=16x16:rate=1",
            "-f",
            "null",
            "-",
        ]
        .map(str::to_owned);
        let error = video::process::run(
            &tools.ffmpeg,
            &args,
            &active_cancel,
            1024,
            std::time::Duration::from_secs(10),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::Cancelled);
    });
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    let cancel = AtomicBool::new(false);
    let mut counts = vec![];
    for name in ["cfr.mp4", "vfr.mp4", "rotated.mp4"] {
        let source = workspace.join("tests/fixtures/video").join(name);
        let original = video::source_sha(&source).unwrap();
        let imported = video::import(&mut s, &tools, &source, &cancel).unwrap();
        assert_eq!(
            video::import(&mut s, &tools, &source, &cancel).unwrap().id,
            imported.id
        );
        if name == "rotated.mp4" {
            assert_eq!(imported.rotation.rem_euclid(180), 90);
        }
        let frames = video::frames(&s, &imported.id, 0, 1000).unwrap().frames;
        for pair in frames.windows(2) {
            assert!((pair[0].duration_ms - pair[1].time_ms + pair[0].time_ms).abs() < 0.00001);
        }
        let jpeg = video::decode(&tools, &imported, &frames[0], "jpeg", 95, &cancel).unwrap();
        assert_eq!(
            image::guess_format(&jpeg).unwrap(),
            image::ImageFormat::Jpeg
        );
        let jpg = image::load_from_memory(&jpeg).unwrap();
        let expected_dimensions = if imported.rotation.rem_euclid(180) == 90 {
            (imported.height, imported.width)
        } else {
            (imported.width, imported.height)
        };
        assert_eq!((jpg.width(), jpg.height()), expected_dimensions);
        if name == "vfr.mp4" {
            assert!(frames
                .windows(3)
                .any(|f| (f[1].time_ms - f[0].time_ms - f[2].time_ms + f[1].time_ms).abs() > 1.0));
        }
        for frame in &frames {
            assert_eq!(
                video::seek(&s, &imported.id, frame.time_ms).unwrap().pts,
                frame.pts
            );
        }
        let opts = ExtractionOptions {
            video_id: imported.id.clone(),
            indices: vec![0, frames.len() as u32 / 2, frames.len() as u32 - 1],
            start_ms: 0.0,
            end_ms: 0.0,
            interval_ms: None,
            format: "png".into(),
            quality: 95,
        };
        let report = video::extract(&mut s, &tools, &opts, &cancel, |_| {}).unwrap();
        assert_eq!(report.imported, 3);
        assert_eq!(
            video::extract(&mut s, &tools, &opts, &cancel, |_| {})
                .unwrap()
                .duplicates,
            3
        );
        let cancel_partial = AtomicBool::new(false);
        let partial = ExtractionOptions {
            indices: vec![1, 2],
            ..opts.clone()
        };
        let partial_report = video::extract(&mut s, &tools, &partial, &cancel_partial, |_| {
            cancel_partial.store(true, Ordering::Relaxed);
        })
        .unwrap();
        assert!(partial_report.cancelled);
        assert_eq!(partial_report.imported, 1);
        let interval = ExtractionOptions {
            indices: vec![],
            start_ms: 0.0,
            end_ms: frames.last().unwrap().time_ms,
            interval_ms: Some(100.0),
            ..opts
        };
        let expected = video::selected_indices(&s, &interval).unwrap();
        assert!(!expected.is_empty());
        video::extract(&mut s, &tools, &interval, &cancel, |_| {}).unwrap();
        assert_eq!(video::source_sha(&source).unwrap(), original);
        counts.push(serde_json::json!({"video":imported,"frames":frames}));
    }
    let records = library::all(&s).unwrap();
    for record in &records {
        let mut doc = a::load(&s, &record.id).unwrap();
        doc.annotations.extend([
            annotation(polygon()),
            annotation(Geometry::Mask {
                width: record.width,
                height: record.height,
                runs: a::pack(
                    String::new(),
                    record.width,
                    record.height,
                    &g::rasterize(&hole(), record.width, record.height).unwrap(),
                )
                .runs,
                iscrowd: true,
            }),
            annotation(multi()),
            annotation(hole()),
        ]);
        doc.split_group = format!("illegal-per-frame-{}", record.id);
        a::save(&mut s, doc, &id()).unwrap();
        assert!(!a::load(&s, &record.id)
            .unwrap()
            .split_group
            .starts_with("illegal"));
    }
    let mut opts = options("coco");
    opts.train_percent = 60;
    opts.val_percent = 20;
    let report = export::export(&s, &directory, &opts, &cancel, |_| {}).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(Path::new(&report.path).join("manifest.json")).unwrap())
            .unwrap();
    let mut seen = std::collections::HashMap::new();
    for image in manifest["images"].as_array().unwrap() {
        let source = image["sourceVideo"]["videoId"].as_str().unwrap();
        let split = image["split"].as_str().unwrap();
        if let Some(old) = seen.insert(source, split) {
            assert_eq!(old, split);
        }
    }
    let mut seg_opts = options("yoloSeg");
    seg_opts.skip_unsupported = true;
    let seg = export::export(&s, &directory, &seg_opts, &cancel, |_| {}).unwrap();
    fs::write(directory.join("latest.json"),a::json(&serde_json::json!({"project":root,"export":report.path,"yoloSegExport":seg.path,"videos":counts,"images":records.len(),"sourceHashesPreserved":true,"groupLeakage":false})).unwrap()).unwrap();
    // Cancellation keeps existing committed frames and documents.
    cancel.store(true, Ordering::Relaxed);
    let before = library::all(&s).unwrap().len();
    let first = video::list(&s).unwrap()[0].clone();
    let opts = ExtractionOptions {
        video_id: first.id,
        indices: vec![0],
        start_ms: 0.0,
        end_ms: 0.0,
        interval_ms: None,
        format: "png".into(),
        quality: 95,
    };
    assert_eq!(
        video::extract(&mut s, &tools, &opts, &cancel, |_| {})
            .unwrap_err()
            .code,
        ErrorCode::Cancelled
    );
    assert_eq!(library::all(&s).unwrap().len(), before);
}

#[test]
#[ignore = "Measures 10000 actual synthetic PNG file imports in the workspace"]
fn large_incremental_import_acceptance() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let directory = workspace.join(".cache/m2-large");
    fs::create_dir_all(&directory).unwrap();
    let source = directory.join("sources");
    fs::create_dir_all(&source).unwrap();
    for i in 0..10000u32 {
        let path = source.join(format!("image-{i:05}.png"));
        if !path.exists() {
            let mut pixels = [127; 192];
            pixels[0] = (i % 256) as u8;
            pixels[1] = (i / 256) as u8;
            fs::write(path, a::encode_png(16, 12, &pixels).unwrap()).unwrap();
        }
    }
    let root = directory.join(id());
    fs::create_dir(&root).unwrap();
    let mut s =
        project::create_with_profile(&root, "10k incremental", "road-encroachment-v1").unwrap();
    let cancel = AtomicBool::new(false);
    let start = std::time::Instant::now();
    let mut events = 0;
    let first = library::import_paths(&mut s, std::slice::from_ref(&source), false, &cancel, |p| {
        events += 1;
        if p.completed == 100 {
            cancel.store(true, Ordering::Relaxed);
        }
    })
    .unwrap();
    assert!(first.cancelled);
    assert_eq!(first.imported, 100);
    let media = library::all(&s).unwrap()[0].id.clone();
    let mut doc = a::load(&s, &media).unwrap();
    doc.annotations.push(annotation(polygon()));
    a::save(&mut s, doc, &id()).unwrap();
    let expected = a::load(&s, &media).unwrap();
    cancel.store(false, Ordering::Relaxed);
    let report = library::import_paths(
        &mut s,
        std::slice::from_ref(&source),
        false,
        &cancel,
        |_| {
            events += 1;
        },
    )
    .unwrap();
    assert_eq!(report.imported, 9900);
    assert_eq!(report.duplicates, 100);
    assert_eq!(library::page(&s, 0, 40, "", "").unwrap().total, 10000);
    assert_eq!(a::load(&s, &media).unwrap(), expected);
    let repeat = library::import_paths(
        &mut s,
        std::slice::from_ref(&source),
        false,
        &cancel,
        |_| {
            events += 1;
        },
    )
    .unwrap();
    assert_eq!(repeat.duplicates, 10000);
    assert_eq!(repeat.imported, 0);
    fs::write(directory.join("latest.json"),a::json(&serde_json::json!({"project":root,"files":10000,"progressEvents":events,"elapsedSeconds":start.elapsed().as_secs_f64(),"cancelledAfter":100,"resumedImported":report.imported,"repeatDuplicates":repeat.duplicates,"annotationsPreserved":true})).unwrap()).unwrap();
}
#[test]
fn schema_two_upgrade_preserves_vector_and_external_mask_references() {
    let tmp = tempfile::tempdir().unwrap();
    let metadata = tmp.path().join(".labelflow");
    fs::create_dir_all(metadata.join("masks")).unwrap();
    let project_id = id();
    let media = id();
    let object = annotation(polygon());
    let mut db = rusqlite::Connection::open(metadata.join("project.sqlite")).unwrap();
    db.execute_batch(include_str!("../migrations/0001_foundation.sql"))
        .unwrap();
    db.execute_batch(include_str!("../migrations/0002_manual_labeling.sql"))
        .unwrap();
    db.execute("INSERT INTO projects(id,name,profile_id,schema_version) VALUES(?1,'Legacy M1','road-encroachment-v1',2)",[&project_id]).unwrap();
    labelflow_lib::services::profile::ensure(&mut db, &project_id, "road-encroachment-v1").unwrap();
    let pixels = [255; 192];
    let bytes = a::encode_png(16, 12, &pixels).unwrap();
    let hash = a::sha(&bytes);
    fs::write(metadata.join(format!("masks/{hash}.png")), bytes).unwrap();
    db.execute("INSERT INTO media(id,project_id,type,path,import_mode,content_sha256,width,height,orientation,review_status,split_group_id) VALUES(?1,?2,'image','missing-original.png','linked',?3,16,12,1,'approved','trip-one')",rusqlite::params![media,project_id,"0".repeat(64)]).unwrap();
    db.execute("INSERT INTO mask_assets(id,relative_path,sha256,width,height,encoding,source_sha256) VALUES(?1,?2,?1,16,12,'png8',?1)",rusqlite::params![hash,format!("masks/{hash}.png")]).unwrap();
    db.execute("INSERT INTO annotations(id,media_id,layer_id,class_id,geometry_type,geometry_json,source,review_status,revision) VALUES(?1,?2,'candidate_objects','candidate-2','polygon',?3,'imported','approved',1)",rusqlite::params![object.id,media,a::json(&object.geometry).unwrap()]).unwrap();
    db.execute("INSERT INTO annotations(id,media_id,layer_id,geometry_type,geometry_json,mask_asset_id,source,review_status,revision) VALUES(?1,?2,'cityscapes_semantic','semantic_layer','{}',?3,'manual','approved',1)",rusqlite::params![id(),media,hash]).unwrap();
    let document = Document {
        media_id: media.clone(),
        revision: 1,
        review_status: "approved".into(),
        event_label: None,
        notes: "legacy".into(),
        split_group: "trip-one".into(),
        annotations: vec![object.clone()],
        masks: vec![],
    };
    let stored = serde_json::json!({"document":document,"assets":[{"layerId":"cityscapes_semantic","assetId":hash}]});
    db.execute(
        "INSERT INTO annotation_documents(media_id,revision,state_json) VALUES(?1,1,?2)",
        rusqlite::params![media, a::json(&stored).unwrap()],
    )
    .unwrap();
    db.execute("UPDATE workspace SET active_media_id=?1", [&media])
        .unwrap();
    db.pragma_update(None, "user_version", 2).unwrap();
    drop(db);
    fs::write(metadata.join("project.json"),a::json(&serde_json::json!({"formatVersion":1,"id":project_id,"name":"Legacy M1","profileId":"road-encroachment-v1"})).unwrap()).unwrap();
    let s = project::open(tmp.path()).unwrap();
    assert_eq!(
        s.project.schema_version,
        labelflow_lib::domain::SCHEMA_VERSION
    );
    let restored = a::load(&s, &media).unwrap();
    assert_eq!(restored.annotations, vec![object]);
    assert_eq!(a::unpack(&restored.masks[0]).unwrap(), pixels);
    let foreign_target:String=s.connection.query_row("SELECT \"table\" FROM pragma_foreign_key_list('media') WHERE \"from\"='source_video_id'",[],|r|r.get(0)).unwrap();
    assert_eq!(foreign_target, "videos");
    assert_eq!(
        s.connection
            .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
                .get::<_, u32>(
                0
            ))
            .unwrap(),
        0
    );
    let backup = fs::read_dir(metadata.join("backups"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let old = rusqlite::Connection::open(backup).unwrap();
    assert_eq!(
        old.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        old.query_row("SELECT count(*) FROM annotations", [], |r| r
            .get::<_, u32>(0))
            .unwrap(),
        2
    );
}
#[test]
fn corrupted_preview_cache_is_rebuilt_without_touching_source_or_labels() {
    let (tmp, mut s, _, _) = setup();
    let path = tmp.path().join("converted-preview.jpg");
    image::RgbImage::from_pixel(16, 12, image::Rgb([24, 40, 70]))
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
    let media = library::all(&s)
        .unwrap()
        .into_iter()
        .find(|r| r.name == "converted-preview.jpg")
        .unwrap()
        .id;
    let original = fs::read(&path).unwrap();
    let (_, cached) = labelflow_lib::services::preview::cached(&s, &media).unwrap();
    let mut corrupt = fs::read(&cached).unwrap();
    let n = corrupt.len();
    corrupt[n / 2] ^= 1;
    fs::write(&cached, corrupt).unwrap();
    let (_, repaired) = labelflow_lib::services::preview::cached(&s, &media).unwrap();
    assert!(labelflow_lib::services::preview::valid(&repaired, (16, 12)));
    fs::write(repaired.with_extension("sha256"), [b'a'; 4096]).unwrap();
    let (_, repaired) = labelflow_lib::services::preview::cached(&s, &media).unwrap();
    assert_eq!(
        fs::metadata(repaired.with_extension("sha256"))
            .unwrap()
            .len(),
        64
    );
    assert_eq!(fs::read(&path).unwrap(), original);
}
