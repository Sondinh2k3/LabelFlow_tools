//! Batch import of class-ID PNG masks (pre-labels from an external model) and fixed
//! ignore templates (e.g. a bus hood), applied to many images in one job.
use crate::{
    domain::{annotation::*, Result},
    services::{annotation as a, library, profile, project::ProjectSession},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

const MAX_FILES: usize = 100_000;

pub struct PreparedMaskFolder {
    pub token: String,
    pub layer_id: String,
    pub items: Vec<(String, PathBuf)>,
    pub values: Vec<u8>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaskFolderInspection {
    pub token: String,
    pub values: Vec<u8>,
    pub matched: u32,
    pub approved_skipped: u32,
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MergeMode {
    /// Only pixels without a human label change: semantic 255, region 0.
    FillUnlabeled,
    /// The imported mask replaces the layer.
    Replace,
}

fn png_files(folder: &Path) -> Result<Vec<PathBuf>> {
    let mut todo = vec![folder.to_path_buf()];
    let mut files = Vec::new();
    while let Some(path) = todo.pop() {
        crate::security::reject_symlink(&path)?;
        if path.is_dir() {
            for entry in fs::read_dir(path)? {
                todo.push(entry?.path());
            }
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("png"))
        {
            files.push(path);
        }
        if todo.len() + files.len() > MAX_FILES {
            return Err(a::invalid("Mask folder is too large."));
        }
    }
    files.sort();
    Ok(files)
}

fn raster_layer(session: &ProjectSession, layer_id: &str) -> Result<LayerDefinition> {
    profile::current(session)?
        .layers
        .into_iter()
        .find(|l| l.id == layer_id && l.layer_type != "instance")
        .ok_or_else(|| a::invalid("Select a semantic or region raster layer."))
}

/// Matches `<media uuid>.png` or `<unique image stem>.png`, decodes every match once to
/// check dimensions and collect the distinct IDs. Pixels are not kept in memory.
pub fn inspect_folder(
    session: &ProjectSession,
    folder: &Path,
    layer_id: &str,
    cancel: &AtomicBool,
    progress: impl Fn(Progress),
) -> Result<(PreparedMaskFolder, MaskFolderInspection)> {
    raster_layer(session, layer_id)?;
    let records = library::all(session)?;
    let mut by_key: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, r) in records.iter().enumerate() {
        by_key.entry(r.id.clone()).or_default().push(i);
        if let Some(stem) = Path::new(&r.name).file_stem() {
            let stem = stem.to_string_lossy().to_string();
            if stem != r.id {
                by_key.entry(stem).or_default().push(i);
            }
        }
    }
    let files = png_files(folder)?;
    let total = files.len() as u32;
    let mut seen = [false; 256];
    let mut items = Vec::new();
    let mut warnings = Vec::new();
    let mut approved = 0;
    let mut claimed = HashMap::new();
    for (n, path) in files.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(a::invalid("Mask folder inspection cancelled."));
        }
        progress(Progress {
            completed: n as u32,
            total,
            message: "Checking masks".into(),
        });
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let record = match by_key.get(stem.as_ref()).map(Vec::as_slice) {
            Some([i]) => &records[*i],
            Some(_) => {
                warnings.push(format!("{name}: matches several images; skipped."));
                continue;
            }
            None => {
                warnings.push(format!("{name}: no imported image with this name."));
                continue;
            }
        };
        if let Some(other) = claimed.insert(record.id.clone(), name.to_string()) {
            warnings.push(format!("{name}: same image as {other}; skipped."));
            continue;
        }
        if record.review_status == "approved" {
            approved += 1;
            continue;
        }
        let result = (|| -> Result<()> {
            if fs::metadata(&path)?.len() > crate::services::media::MAX_FILE_BYTES {
                return Err(a::invalid("mask file is too large"));
            }
            let (w, h, pixels) = a::decode_png(&fs::read(&path)?)?;
            if (w, h) != (record.width, record.height) {
                return Err(a::invalid(format!(
                    "mask is {w}×{h}, image is {}×{}",
                    record.width, record.height
                )));
            }
            for p in pixels {
                seen[p as usize] = true;
            }
            Ok(())
        })();
        match result {
            Ok(()) => items.push((record.id.clone(), path)),
            Err(e) => warnings.push(format!("{name}: {}", e.message)),
        }
    }
    if items.is_empty() {
        return Err(a::invalid(
            "No usable masks: name each PNG after its image (same file stem or media ID) at the same size.",
        ));
    }
    let values = (0..=255u8)
        .filter(|v| seen[*v as usize])
        .collect::<Vec<_>>();
    warnings.truncate(200);
    let token = uuid::Uuid::new_v4().to_string();
    let inspection = MaskFolderInspection {
        token: token.clone(),
        values: values.clone(),
        matched: items.len() as u32,
        approved_skipped: approved,
        warnings,
    };
    Ok((
        PreparedMaskFolder {
            token,
            layer_id: layer_id.into(),
            items,
            values,
        },
        inspection,
    ))
}

fn merge(old: Option<&[u8]>, imported: Vec<u8>, mode: MergeMode, empty: u8) -> Vec<u8> {
    match (mode, old) {
        (MergeMode::FillUnlabeled, Some(old)) => old
            .iter()
            .zip(imported)
            .map(|(&o, n)| if o == empty { n } else { o })
            .collect(),
        _ => imported,
    }
}

/// Applies every prepared mask through the normal save path (revision, journal, assets).
/// Approved images are never changed; each image succeeds or fails on its own.
pub fn apply_folder(
    session: &mut ProjectSession,
    prepared: PreparedMaskFolder,
    mapping: &[PixelMapping],
    mode: MergeMode,
    cancel: &AtomicBool,
    progress: impl Fn(Progress),
) -> Result<ImportReport> {
    let layer = raster_layer(session, &prepared.layer_id)?;
    let semantic = layer.layer_type == "semantic";
    let mut table = [None::<u8>; 256];
    for m in mapping {
        if table[m.source as usize].replace(m.target).is_some() {
            return Err(a::invalid("Each source ID needs exactly one mapping."));
        }
    }
    if prepared.values.iter().any(|v| table[*v as usize].is_none()) {
        return Err(a::invalid(
            "Every imported mask ID needs an explicit mapping.",
        ));
    }
    let empty = if semantic { 255 } else { 0 };
    let total = prepared.items.len() as u32;
    let mut report = ImportReport {
        imported: 0,
        duplicates: 0,
        warnings: Vec::new(),
        cancelled: false,
    };
    for (n, (media_id, path)) in prepared.items.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        progress(Progress {
            completed: n as u32,
            total,
            message: "Importing masks".into(),
        });
        let result = (|| -> Result<bool> {
            let mut doc = a::load(session, media_id)?;
            if doc.review_status == "approved" {
                return Err(a::invalid("approved since inspection; skipped"));
            }
            let (w, h, pixels) = a::decode_png(&fs::read(path)?)?;
            let mut mapped = Vec::with_capacity(pixels.len());
            for p in pixels {
                mapped.push(
                    table[p as usize].ok_or_else(|| a::invalid("file changed after inspection"))?,
                );
            }
            let old = doc
                .masks
                .iter()
                .find(|m| m.layer_id == prepared.layer_id)
                .map(a::unpack)
                .transpose()?;
            let pixels = merge(old.as_deref(), mapped, mode, empty);
            if old.as_deref() == Some(pixels.as_slice()) {
                return Ok(false);
            }
            doc.masks.retain(|m| m.layer_id != prepared.layer_id);
            doc.masks
                .push(a::pack(prepared.layer_id.clone(), w, h, &pixels));
            if doc.review_status == "unlabeled" {
                doc.review_status = "in_progress".into();
            }
            a::save(session, doc, &uuid::Uuid::new_v4().to_string())?;
            Ok(true)
        })();
        match result {
            Ok(true) => report.imported += 1,
            Ok(false) => report.duplicates += 1,
            Err(e) => report
                .warnings
                .push(format!("{}: {}", path.display(), e.message)),
        }
    }
    progress(Progress {
        completed: total,
        total,
        message: "Masks imported".into(),
    });
    Ok(report)
}

pub struct PreparedTemplate {
    pub token: String,
    pub width: u32,
    pub height: u32,
    /// true = force semantic ignore (255) at this pixel.
    pub ignore: Vec<bool>,
    pub targets: Vec<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInspection {
    pub token: String,
    pub width: u32,
    pub height: u32,
    pub ignore_pixels: u64,
    pub images: u32,
    pub approved_skipped: u32,
}

/// A template PNG marks pixels to force to Ignore (any non-zero value) on every image of
/// the same size, optionally limited to one trip. Typical use: a fixed bus hood or dash.
pub fn inspect_template(
    session: &ProjectSession,
    path: &Path,
    trip_id: Option<&str>,
) -> Result<(PreparedTemplate, TemplateInspection)> {
    if fs::metadata(path)?.len() > crate::services::media::MAX_FILE_BYTES {
        return Err(a::invalid("Template file is too large."));
    }
    let (width, height, pixels) = a::decode_png(&fs::read(path)?)?;
    let ignore = pixels.iter().map(|p| *p != 0).collect::<Vec<_>>();
    let ignore_pixels = ignore.iter().filter(|v| **v).count() as u64;
    if ignore_pixels == 0 {
        return Err(a::invalid(
            "Template has no non-zero pixels; paint the ignore area white on black.",
        ));
    }
    let in_trip: Option<std::collections::HashSet<String>> = match trip_id {
        Some(trip) => {
            let mut statement = session
                .connection
                .prepare("SELECT media_id FROM media_review_metadata WHERE trip_id=?1")?;
            let ids = statement
                .query_map([trip], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<_, _>>()?;
            Some(ids)
        }
        None => None,
    };
    let mut targets = Vec::new();
    let mut approved = 0;
    for r in library::all(session)? {
        if (r.width, r.height) != (width, height)
            || in_trip.as_ref().is_some_and(|set| !set.contains(&r.id))
        {
            continue;
        }
        if r.review_status == "approved" {
            approved += 1;
        } else {
            targets.push(r.id);
        }
    }
    if targets.is_empty() {
        return Err(a::invalid(format!(
            "No unapproved images of {width}×{height} to apply the template to."
        )));
    }
    let token = uuid::Uuid::new_v4().to_string();
    let inspection = TemplateInspection {
        token: token.clone(),
        width,
        height,
        ignore_pixels,
        images: targets.len() as u32,
        approved_skipped: approved,
    };
    Ok((
        PreparedTemplate {
            token,
            width,
            height,
            ignore,
            targets,
        },
        inspection,
    ))
}

pub fn apply_template(
    session: &mut ProjectSession,
    prepared: PreparedTemplate,
    cancel: &AtomicBool,
    progress: impl Fn(Progress),
) -> Result<ImportReport> {
    let layer = profile::SEMANTIC_LAYER.to_string();
    let total = prepared.targets.len() as u32;
    let mut report = ImportReport {
        imported: 0,
        duplicates: 0,
        warnings: Vec::new(),
        cancelled: false,
    };
    for (n, media_id) in prepared.targets.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        progress(Progress {
            completed: n as u32,
            total,
            message: "Applying ignore template".into(),
        });
        let result = (|| -> Result<bool> {
            let mut doc = a::load(session, media_id)?;
            if doc.review_status == "approved" {
                return Err(a::invalid("approved; skipped"));
            }
            let Some(index) = doc.masks.iter().position(|m| m.layer_id == layer) else {
                // No semantic mask yet: every pixel is already Ignore.
                return Ok(false);
            };
            let mut pixels = a::unpack(&doc.masks[index])?;
            if pixels.len() != prepared.ignore.len() {
                return Err(a::invalid("size changed"));
            }
            let mut changed = false;
            for (p, &ignore) in pixels.iter_mut().zip(&prepared.ignore) {
                if ignore && *p != 255 {
                    *p = 255;
                    changed = true;
                }
            }
            if !changed {
                return Ok(false);
            }
            doc.masks[index] = a::pack(layer.clone(), prepared.width, prepared.height, &pixels);
            a::save(session, doc, &uuid::Uuid::new_v4().to_string())?;
            Ok(true)
        })();
        match result {
            Ok(true) => report.imported += 1,
            Ok(false) => report.duplicates += 1,
            Err(e) => report.warnings.push(format!("{media_id}: {}", e.message)),
        }
    }
    progress(Progress {
        completed: total,
        total,
        message: "Template applied".into(),
    });
    Ok(report)
}
