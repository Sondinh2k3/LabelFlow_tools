use crate::{
    domain::{annotation::*, AppError, ErrorCode, Result},
    services::{annotation as a, library, profile, project::ProjectSession},
};
pub mod instances;
pub mod masks;
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};
pub fn parse_yolo(text: &str, width: u32, height: u32) -> Result<Vec<(u8, Geometry)>> {
    let mut result = Vec::new();
    for (line, row) in text.lines().enumerate() {
        if row.trim().is_empty() {
            continue;
        }
        let parts: Vec<_> = row.split_whitespace().collect();
        if parts.len() != 5 {
            return Err(a::invalid(format!("YOLO detection line {} must have exactly 5 fields; polygons are not detection boxes.",line+1)));
        }
        let class: u8 = parts[0]
            .parse()
            .map_err(|_| a::invalid("YOLO class ID must be 0..255."))?;
        let values = parts[1..]
            .iter()
            .map(|p| {
                p.parse::<f64>()
                    .map_err(|_| a::invalid("Invalid YOLO coordinate."))
            })
            .collect::<Result<Vec<_>>>()?;
        if values
            .iter()
            .any(|v| !v.is_finite() || *v < 0.0 || *v > 1.0)
        {
            return Err(a::invalid(
                "YOLO normalized values must be finite and in [0,1].",
            ));
        }
        let (cx, cy, w, h) = (
            values[0] * f64::from(width),
            values[1] * f64::from(height),
            values[2] * f64::from(width),
            values[3] * f64::from(height),
        );
        let g = Geometry::Bbox {
            x: cx - w / 2.0,
            y: cy - h / 2.0,
            width: w,
            height: h,
        };
        a::validate_geometry(&g, width, height)?;
        result.push((class, g));
        if result.len() > 10_000 {
            return Err(a::invalid("Too many YOLO objects in one image."));
        }
    }
    Ok(result)
}
pub struct PreparedYolo {
    pub token: String,
    pub rows: Vec<(String, Vec<(u8, Geometry)>)>,
    pub classes: Vec<u8>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YoloInspection {
    pub token: String,
    pub classes: Vec<u8>,
    pub files: usize,
    pub objects: usize,
}
pub fn inspect_yolo(session: &ProjectSession, folder: &Path) -> Result<PreparedYolo> {
    let records = library::all(session)?;
    let directory = if folder.join("labels").is_dir() {
        folder.join("labels")
    } else {
        folder.to_path_buf()
    };
    let mut todo = vec![directory];
    let mut files = Vec::new();
    while let Some(path) = todo.pop() {
        crate::security::reject_symlink(&path)?;
        if path.is_dir() {
            for entry in fs::read_dir(path)? {
                todo.push(entry?.path());
            }
        } else if path.extension().is_some_and(|e| e == "txt") {
            files.push(path);
        }
        if todo.len() + files.len() > 100_000 {
            return Err(a::invalid("Label import is too large."));
        }
    }
    files.sort();
    let mut rows = Vec::new();
    let mut classes = HashSet::new();
    for path in files {
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let candidates = records
            .iter()
            .filter(|r| {
                r.id == stem
                    || Path::new(&r.name)
                        .file_stem()
                        .is_some_and(|s| s.to_string_lossy() == stem)
            })
            .collect::<Vec<_>>();
        if candidates.len() != 1 {
            return Err(a::invalid(format!(
                "{}: expected one matching imported image, found {}.",
                path.display(),
                candidates.len()
            )));
        }
        if fs::metadata(&path)?.len() > 4 * 1024 * 1024 {
            return Err(a::invalid("Label file exceeds 4 MiB."));
        }
        let record = candidates[0];
        let parsed = parse_yolo(&fs::read_to_string(&path)?, record.width, record.height)?;
        classes.extend(parsed.iter().map(|p| p.0));
        rows.push((record.id.clone(), parsed));
    }
    if rows.is_empty() {
        return Err(a::invalid("No YOLO detection label files found."));
    }
    let mut classes = classes.into_iter().collect::<Vec<_>>();
    classes.sort();
    Ok(PreparedYolo {
        token: uuid::Uuid::new_v4().to_string(),
        rows,
        classes,
    })
}
pub fn import_yolo(
    session: &mut ProjectSession,
    prepared: PreparedYolo,
    mapping: &[ClassMapping],
) -> Result<ImportReport> {
    let profile = profile::current(session)?;
    let map: HashMap<u8, &String> = mapping.iter().map(|m| (m.source, &m.target)).collect();
    if map.len() != mapping.len()
        || prepared.classes.iter().any(|c| !map.contains_key(c))
        || mapping.iter().any(|m| {
            !profile
                .classes
                .iter()
                .any(|c| c.id == m.target && c.namespace == "candidate" && !c.is_ignore)
        })
    {
        return Err(a::invalid(
            "Every source class must be explicitly mapped to a detection class.",
        ));
    }
    let mut report = ImportReport::default();
    for (media_id, rows) in prepared.rows {
        let result = (|| -> Result<()> {
            let mut doc = a::load(session, &media_id)?;
            if doc.review_status == "approved" {
                return Err(a::invalid(
                    "Approved image was preserved; change its review state before importing.",
                ));
            }
            for (class, g) in rows {
                doc.annotations.push(Annotation {
                    id: uuid::Uuid::new_v4().to_string(),
                    layer_id: "candidate_objects".into(),
                    class_id: Some(map[&class].clone()),
                    geometry: g,
                    source: "imported".into(),
                });
            }
            doc.review_status = "in_progress".into();
            a::save(session, doc, &uuid::Uuid::new_v4().to_string())?;
            Ok(())
        })();
        match result {
            Ok(()) => report.imported += 1,
            Err(e) => report.warnings.push(format!("{media_id}: {e}")),
        }
    }
    Ok(report)
}
pub struct PreparedMask {
    pub token: String,
    pub media_id: String,
    pub layer_id: String,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub revision: u32,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaskInspection {
    pub token: String,
    pub values: Vec<u8>,
    pub width: u32,
    pub height: u32,
}
pub fn inspect_mask(
    session: &ProjectSession,
    path: &Path,
    media_id: &str,
    layer_id: &str,
) -> Result<PreparedMask> {
    let record = library::record(session, media_id)?;
    let profile = profile::current(session)?;
    if !profile
        .layers
        .iter()
        .any(|l| l.id == layer_id && l.layer_type != "instance")
    {
        return Err(a::invalid("Select a semantic or region raster layer."));
    }
    if fs::metadata(path)?.len() > crate::services::media::MAX_FILE_BYTES {
        return Err(a::invalid("Mask file is too large."));
    }
    let (w, h, pixels) = a::decode_png(&fs::read(path)?)?;
    if (w, h) != (record.width, record.height) {
        return Err(a::invalid(
            "Imported mask dimensions must exactly match the canonical image.",
        ));
    }
    let doc = a::load(session, media_id)?;
    if doc.review_status == "approved" {
        return Err(a::invalid(
            "Change the approved image review state before replacing its mask.",
        ));
    }
    Ok(PreparedMask {
        token: uuid::Uuid::new_v4().to_string(),
        media_id: media_id.into(),
        layer_id: layer_id.into(),
        width: w,
        height: h,
        pixels,
        revision: doc.revision,
    })
}
pub fn apply_mask(
    session: &mut ProjectSession,
    prepared: PreparedMask,
    mapping: &[PixelMapping],
) -> Result<Document> {
    let map: HashMap<_, _> = mapping.iter().map(|m| (m.source, m.target)).collect();
    if map.len() != mapping.len() || prepared.pixels.iter().any(|p| !map.contains_key(p)) {
        return Err(a::invalid(
            "Every imported mask ID needs an explicit mapping.",
        ));
    }
    let pixels = prepared.pixels.iter().map(|p| map[p]).collect::<Vec<_>>();
    let mut doc = a::load(session, &prepared.media_id)?;
    if doc.revision != prepared.revision {
        return Err(AppError::new(
            ErrorCode::Conflict,
            "Image changed after mask inspection. Inspect again.",
        ));
    }
    doc.masks.retain(|m| m.layer_id != prepared.layer_id);
    doc.masks.push(a::pack(
        prepared.layer_id,
        prepared.width,
        prepared.height,
        &pixels,
    ));
    doc.review_status = "in_progress".into();
    a::save(session, doc, &uuid::Uuid::new_v4().to_string())?;
    a::load(session, &prepared.media_id)
}
