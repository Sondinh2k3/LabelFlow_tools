use crate::{
    domain::{annotation::*, AppError, ErrorCode, Result},
    services::{profile, project::ProjectSession},
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::{Cursor, Write},
    path::Path,
};

pub fn invalid(message: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::InvalidInput, message)
}
pub fn json<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|e| invalid(e.to_string()))
}
pub fn parse<T: serde::de::DeserializeOwned>(value: &str) -> Result<T> {
    serde_json::from_str(value).map_err(|e| AppError::new(ErrorCode::Database, e.to_string()))
}
pub fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn valid_status(status: &str) -> bool {
    [
        "unlabeled",
        "in_progress",
        "needs_review",
        "approved",
        "rejected",
    ]
    .contains(&status)
}
fn validate_runs(mask: &RasterMask) -> Result<usize> {
    let len = u64::from(mask.width) * u64::from(mask.height);
    if len == 0
        || len > super::media::MAX_PIXELS
        || !mask.runs.len().is_multiple_of(2)
        || mask.runs.len() as u64 > len * 2
    {
        return Err(invalid("Invalid raster dimensions or run count."));
    }
    let mut covered = 0u64;
    for pair in mask.runs.as_chunks::<2>().0 {
        if pair[0] > 255 || pair[1] == 0 || covered + u64::from(pair[1]) > len {
            return Err(invalid("Invalid raster run."));
        }
        covered += u64::from(pair[1]);
    }
    if covered != len {
        return Err(invalid("Raster runs do not cover the image."));
    }
    Ok(len as usize)
}
pub fn unpack(mask: &RasterMask) -> Result<Vec<u8>> {
    let mut pixels = Vec::with_capacity(validate_runs(mask)?);
    for pair in mask.runs.as_chunks::<2>().0 {
        pixels.resize(pixels.len() + pair[1] as usize, pair[0] as u8);
    }
    Ok(pixels)
}
pub fn pack(layer_id: String, width: u32, height: u32, pixels: &[u8]) -> RasterMask {
    let mut runs = Vec::new();
    for &p in pixels {
        if runs.len() >= 2 && runs[runs.len() - 2] == u32::from(p) {
            let n = runs.len() - 1;
            runs[n] += 1;
        } else {
            runs.extend([u32::from(p), 1]);
        }
    }
    RasterMask {
        layer_id,
        width,
        height,
        runs,
    }
}
pub fn encode_png(width: u32, height: u32, pixels: &[u8]) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| invalid(e.to_string()))?;
        writer
            .write_image_data(pixels)
            .map_err(|e| invalid(e.to_string()))?;
    }
    Ok(bytes)
}
pub fn decode_png(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>)> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_limits(png::Limits {
        bytes: 128 * 1024 * 1024,
    });
    let mut reader = decoder.read_info().map_err(|e| invalid(e.to_string()))?;
    let header = reader.info();
    if header.bit_depth != png::BitDepth::Eight
        || !matches!(
            header.color_type,
            png::ColorType::Grayscale | png::ColorType::Indexed
        )
        || u64::from(header.width) * u64::from(header.height) > super::media::MAX_PIXELS
    {
        return Err(invalid("Mask must be an 8-bit grayscale or indexed PNG within image limits; RGB masks are not class IDs."));
    }
    let mut data = vec![
        0;
        reader
            .output_buffer_size()
            .ok_or_else(|| invalid("Mask allocation overflow."))?
    ];
    let info = reader
        .next_frame(&mut data)
        .map_err(|e| invalid(e.to_string()))?;
    data.truncate(info.buffer_size());
    if data.len() != info.width as usize * info.height as usize {
        return Err(invalid("Unexpected PNG mask encoding."));
    }
    Ok((info.width, info.height, data))
}
pub fn atomic_file(path: &Path, bytes: &[u8]) -> Result<()> {
    crate::security::reject_symlink(path)?;
    if path.exists() {
        if fs::read(path)? == bytes {
            return Ok(());
        }
        return Err(invalid("Existing immutable asset has different contents."));
    }
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(temporary, path)?;
    Ok(())
}
fn cross(a: &Point, b: &Point, c: &Point) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}
fn on(a: &Point, b: &Point, c: &Point) -> bool {
    cross(a, b, c).abs() < 1e-8
        && c.x >= a.x.min(b.x) - 1e-8
        && c.x <= a.x.max(b.x) + 1e-8
        && c.y >= a.y.min(b.y) - 1e-8
        && c.y <= a.y.max(b.y) + 1e-8
}
fn intersects(a: &Point, b: &Point, c: &Point, d: &Point) -> bool {
    let (x, y, z, w) = (
        cross(a, b, c),
        cross(a, b, d),
        cross(c, d, a),
        cross(c, d, b),
    );
    (x * y < 0.0 && z * w < 0.0) || on(a, b, c) || on(a, b, d) || on(c, d, a) || on(c, d, b)
}
pub fn inside(p: &Point, ring: &[Point]) -> bool {
    let mut result = false;
    for i in 0..ring.len() {
        let a = &ring[i];
        let b = &ring[(i + 1) % ring.len()];
        if on(a, b, p) {
            return true;
        }
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            result = !result;
        }
    }
    result
}
fn validate_ring(ring: &[Point], width: u32, height: u32) -> Result<()> {
    if ring.len() < 3 || ring.len() > 10_000 {
        return Err(invalid("Polygons require 3–10000 vertices."));
    }
    let mut area = 0.0;
    for (i, p) in ring.iter().enumerate() {
        if !p.x.is_finite()
            || !p.y.is_finite()
            || p.x < 0.0
            || p.y < 0.0
            || p.x > f64::from(width)
            || p.y > f64::from(height)
        {
            return Err(invalid("Polygon vertex is outside the canonical image."));
        }
        let next = &ring[(i + 1) % ring.len()];
        if (next.x - p.x).hypot(next.y - p.y) < 1e-8 {
            return Err(invalid("Polygon has duplicate vertices."));
        }
        area += p.x * next.y - next.x * p.y;
        for j in i + 1..ring.len() {
            if j == i + 1 || (i == 0 && j == ring.len() - 1) {
                continue;
            }
            if intersects(p, next, &ring[j], &ring[(j + 1) % ring.len()]) {
                return Err(invalid("Self-intersecting polygon."));
            }
        }
    }
    if area.abs() < 1e-6 {
        return Err(invalid("Polygon has zero area."));
    }
    Ok(())
}
pub fn validate_geometry(g: &Geometry, width: u32, height: u32) -> Result<()> {
    match g {
        Geometry::Bbox {
            x,
            y,
            width: w,
            height: h,
        } => {
            if [*x, *y, *w, *h].iter().any(|v| !v.is_finite())
                || *x < 0.0
                || *y < 0.0
                || *w <= 0.0
                || *h <= 0.0
                || x + w > f64::from(width) + 1e-7
                || y + h > f64::from(height) + 1e-7
            {
                return Err(invalid(
                    "Bounding box must have positive area and lie inside the image.",
                ));
            }
        }
        Geometry::MultiPolygon { polygons } => {
            if polygons.is_empty() || polygons.len() > 1000 {
                return Err(invalid("Multi-polygon component count is invalid."));
            }
            for polygon in polygons {
                validate_geometry(
                    &Geometry::Polygon {
                        points: polygon.points.clone(),
                        holes: polygon.holes.clone(),
                    },
                    width,
                    height,
                )?;
            }
        }
        Geometry::Mask {
            width: w,
            height: h,
            runs,
            ..
        } => {
            if (*w, *h) != (width, height) {
                return Err(invalid("Instance mask dimensions must match image."));
            }
            crate::domain::geometry::mask_pixels(*w, *h, runs)?;
        }
        Geometry::Polygon { points, holes } => {
            validate_ring(points, width, height)?;
            for (i, hole) in holes.iter().enumerate() {
                validate_ring(hole, width, height)?;
                if !hole.iter().all(|p| inside(p, points)) {
                    return Err(invalid("Polygon hole lies outside its exterior."));
                }
                for outer in std::iter::once(points).chain(holes[..i].iter()) {
                    for a in 0..hole.len() {
                        for b in 0..outer.len() {
                            if intersects(
                                &hole[a],
                                &hole[(a + 1) % hole.len()],
                                &outer[b],
                                &outer[(b + 1) % outer.len()],
                            ) {
                                return Err(invalid("Polygon rings intersect."));
                            }
                        }
                    }
                }
                if holes[..i]
                    .iter()
                    .any(|other| inside(&hole[0], other) || inside(&other[0], hole))
                {
                    return Err(invalid("Polygon holes overlap."));
                }
            }
        }
    }
    Ok(())
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredMask {
    layer_id: String,
    asset_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    annotation_id: Option<String>,
}
#[derive(Serialize, Deserialize)]
struct Stored {
    document: Document,
    assets: Vec<StoredMask>,
}

pub fn load(session: &ProjectSession, media_id: &str) -> Result<Document> {
    let record = super::library::record(session, media_id)?;
    let saved: Option<String> = session
        .connection
        .query_row(
            "SELECT state_json FROM annotation_documents WHERE media_id=?1",
            [media_id],
            |r| r.get(0),
        )
        .optional()?;
    let Some(saved) = saved else {
        let group = super::quality::group(session, media_id)?;
        return Ok(Document {
            media_id: media_id.into(),
            revision: 0,
            review_status: record.review_status,
            event_label: None,
            notes: String::new(),
            split_group: group,
            annotations: Vec::new(),
            masks: Vec::new(),
        });
    };
    let mut stored: Stored = parse(&saved)?;
    for asset in stored.assets {
        let (relative, hash, w, h): (String, String, u32, u32) = session.connection.query_row(
            "SELECT relative_path,sha256,width,height FROM mask_assets WHERE id=?1",
            [asset.asset_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        let expected = format!("masks/{hash}.png");
        if relative != expected {
            return Err(invalid("Unsafe mask asset path."));
        }
        let path = session.root.join(".labelflow").join(&relative);
        crate::security::reject_symlink(&path)?;
        let bytes = fs::read(path)?;
        if sha(&bytes) != hash {
            return Err(AppError::new(
                ErrorCode::Database,
                "Mask checksum mismatch. Restore a complete backup.",
            ));
        }
        let (dw, dh, pixels) = decode_png(&bytes)?;
        if (w, h) != (dw, dh) || (w, h) != (record.width, record.height) {
            return Err(invalid("Mask dimensions disagree with source image."));
        }
        if let Some(id) = asset.annotation_id {
            let shape = stored
                .document
                .annotations
                .iter_mut()
                .find(|a| a.id == id)
                .ok_or_else(|| invalid("Instance mask reference is missing its annotation."))?;
            let iscrowd = matches!(shape.geometry, Geometry::Mask { iscrowd: true, .. });
            shape.geometry = Geometry::Mask {
                width: w,
                height: h,
                runs: pack(String::new(), w, h, &pixels).runs,
                iscrowd,
            };
        } else {
            stored
                .document
                .masks
                .push(pack(asset.layer_id, w, h, &pixels));
        }
    }
    stored.document.split_group = super::quality::group(session, media_id)?;
    Ok(stored.document)
}
pub fn save(session: &mut ProjectSession, document: Document, operation_id: &str) -> Result<u32> {
    save_inner(session, document, operation_id, None)
}
pub(crate) fn save_suggestion(
    session: &mut ProjectSession,
    document: Document,
    operation_id: &str,
    suggestion_id: &str,
) -> Result<u32> {
    save_inner(
        session,
        document,
        operation_id,
        Some(("ai_suggestions", suggestion_id)),
    )
}
pub(crate) fn save_review_prediction(
    session: &mut ProjectSession,
    document: Document,
    operation_id: &str,
    id: &str,
) -> Result<u32> {
    save_inner(
        session,
        document,
        operation_id,
        Some(("review_predictions", id)),
    )
}
fn save_inner(
    session: &mut ProjectSession,
    mut document: Document,
    operation_id: &str,
    suggestion_id: Option<(&str, &str)>,
) -> Result<u32> {
    uuid::Uuid::parse_str(operation_id).map_err(|_| invalid("Invalid operation ID."))?;
    let record = super::library::record(session, &document.media_id)?;
    if !valid_status(&document.review_status)
        || document.notes.len() > 32_000
        || document.split_group.len() > 512
        || document
            .event_label
            .as_deref()
            .is_some_and(|e| !["positive", "negative", "uncertain"].contains(&e))
        || document.annotations.len() > 10_000
    {
        return Err(invalid("Invalid document metadata or annotation count."));
    }
    let video_frame: bool = session.connection.query_row(
        "SELECT source_video_id IS NOT NULL FROM media WHERE id=?1",
        [&document.media_id],
        |r| r.get(0),
    )?;
    let trip_assigned: bool = session.connection.query_row("SELECT EXISTS(SELECT 1 FROM media_review_metadata WHERE media_id=?1 AND trip_id IS NOT NULL)",[&document.media_id],|r|r.get(0))?;
    if video_frame || trip_assigned {
        document.split_group = super::quality::group(session, &document.media_id)?;
    }
    let profile = profile::current(session)?;
    let mut ids = HashSet::new();
    for a in &document.annotations {
        uuid::Uuid::parse_str(&a.id).map_err(|_| invalid("Invalid annotation ID."))?;
        if !ids.insert(a.id.clone())
            || !["manual", "imported", "model_approved"].contains(&a.source.as_str())
        {
            return Err(invalid("Duplicate ID or invalid annotation source."));
        }
        let layer = profile
            .layers
            .iter()
            .find(|l| l.id == a.layer_id)
            .ok_or_else(|| invalid("Unknown layer."))?;
        if layer.layer_type == "semantic" {
            return Err(invalid(
                "Semantic classes must use their mutually exclusive raster, not vector objects.",
            ));
        }
        match (layer.namespace.as_ref(), a.class_id.as_ref()) {
            (Some(ns), Some(id))
                if profile
                    .classes
                    .iter()
                    .any(|c| &c.namespace == ns && &c.id == id && !c.is_ignore) => {}
            (None, None) => {}
            _ => return Err(invalid("Missing class or wrong class namespace.")),
        }
        if layer.layer_type == "region" && matches!(a.geometry, Geometry::Bbox { .. }) {
            return Err(invalid(
                "Region outlines use polygons; boxes are object instances.",
            ));
        }
        validate_geometry(&a.geometry, record.width, record.height)?;
    }
    let payload = json(&document)?;
    let payload_hash = sha(payload.as_bytes());
    let existing: Option<(String, u32, String)> = session
        .connection
        .query_row(
            "SELECT media_id,revision,payload_sha256 FROM annotation_operations WHERE id=?1",
            [operation_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    if let Some((media, revision, hash)) = existing {
        if media == document.media_id && hash == payload_hash {
            return Ok(revision);
        }
        return Err(invalid("Operation ID reused with different contents."));
    }
    let previous: Option<(u32, String)> = session
        .connection
        .query_row(
            "SELECT revision,state_json FROM annotation_documents WHERE media_id=?1",
            [&document.media_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let head = previous.as_ref().map_or(0, |v| v.0);
    if head != document.revision {
        return Err(AppError::new(
            ErrorCode::Conflict,
            format!(
                "Expected revision {}, found {head}. Reload before editing.",
                document.revision
            ),
        ));
    }
    let mut assets = Vec::new();
    let mut asset_rows = Vec::new();
    let mut mask_layers = HashSet::new();
    for mask in &document.masks {
        let layer = profile
            .layers
            .iter()
            .find(|l| l.id == mask.layer_id && l.layer_type != "instance")
            .ok_or_else(|| invalid("Unknown raster layer."))?;
        if !mask_layers.insert(mask.layer_id.clone())
            || (mask.width, mask.height) != (record.width, record.height)
        {
            return Err(invalid("Duplicate mask layer or wrong dimensions."));
        }
        validate_runs(mask)?;
        if layer.layer_type == "region" {
            if mask.runs.as_chunks::<2>().0.iter().any(|p| p[0] > 1) {
                return Err(invalid("Region masks must be binary (0/1)."));
            }
        } else {
            let mut allowed = [false; 256];
            for class in profile
                .classes
                .iter()
                .filter(|c| Some(&c.namespace) == layer.namespace.as_ref())
            {
                allowed[usize::from(class.export_id)] = true;
            }
            if mask
                .runs
                .as_chunks::<2>()
                .0
                .iter()
                .any(|p| !allowed[p[0] as usize])
            {
                return Err(invalid("Unknown semantic train ID."));
            }
        }
        let hash = ensure_mask_asset(session, mask, &mut asset_rows)?;
        assets.push(StoredMask {
            layer_id: mask.layer_id.clone(),
            asset_id: hash,
            annotation_id: None,
        });
    }
    for shape in &mut document.annotations {
        if let Geometry::Mask {
            width,
            height,
            runs,
            ..
        } = &mut shape.geometry
        {
            let mask = RasterMask {
                layer_id: shape.layer_id.clone(),
                width: *width,
                height: *height,
                runs: runs.clone(),
            };
            let hash = ensure_mask_asset(session, &mask, &mut asset_rows)?;
            assets.push(StoredMask {
                layer_id: shape.layer_id.clone(),
                asset_id: hash,
                annotation_id: Some(shape.id.clone()),
            });
            runs.clear();
        }
    }

    document.revision = head
        .checked_add(1)
        .ok_or_else(|| invalid("Revision overflow."))?;
    let masks = std::mem::take(&mut document.masks);
    let stored = Stored {
        document: document.clone(),
        assets,
    };
    let state = json(&stored)?;
    let tx = session.connection.transaction()?;
    for (hash, path, w, h, source_hash) in asset_rows {
        tx.execute("INSERT OR IGNORE INTO mask_assets(id,relative_path,sha256,width,height,encoding,source_sha256) VALUES(?1,?2,?1,?3,?4,'png8',?5)",params![hash,path,w,h,source_hash])?;
    }
    tx.execute(
        "DELETE FROM annotations WHERE media_id=?1",
        [&document.media_id],
    )?;
    for a in &document.annotations {
        let asset = stored
            .assets
            .iter()
            .find(|asset| asset.annotation_id.as_deref() == Some(&a.id))
            .map(|asset| &asset.asset_id);
        tx.execute("INSERT INTO annotations(id,media_id,layer_id,class_id,geometry_type,geometry_json,source,review_status,revision,mask_asset_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",params![a.id,document.media_id,a.layer_id,a.class_id,match a.geometry{Geometry::Bbox{..}=>"bbox",Geometry::Polygon{..}=>"polygon",Geometry::MultiPolygon{..}=>"multiPolygon",Geometry::Mask{..}=>"mask"},json(&a.geometry)?,a.source,document.review_status,document.revision,asset])?;
    }
    for (index, asset) in stored.assets.iter().enumerate() {
        if asset.annotation_id.is_some() {
            continue;
        }
        let layer = profile
            .layers
            .iter()
            .find(|l| l.id == asset.layer_id)
            .unwrap();
        let model_layer: bool = if let Some((table, id)) = suggestion_id {
            if table == "ai_suggestions" {
                tx.query_row("SELECT json_extract(prediction_json,'$.model.layerId')=?2 FROM ai_suggestions WHERE id=?1",params![id,asset.layer_id],|r|r.get(0))?
            } else {
                tx.query_row("SELECT EXISTS(SELECT 1 FROM review_predictions p,json_each(p.prediction_json,'$.masks') m WHERE p.id=?1 AND json_extract(m.value,'$.layerId')=?2)",params![id,asset.layer_id],|r|r.get(0))?
            }
        } else {
            false
        };
        let source = if model_layer {
            "model_approved"
        } else {
            "manual"
        };
        tx.execute("INSERT INTO annotations(id,media_id,layer_id,geometry_type,geometry_json,mask_asset_id,source,review_status,revision) VALUES(?1,?2,?3,?4,'{}',?5,?6,?7,?8)",params![format!("{}-raster-{index}",document.media_id),document.media_id,asset.layer_id,if layer.layer_type=="semantic"{"semantic_layer"}else{"region"},asset.asset_id,source,document.review_status,document.revision])?;
    }
    tx.execute("INSERT INTO annotation_documents(media_id,revision,state_json) VALUES(?1,?2,?3) ON CONFLICT(media_id) DO UPDATE SET revision=excluded.revision,state_json=excluded.state_json,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",params![document.media_id,document.revision,state])?;
    tx.execute(
        "UPDATE media SET review_status=?1,split_group_id=?2 WHERE id=?3",
        params![
            document.review_status,
            document.split_group,
            document.media_id
        ],
    )?;
    tx.execute(
        "DELETE FROM image_events WHERE media_id=?1",
        [&document.media_id],
    )?;
    if let Some(event) = &document.event_label {
        tx.execute("INSERT INTO image_events(id,media_id,event_type,notes,reviewed_at) VALUES(?1,?1,?2,?3,strftime('%Y-%m-%dT%H:%M:%fZ','now'))",params![document.media_id,event,document.notes])?;
    }
    tx.execute("INSERT INTO annotation_operations(id,media_id,operation,before_json,after_json,payload_sha256,revision) VALUES(?1,?2,'replace_document',?3,?4,?5,?6)",params![operation_id,document.media_id,previous.map_or("null".into(),|v|v.1),state,payload_hash,document.revision])?;
    if let Some((table, id)) = suggestion_id {
        let changed = tx.execute(&format!("UPDATE {table} SET status='accepted',accepted_revision=?1,operation_id=?2,reviewed_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?3 AND status='pending' AND media_id=?4 AND base_revision=?5"),params![document.revision,operation_id,id,document.media_id,head])?;
        if changed != 1 {
            return Err(invalid("AI suggestion changed during approval."));
        }
    }
    tx.commit()?;
    drop(masks);
    Ok(document.revision)
}

type AssetRow = (String, String, u32, u32, String);
fn ensure_mask_asset(
    session: &ProjectSession,
    mask: &RasterMask,
    rows: &mut Vec<AssetRow>,
) -> Result<String> {
    let source_hash = sha(json(&(mask.width, mask.height, &mask.runs))?.as_bytes());
    let cached: Option<(String, String)> = session
        .connection
        .query_row(
            "SELECT id,relative_path FROM mask_assets WHERE source_sha256=?1",
            [&source_hash],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((hash, relative)) = cached {
        if relative != format!("masks/{hash}.png") {
            return Err(invalid("Unsafe cached mask path."));
        }
        let path = session.root.join(".labelflow").join(relative);
        crate::security::reject_symlink(&path)?;
        if sha(&fs::read(path)?) != hash {
            return Err(invalid(
                "Referenced mask checksum changed. Restore a complete backup.",
            ));
        }
        return Ok(hash);
    }
    let pixels = unpack(mask)?;
    let bytes = encode_png(mask.width, mask.height, &pixels)?;
    let hash = sha(&bytes);
    let relative = format!("masks/{hash}.png");
    let dir = session.root.join(".labelflow/masks");
    crate::security::reject_symlink(&dir)?;
    fs::create_dir_all(dir)?;
    atomic_file(&session.root.join(".labelflow").join(&relative), &bytes)?;
    rows.push((hash.clone(), relative, mask.width, mask.height, source_hash));
    Ok(hash)
}
