use crate::{
    domain::{annotation::*, geometry, Result},
    services::{annotation as a, library, profile, project::ProjectSession},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CategoryMapping {
    pub source: i64,
    pub target: String,
}
pub struct Prepared {
    pub root: PathBuf,
    pub token: String,
    pub rows: Vec<(String, Vec<(i64, Geometry)>)>,
    pub classes: Vec<i64>,
    pub names: BTreeMap<i64, String>,
    pub fingerprint: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    pub token: String,
    pub classes: Vec<i64>,
    pub names: BTreeMap<i64, String>,
    pub files: usize,
    pub objects: usize,
}
impl Prepared {
    pub fn inspection(&self) -> Inspection {
        Inspection {
            token: self.token.clone(),
            classes: self.classes.clone(),
            names: self.names.clone(),
            files: self.rows.len(),
            objects: self.rows.iter().map(|r| r.1.len()).sum(),
        }
    }
}
pub fn parse_yolo_seg(text: &str, width: u32, height: u32) -> Result<Vec<(i64, Geometry)>> {
    let mut rows = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 7 || fields.len() % 2 != 1 {
            return Err(a::invalid("YOLO segmentation needs class + at least 3 x/y pairs. Detection rows are not segmentation masks."));
        }
        let class = fields[0]
            .parse::<i64>()
            .map_err(|_| a::invalid("Invalid YOLO class ID."))?;
        if !(0..=i64::from(u16::MAX)).contains(&class) {
            return Err(a::invalid("YOLO class ID is outside 0–65535."));
        }
        let values = fields[1..]
            .iter()
            .map(|s| {
                s.parse::<f64>()
                    .map_err(|_| a::invalid("Invalid polygon coordinate."))
            })
            .collect::<Result<Vec<_>>>()?;
        if values
            .iter()
            .any(|n| !n.is_finite() || *n < 0.0 || *n > 1.0)
        {
            return Err(a::invalid("YOLO polygon coordinates must be in [0,1]."));
        }
        let mut points = values
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[x, y]| Point {
                x: x * f64::from(width),
                y: y * f64::from(height),
            })
            .collect::<Vec<_>>();
        if points.len() > 3 && points.first() == points.last() {
            points.pop();
        }
        let shape = Geometry::Polygon {
            points,
            holes: Vec::new(),
        };
        a::validate_geometry(&shape, width, height)?;
        rows.push((class, shape));
        if rows.len() > 10000 {
            return Err(a::invalid("Too many objects in label file."));
        }
    }
    Ok(rows)
}
pub fn inspect_yolo_seg(s: &ProjectSession, folder: &Path) -> Result<Prepared> {
    let records = library::all(s)?;
    let mut pending = vec![if folder.join("labels").is_dir() {
        folder.join("labels")
    } else {
        folder.to_owned()
    }];
    let mut rows = Vec::new();
    let mut fingerprints = Vec::new();
    let mut seen = HashSet::new();
    while let Some(path) = pending.pop() {
        crate::security::reject_symlink(&path)?;
        if path.is_dir() {
            for entry in fs::read_dir(path)? {
                pending.push(entry?.path());
            }
        } else if path.extension().is_some_and(|e| e == "txt") {
            if fs::metadata(&path)?.len() > 4 * 1024 * 1024 {
                return Err(a::invalid("YOLO file exceeds 4 MiB."));
            }
            let record = match_image(
                &records,
                path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .as_ref(),
                None,
            )?;
            if !seen.insert(record.id.clone()) {
                return Err(a::invalid(
                    "More than one label file matches the same image.",
                ));
            }
            let bytes = fs::read(&path)?;
            fingerprints.push(a::sha(&bytes));
            rows.push((
                record.id.clone(),
                parse_yolo_seg(
                    std::str::from_utf8(&bytes)
                        .map_err(|_| a::invalid("YOLO text must be UTF-8."))?,
                    record.width,
                    record.height,
                )?,
            ));
        }
        if pending.len() + rows.len() > 100000 {
            return Err(a::invalid("Dataset import exceeds 100000 entries."));
        }
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    fingerprints.sort();
    prepared(
        &s.root,
        rows,
        BTreeMap::new(),
        a::sha(a::json(&fingerprints)?.as_bytes()),
    )
}
fn match_image<'a>(
    records: &'a [MediaRecord],
    name: &str,
    dimensions: Option<(u32, u32)>,
) -> Result<&'a MediaRecord> {
    let normalized = name.replace('\\', "/");
    let name = Path::new(&normalized);
    let stem = name.file_stem().unwrap_or_default().to_string_lossy();
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
        return Err(a::invalid(format!("{} must match exactly one imported image; found {}. Import images first, and disambiguate repeated filenames.",name.display(),candidates.len())));
    }
    let record = candidates[0];
    if dimensions.is_some_and(|d| d != (record.width, record.height)) {
        return Err(a::invalid(
            "COCO image dimensions disagree with canonical imported pixels.",
        ));
    }
    Ok(record)
}
fn prepared(
    root: &Path,
    rows: Vec<(String, Vec<(i64, Geometry)>)>,
    names: BTreeMap<i64, String>,
    fingerprint: String,
) -> Result<Prepared> {
    if rows.is_empty() {
        return Err(a::invalid("No matching annotation images were found."));
    }
    let classes = rows
        .iter()
        .flat_map(|(_, shapes)| shapes.iter().map(|s| s.0))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok(Prepared {
        root: root.to_owned(),
        token: uuid::Uuid::new_v4().to_string(),
        rows,
        classes,
        names,
        fingerprint,
    })
}
#[derive(Deserialize)]
struct Coco {
    images: Vec<CocoImage>,
    categories: Vec<CocoCategory>,
    annotations: Vec<CocoAnnotation>,
}
#[derive(Deserialize)]
struct CocoImage {
    id: i64,
    file_name: String,
    width: u32,
    height: u32,
}
#[derive(Deserialize)]
struct CocoCategory {
    id: i64,
    name: String,
}
#[derive(Deserialize)]
struct CocoAnnotation {
    id: i64,
    image_id: i64,
    category_id: i64,
    bbox: Option<Vec<f64>>,
    segmentation: Option<serde_json::Value>,
    #[serde(default)]
    iscrowd: u8,
}
pub fn inspect_coco(s: &ProjectSession, path: &Path) -> Result<Prepared> {
    if fs::metadata(path)?.len() > 64 * 1024 * 1024 {
        return Err(a::invalid("COCO JSON exceeds 64 MiB."));
    }
    let bytes = fs::read(path)?;
    let coco: Coco = serde_json::from_slice(&bytes)
        .map_err(|e| a::invalid(format!("Invalid COCO JSON: {e}")))?;
    if coco.images.len() > 100000 || coco.annotations.len() > 1_000_000 {
        return Err(a::invalid("COCO import exceeds M2 dataset budget."));
    }
    let records = library::all(s)?;
    let mut matches = HashMap::new();
    let mut rows = BTreeMap::new();
    let mut media_seen = HashSet::new();
    for image in coco.images {
        if matches.contains_key(&image.id) {
            return Err(a::invalid("Duplicate COCO image ID."));
        }
        let record = match_image(
            &records,
            &image.file_name,
            Some((image.width, image.height)),
        )?;
        if !media_seen.insert(record.id.clone()) {
            return Err(a::invalid(
                "Multiple COCO images refer to one imported image.",
            ));
        }
        matches.insert(image.id, (record.id.clone(), image.width, image.height));
        rows.insert(record.id.clone(), Vec::new());
    }
    let mut names = BTreeMap::new();
    for category in coco.categories {
        if category.id < 0
            || category.id > i64::from(i32::MAX)
            || names.insert(category.id, category.name).is_some()
        {
            return Err(a::invalid("Invalid or duplicate COCO category ID."));
        }
    }
    let mut ids = HashSet::new();
    for object in coco.annotations {
        if !ids.insert(object.id) || object.iscrowd > 1 {
            return Err(a::invalid(
                "Duplicate COCO annotation ID or invalid iscrowd.",
            ));
        }
        if !names.contains_key(&object.category_id) {
            return Err(a::invalid(
                "COCO annotation references an unknown category.",
            ));
        }
        let (id, width, height) = matches
            .get(&object.image_id)
            .ok_or_else(|| a::invalid("COCO annotation references an unknown image."))?;
        let segmentation = object
            .segmentation
            .filter(|v| !v.is_null() && !v.as_array().is_some_and(|a| a.is_empty()));
        if object.iscrowd == 1 && !segmentation.as_ref().is_some_and(|v| v.is_object()) {
            return Err(a::invalid("COCO iscrowd=1 requires RLE segmentation in M2; crowd boxes/polygons cannot retain crowd semantics in the canonical vector model."));
        }
        let geometry = if let Some(segmentation) = segmentation {
            if let Some(polygons) = segmentation.as_array() {
                let mut parts = Vec::new();
                for polygon in polygons {
                    let coords = polygon.as_array().ok_or_else(|| {
                        a::invalid("COCO polygons must be arrays of coordinates.")
                    })?;
                    if coords.len() < 6 || coords.len() % 2 != 0 {
                        return Err(a::invalid("COCO polygon needs at least 3 x/y pairs."));
                    }
                    let mut points = coords
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|[x, y]| {
                            Ok(Point {
                                x: x.as_f64().ok_or_else(|| {
                                    a::invalid("Invalid COCO polygon coordinate.")
                                })?,
                                y: y.as_f64().ok_or_else(|| {
                                    a::invalid("Invalid COCO polygon coordinate.")
                                })?,
                            })
                        })
                        .collect::<Result<Vec<_>>>()?;
                    if points.len() > 3 && points.first() == points.last() {
                        points.pop();
                    }
                    parts.push(PolygonRings {
                        points,
                        holes: Vec::new(),
                    });
                }
                if parts.len() == 1 {
                    let p = parts.pop().unwrap();
                    Geometry::Polygon {
                        points: p.points,
                        holes: p.holes,
                    }
                } else {
                    Geometry::MultiPolygon { polygons: parts }
                }
            } else {
                let size = segmentation["size"]
                    .as_array()
                    .ok_or_else(|| a::invalid("COCO RLE requires size [height,width]."))?;
                if size.len() != 2
                    || size[0].as_u64() != Some(u64::from(*height))
                    || size[1].as_u64() != Some(u64::from(*width))
                {
                    return Err(a::invalid("COCO RLE dimensions must match image."));
                }
                let counts = if let Some(text) = segmentation["counts"].as_str() {
                    geometry::decompress_coco(text, u64::from(*width) * u64::from(*height))?
                } else {
                    segmentation["counts"]
                        .as_array()
                        .ok_or_else(|| {
                            a::invalid("COCO counts must be integers or a compressed string.")
                        })?
                        .iter()
                        .map(|v| {
                            v.as_u64()
                                .and_then(|n| u32::try_from(n).ok())
                                .ok_or_else(|| a::invalid("Invalid COCO RLE count."))
                        })
                        .collect::<Result<Vec<_>>>()?
                };
                let pixels = geometry::decode_coco_counts(&counts, *width, *height)?;
                Geometry::Mask {
                    width: *width,
                    height: *height,
                    runs: a::pack(String::new(), *width, *height, &pixels).runs,
                    iscrowd: object.iscrowd == 1,
                }
            }
        } else {
            let bbox = object
                .bbox
                .ok_or_else(|| a::invalid("COCO annotation requires a bbox or segmentation."))?;
            if bbox.len() != 4 {
                return Err(a::invalid("COCO bbox requires x,y,width,height."));
            }
            Geometry::Bbox {
                x: bbox[0],
                y: bbox[1],
                width: bbox[2],
                height: bbox[3],
            }
        };
        a::validate_geometry(&geometry, *width, *height)?;
        rows.get_mut(id)
            .unwrap()
            .push((object.category_id, geometry));
    }
    prepared(&s.root, rows.into_iter().collect(), names, a::sha(&bytes))
}
pub fn apply(
    s: &mut ProjectSession,
    prepared: Prepared,
    mapping: &[CategoryMapping],
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<ImportReport> {
    if s.root != prepared.root {
        return Err(a::invalid(
            "Import preview belongs to another project root.",
        ));
    }
    let classes = profile::current(s)?.classes;
    let map = mapping
        .iter()
        .map(|m| (m.source, &m.target))
        .collect::<HashMap<_, _>>();
    if map.len() != mapping.len()
        || prepared.classes.iter().any(|c| !map.contains_key(c))
        || mapping.iter().any(|m| {
            !classes
                .iter()
                .any(|c| c.id == m.target && c.namespace == "candidate" && !c.is_ignore)
        })
    {
        return Err(a::invalid(
            "Map every source ID explicitly to a candidate object class.",
        ));
    }
    let total = prepared.rows.len() as u32;
    let mut report = ImportReport::default();
    for (index, (media_id, shapes)) in prepared.rows.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        let result = (|| -> Result<bool> {
            let mut doc = a::load(s, &media_id)?;
            if doc.review_status == "approved" {
                return Err(a::invalid(
                    "Approved image preserved. Change its review state before importing.",
                ));
            }
            let mut added = false;
            for (i, (class, geometry)) in shapes.into_iter().enumerate() {
                let signature = a::sha(
                    a::json(&(&prepared.fingerprint, &media_id, i, map[&class], &geometry))?
                        .as_bytes(),
                );
                let mut uuid = [0; 16];
                for (j, byte) in uuid.iter_mut().enumerate() {
                    *byte = u8::from_str_radix(&signature[j * 2..j * 2 + 2], 16).unwrap();
                }
                uuid[6] = (uuid[6] & 15) | 0x80;
                uuid[8] = (uuid[8] & 63) | 0x80;
                let id = uuid::Uuid::from_bytes(uuid).to_string();
                if doc.annotations.iter().any(|a| a.id == id) {
                    continue;
                }
                added = true;
                doc.annotations.push(Annotation {
                    id,
                    layer_id: "candidate_objects".into(),
                    class_id: Some(map[&class].clone()),
                    geometry,
                    source: "imported".into(),
                });
            }
            if added {
                doc.review_status = "in_progress".into();
                a::save(s, doc, &uuid::Uuid::new_v4().to_string())?;
            }
            Ok(added)
        })();
        match result {
            Ok(true) => report.imported += 1,
            Ok(false) => report.duplicates += 1,
            Err(e) => report.warnings.push(format!("{media_id}: {e}")),
        }
        progress(Progress {
            completed: index as u32 + 1,
            total,
            message: media_id,
        });
    }
    Ok(report)
}
