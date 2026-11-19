use crate::{
    domain::{annotation::*, AppError, ErrorCode, Result},
    services::{annotation as a, library, profile, project::ProjectSession},
};
pub mod instances;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs,
    io::Cursor,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
pub fn yolo_row(g: &Geometry, width: u32, height: u32, class: u8) -> Result<String> {
    a::validate_geometry(g, width, height)?;
    let Geometry::Bbox {
        x,
        y,
        width: w,
        height: h,
    } = g
    else {
        return Err(a::invalid("YOLO detection supports bounding boxes only."));
    };
    Ok(format!(
        "{class} {:.10} {:.10} {:.10} {:.10}",
        (x + w / 2.0) / f64::from(width),
        (y + h / 2.0) / f64::from(height),
        w / f64::from(width),
        h / f64::from(height)
    ))
}
pub fn split_for(group: &str, seed: &str, train: u8, val: u8) -> Result<&'static str> {
    if u16::from(train) + u16::from(val) > 100 {
        return Err(a::invalid("Split percentages exceed 100."));
    }
    let hash = a::sha(format!("{}:{seed}:{group}", seed.len()).as_bytes());
    let n = u32::from_str_radix(&hash[..8], 16).unwrap() % 100;
    Ok(if n < u32::from(train) {
        "train"
    } else if n < u32::from(train) + u32::from(val) {
        "val"
    } else {
        "test"
    })
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportImage {
    id: String,
    source: String,
    source_sha256: String,
    width: u32,
    height: u32,
    split: String,
    group: String,
    revision: u32,
    review_status: String,
    event_label: Option<String>,
    notes: String,
    source_video: Option<serde_json::Value>,
}
pub fn export(
    session: &ProjectSession,
    parent: &Path,
    options: &ExportOptions,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<ExportReport> {
    let job = crate::jobs::start(
        &session.connection,
        "export_dataset",
        &a::json(&(parent, options))?,
    )?;
    let result = export_inner(session, parent, options, cancel, |p| {
        crate::jobs::progress(&session.connection, &job, p.completed, p.total);
        progress(p);
    });
    match &result {
        Ok(_) => crate::jobs::finish(&session.connection, &job, "completed", None),
        Err(error) => crate::jobs::finish(
            &session.connection,
            &job,
            if error.code == ErrorCode::Cancelled {
                "cancelled"
            } else {
                "failed"
            },
            Some(&error.to_string()),
        ),
    }
    result
}
fn export_inner(
    session: &ProjectSession,
    parent: &Path,
    options: &ExportOptions,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<ExportReport> {
    if !["yolo", "semantic", "all", "yoloSeg", "coco"].contains(&options.format.as_str())
        || options.seed.len() > 200
    {
        return Err(a::invalid("Invalid export options."));
    }
    split_for(
        "",
        &options.seed,
        options.train_percent,
        options.val_percent,
    )?;
    let profile = profile::current(session)?;
    let records = library::all(session)?
        .into_iter()
        .filter(|r| !options.approved_only || r.review_status == "approved")
        .collect::<Vec<_>>();
    if records.is_empty() {
        return Err(a::invalid("No images match this export."));
    }
    let parent = parent.canonicalize()?;
    let name = format!("LabelFlow-export-{}", uuid::Uuid::new_v4());
    let staging = parent.join(format!(".{name}.tmp"));
    fs::create_dir(&staging)?;
    let outcome = (|| -> Result<ExportReport> {
        let mut hashes = BTreeMap::new();
        let mut images = Vec::new();
        let mut warnings = Vec::new();
        let mut object_count = 0;
        let mut coco_images: BTreeMap<String, Vec<serde_json::Value>> = BTreeMap::new();
        let mut coco_annotations: BTreeMap<String, Vec<serde_json::Value>> = BTreeMap::new();
        let mut coco_id = 0u32;
        // Dataset QC: per-class pixel/image counts and images that are mostly Ignore.
        let mut valid_semantic = [false; 256];
        for c in profile
            .classes
            .iter()
            .filter(|c| c.namespace == crate::services::profile::SEMANTIC_NAMESPACE)
        {
            valid_semantic[usize::from(c.export_id)] = true;
        }
        let mut class_pixels = [0u64; 256];
        let mut class_images = [0u32; 256];
        let mut high_ignore: Vec<String> = Vec::new();
        let mut without_semantic = 0u32;
        let write =
            |relative: &str, bytes: &[u8], hashes: &mut BTreeMap<String, String>| -> Result<()> {
                let target = staging.join(relative);
                fs::create_dir_all(target.parent().unwrap())?;
                a::atomic_file(&target, bytes)?;
                hashes.insert(relative.into(), a::sha(bytes));
                Ok(())
            };
        for (index, record) in records.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return Err(AppError::new(
                    ErrorCode::Cancelled,
                    "Export cancelled; prior exports were preserved.",
                ));
            }
            let (_, source) = library::checked_load(session, record)?;
            let doc = a::load(session, &record.id)?;
            let group = crate::services::quality::group(session, &record.id)?;
            let split = split_for(
                &group,
                &options.seed,
                options.train_percent,
                options.val_percent,
            )?;
            let mut bytes = Cursor::new(Vec::new());
            source
                .write_to(&mut bytes, image::ImageFormat::Png)
                .map_err(|e| a::invalid(e.to_string()))?;
            write(
                &format!("images/{split}/{}.png", record.id),
                &bytes.into_inner(),
                &mut hashes,
            )?;
            if matches!(options.format.as_str(), "yolo" | "all") {
                let mut rows = Vec::new();
                let mut excluded = 0;
                for shape in &doc.annotations {
                    a::validate_geometry(&shape.geometry, record.width, record.height)?;
                    if matches!(shape.geometry, Geometry::Bbox { .. }) {
                        let class = profile
                            .classes
                            .iter()
                            .find(|c| {
                                Some(&c.id) == shape.class_id.as_ref() && c.namespace == "candidate"
                            })
                            .ok_or_else(|| a::invalid("Unmapped detection class."))?;
                        rows.push(yolo_row(
                            &shape.geometry,
                            record.width,
                            record.height,
                            class.export_id,
                        )?);
                    } else if profile
                        .layers
                        .iter()
                        .any(|l| l.id == shape.layer_id && l.layer_type == "instance")
                    {
                        excluded += 1;
                    }
                }
                if excluded > 0 {
                    warnings.push(format!("{}: {excluded} instance polygons are not part of the detection-only format; no polygon-to-box conversion was performed.",record.name));
                }
                object_count += rows.len() as u32;
                let text = if rows.is_empty() {
                    String::new()
                } else {
                    format!("{}\n", rows.join("\n"))
                };
                write(
                    &format!("labels/{split}/{}.txt", record.id),
                    text.as_bytes(),
                    &mut hashes,
                )?;
            }
            if options.format == "yoloSeg" || options.format == "coco" {
                let image_id = index as u32 + 1;
                coco_images.entry(split.into()).or_default().push(serde_json::json!({"id":image_id,"file_name":format!("images/{split}/{}.png",record.id),"width":record.width,"height":record.height}));
                let mut rows = Vec::new();
                for shape in &doc.annotations {
                    if !profile
                        .layers
                        .iter()
                        .any(|l| l.id == shape.layer_id && l.layer_type == "instance")
                    {
                        continue;
                    }
                    let class = profile
                        .classes
                        .iter()
                        .find(|c| {
                            Some(&c.id) == shape.class_id.as_ref() && c.namespace == "candidate"
                        })
                        .ok_or_else(|| a::invalid("Unknown instance class."))?;
                    if options.format == "coco" {
                        coco_id += 1;
                        coco_annotations.entry(split.into()).or_default().push(
                            instances::coco_annotation(
                                shape,
                                class.export_id,
                                image_id,
                                coco_id,
                                record.width,
                                record.height,
                            )?,
                        );
                        object_count += 1;
                    } else {
                        match instances::yolo_seg_row(
                            &shape.geometry,
                            record.width,
                            record.height,
                            class.export_id,
                        ) {
                            Ok(row) => {
                                rows.push(row);
                                object_count += 1;
                            }
                            Err(error) => {
                                let message =
                                    format!("{} / annotation {}: {error}", record.name, shape.id);
                                if !options.skip_unsupported {
                                    return Err(a::invalid(format!("{message} Choose COCO or explicitly allow skipped unsupported annotations.")));
                                }
                                warnings.push(message);
                            }
                        }
                    }
                }
                if options.format == "yoloSeg" {
                    write(
                        &format!("labels/{split}/{}.txt", record.id),
                        format!(
                            "{}{}",
                            rows.join("\n"),
                            if rows.is_empty() { "" } else { "\n" }
                        )
                        .as_bytes(),
                        &mut hashes,
                    )?;
                }
            }
            if options.format != "yolo" {
                for layer in profile.layers.iter().filter(|l| l.layer_type != "instance") {
                    let mut pixels = vec![
                        if layer.layer_type == "semantic" {
                            255
                        } else {
                            0
                        };
                        record.width as usize * record.height as usize
                    ];
                    let present = doc.masks.iter().find(|m| m.layer_id == layer.id);
                    if let Some(mask) = present {
                        pixels = a::unpack(mask)?;
                    }
                    if layer.layer_type == "semantic" {
                        let mut histogram = [0u64; 256];
                        for p in &pixels {
                            histogram[usize::from(*p)] += 1;
                        }
                        if (0..256).any(|v| histogram[v] > 0 && !valid_semantic[v]) {
                            return Err(a::invalid("Semantic mask contains unknown class IDs."));
                        }
                        if present.is_none() {
                            without_semantic += 1;
                        } else if histogram[255] * 10 > pixels.len() as u64 {
                            high_ignore.push(record.name.clone());
                        }
                        for v in 0..255 {
                            if histogram[v] > 0 {
                                class_pixels[v] += histogram[v];
                                class_images[v] += 1;
                            }
                        }
                        class_pixels[255] += histogram[255];
                    } else {
                        if pixels.iter().any(|p| *p > 1) {
                            return Err(a::invalid("Region mask is not binary."));
                        }
                        for shape in doc.annotations.iter().filter(|s| s.layer_id == layer.id) {
                            let layer_pixels = crate::domain::geometry::rasterize(
                                &shape.geometry,
                                record.width,
                                record.height,
                            )?;
                            for (target, value) in pixels.iter_mut().zip(layer_pixels) {
                                *target |= value;
                            }
                        }
                    }
                    write(
                        &format!("masks/{}/{split}/{}.png", layer.id, record.id),
                        &a::encode_png(record.width, record.height, &pixels)?,
                        &mut hashes,
                    )?;
                }
            }
            images.push(ExportImage {
                id: record.id.clone(),
                source: record.path.clone(),
                source_sha256: record.sha256.clone(),
                width: record.width,
                height: record.height,
                split: split.into(),
                group,
                revision: doc.revision,
                review_status: doc.review_status,
                event_label: doc.event_label,
                notes: doc.notes,
                source_video: crate::services::quality::origin(session, &record.id)?,
            });
            progress(Progress {
                completed: index as u32 + 1,
                total: records.len() as u32,
                message: record.name.clone(),
            });
        }
        for (split, percent) in [
            ("train", options.train_percent),
            ("val", options.val_percent),
            ("test", 100 - options.train_percent - options.val_percent),
        ] {
            if percent > 0 && !images.iter().any(|i| i.split == split) {
                warnings.push(format!("The {split} split is empty. Grouped assignment preserves whole source groups; use more groups or adjust the seed/ratios."));
            }
        }
        if options.format == "coco" {
            let categories=profile.classes.iter().filter(|c|c.namespace=="candidate").map(|c|serde_json::json!({"id":c.export_id,"name":c.display_name,"supercategory":"candidate"})).collect::<Vec<_>>();
            for split in ["train", "val", "test"] {
                let document = serde_json::json!({"info":{"description":"LabelFlow canonical detection/instance annotations"},"licenses":[],"images":coco_images.remove(split).unwrap_or_default(),"annotations":coco_annotations.remove(split).unwrap_or_default(),"categories":categories});
                write(
                    &format!("annotations/instances_{split}.json"),
                    a::json(&document)?.as_bytes(),
                    &mut hashes,
                )?;
            }
        }
        let mut yaml =
            "train: images/train\nval: images/val\ntest: images/test\nnames:\n".to_owned();
        for c in profile
            .classes
            .iter()
            .filter(|c| c.namespace == "candidate")
        {
            yaml.push_str(&format!(
                "  {}: {}\n",
                c.export_id,
                serde_json::to_string(&c.display_name).unwrap()
            ));
        }
        if matches!(options.format.as_str(), "yolo" | "all" | "yoloSeg") {
            write("data.yaml", yaml.as_bytes(), &mut hashes)?;
        }
        write("classes.json", a::json(&profile)?.as_bytes(), &mut hashes)?;
        if options.format == "semantic" || options.format == "all" {
            let total: u64 = class_pixels.iter().sum();
            let classes = profile
                .classes
                .iter()
                .filter(|c| c.namespace == crate::services::profile::SEMANTIC_NAMESPACE)
                .map(|c| {
                    let id = usize::from(c.export_id);
                    serde_json::json!({"id": c.export_id, "name": c.display_name,
                        "pixels": class_pixels[id],
                        "share": if total > 0 { class_pixels[id] as f64 / total as f64 } else { 0.0 },
                        "images": if id == 255 { serde_json::Value::Null } else { class_images[id].into() }})
                })
                .collect::<Vec<_>>();
            let absent = profile
                .classes
                .iter()
                .filter(|c| {
                    c.namespace == crate::services::profile::SEMANTIC_NAMESPACE
                        && !c.is_ignore
                        && class_images[usize::from(c.export_id)] == 0
                })
                .map(|c| format!("{} {}", c.export_id, c.display_name))
                .collect::<Vec<_>>();
            // Only meaningful once some semantic labels exist.
            if !absent.is_empty() && class_pixels[..255].iter().any(|n| *n > 0) {
                warnings.push(format!(
                    "Semantic classes with no exported pixels: {}.",
                    absent.join(", ")
                ));
            }
            if without_semantic > 0 {
                warnings.push(format!(
                    "{without_semantic} images have no semantic mask; they export as all Ignore (255)."
                ));
            }
            if !high_ignore.is_empty() {
                warnings.push(format!(
                    "{} images are more than 10% Ignore; see semantic-stats.json.",
                    high_ignore.len()
                ));
            }
            high_ignore.truncate(500);
            write(
                "semantic-stats.json",
                a::json(&serde_json::json!({"classes": classes,
                    "imagesWithoutSemanticMask": without_semantic,
                    "imagesOverTenPercentIgnore": high_ignore}))?
                .as_bytes(),
                &mut hashes,
            )?;
        }
        let manifest = serde_json::json!({"formatVersion":1,"coordinateConvention":"canonical image pixels; source EXIF normalized in exported images","ignoreId":255,"semanticIds":"project semantic class export IDs (see classes.json, namespace cityscapes); 255 = ignore","regionCompositing":"binary union of that region raster and polygon exteriors minus holes; independent of semantic classes","seed":options.seed,"trainPercent":options.train_percent,"valPercent":options.val_percent,"images":images,"warnings":warnings,"checksums":hashes});
        a::atomic_file(
            &staging.join("manifest.json"),
            a::json(&manifest)?.as_bytes(),
        )?;
        for (path, hash) in &hashes {
            if a::sha(&fs::read(staging.join(path))?) != *hash {
                return Err(a::invalid("Export checksum verification failed."));
            }
        }
        let destination = parent.join(name);
        let tx = session.connection.unchecked_transaction()?;
        for image in &images {
            tx.execute("INSERT INTO dataset_splits(media_id,split_name,split_group_id,assignment_method,seed) VALUES(?1,?2,?3,'grouped-sha256',?4) ON CONFLICT(media_id) DO UPDATE SET split_name=excluded.split_name,split_group_id=excluded.split_group_id,assignment_method=excluded.assignment_method,seed=excluded.seed",rusqlite::params![image.id,image.split,image.group,options.seed])?;
        }
        fs::rename(&staging, &destination)?;
        if let Err(error) = tx.commit() {
            let _ = fs::rename(&destination, &staging);
            return Err(error.into());
        }
        Ok(ExportReport {
            path: destination.to_string_lossy().into_owned(),
            images: images.len() as u32,
            annotations: object_count,
            warnings,
        })
    })();
    // Failed staging is intentionally preserved for diagnosis, never published.
    outcome
}
