use crate::{
    domain::{
        advanced::*,
        annotation::*,
        review::{Prediction, PredictionModel},
        AppError, ErrorCode, Result,
    },
    services::{
        annotation as a, library,
        project::ProjectSession,
        review::{predictions, queue},
    },
};
use image::{imageops::FilterType, DynamicImage, GenericImageView, GrayImage};
use rusqlite::{params, OptionalExtension};
use std::{collections::HashSet, sync::atomic::AtomicBool};

const MODEL: &str = "labelflow.translation-v1";
const WINDOW_MS: f64 = 2000.;
fn frame(s: &ProjectSession, id: &str) -> Result<(String, f64)> {
    s.connection.query_row("SELECT source_video_id,frame_time_ms FROM media WHERE id=?1 AND source_video_id IS NOT NULL",[id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|_|a::invalid("Propagation requires extracted frames from the same video."))
}
fn pair(s: &ProjectSession, anchor: &str, target: &str) -> Result<()> {
    let (a, t) = frame(s, anchor)?;
    let (b, u) = frame(s, target)?;
    let x = library::record(s, anchor)?;
    let y = library::record(s, target)?;
    if anchor == target
        || a != b
        || !t.is_finite()
        || !u.is_finite()
        || (t - u).abs() > WINDOW_MS
        || (x.width, x.height) != (y.width, y.height)
    {
        return Err(a::invalid("Targets must be different, same-size frames in the same video within 2 seconds of the anchor."));
    }
    Ok(())
}
pub fn targets(s: &ProjectSession, anchor: &str) -> Result<Vec<FrameCandidate>> {
    let (video, time) = frame(s, anchor)?;
    let record = library::record(s, anchor)?;
    let mut q=s.connection.prepare("SELECT id,path,frame_time_ms,review_status FROM media WHERE source_video_id=?1 AND id<>?2 AND width=?3 AND height=?4 AND ABS(frame_time_ms-?5)<=2000 ORDER BY ABS(frame_time_ms-?5),frame_time_ms,id LIMIT 40")?;
    let result = q
        .query_map(
            params![video, anchor, record.width, record.height, time],
            |r| {
                let path: String = r.get(1)?;
                let t: f64 = r.get(2)?;
                Ok(FrameCandidate {
                    media_id: r.get(0)?,
                    name: std::path::Path::new(&path)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into(),
                    time_ms: t,
                    delta_ms: t - time,
                    review_status: r.get(3)?,
                })
            },
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(result)
}
fn gray(img: &DynamicImage, factor: u32) -> GrayImage {
    img.resize_exact(
        img.width() / factor,
        img.height() / factor,
        FilterType::Triangle,
    )
    .to_luma8()
}
// Global translation, bounded exhaustive search on a small canonical grayscale image.
// Error uses the same interior window for every offset, avoiding overlap-size bias.
pub fn register(
    anchor: &DynamicImage,
    target: &DynamicImage,
    cancel: &AtomicBool,
) -> Result<(i32, i32, f64, f64)> {
    if anchor.dimensions() != target.dimensions() {
        return Err(a::invalid("Registration dimensions differ."));
    }
    let factor = anchor.width().max(anchor.height()).div_ceil(160).max(1);
    let a = gray(anchor, factor);
    let b = gray(target, factor);
    let w = a.width() as i32;
    let h = a.height() as i32;
    if w < 32 || h < 32 {
        return Err(a::invalid(
            "Registration needs a preview of at least 32 pixels per axis.",
        ));
    }
    let radius = 12.min(w / 8).min(h / 8);
    let mut sum = 0.;
    let mut sq = 0.;
    let mut count = 0.;
    for y in (radius..h - radius).step_by(2) {
        for x in (radius..w - radius).step_by(2) {
            let v = f64::from(a[(x as u32, y as u32)][0]);
            sum += v;
            sq += v * v;
            count += 1.;
        }
    }
    if sq / count - (sum / count).powi(2) < 100. {
        return Err(a::invalid(
            "Anchor has too little texture for reliable translation.",
        ));
    }
    let mut scores = Vec::new();
    for dy in -radius..=radius {
        queue::check(cancel)?;
        for dx in -radius..=radius {
            let mut error = 0u64;
            for y in (radius..h - radius).step_by(2) {
                for x in (radius..w - radius).step_by(2) {
                    error += u64::from(
                        a[(x as u32, y as u32)][0]
                            .abs_diff(b[((x + dx) as u32, (y + dy) as u32)][0]),
                    );
                }
            }
            scores.push((error as f64 / count, dx, dy));
        }
    }
    scores.sort_by(|a, b| {
        a.0.total_cmp(&b.0)
            .then((a.1.abs() + a.2.abs()).cmp(&(b.1.abs() + b.2.abs())))
            .then(a.1.cmp(&b.1))
            .then(a.2.cmp(&b.2))
    });
    let (error, dx, dy) = scores[0];
    let second = scores
        .iter()
        .find(|(_, x, y)| (x - dx).abs() > 1 || (y - dy).abs() > 1)
        .map(|v| v.0)
        .unwrap_or(error);
    let margin = (second - error).max(0.);
    if error > 32. || margin < 0.5 || dx.abs() == radius || dy.abs() == radius {
        return Err(a::invalid("Translation is ambiguous, beyond search bounds or photometrically inconsistent; label this frame manually."));
    }
    Ok((dx * factor as i32, dy * factor as i32, error, margin))
}
pub fn shift_mask(mask: &RasterMask, dx: i32, dy: i32, fill: u8) -> Result<RasterMask> {
    let old = a::unpack(mask)?;
    let (w, h) = (mask.width as i64, mask.height as i64);
    let mut next = vec![fill; old.len()];
    for y in 0..h {
        let yy = y + i64::from(dy);
        if yy < 0 || yy >= h {
            continue;
        }
        let left = 0.max(-i64::from(dx));
        let right = w.min(w - i64::from(dx));
        if left < right {
            let from = (y * w + left) as usize;
            let to = (yy * w + left + i64::from(dx)) as usize;
            next[to..to + (right - left) as usize]
                .copy_from_slice(&old[from..from + (right - left) as usize]);
        }
    }
    Ok(a::pack(
        mask.layer_id.clone(),
        mask.width,
        mask.height,
        &next,
    ))
}
fn shift_shape(g: &Geometry, dx: i32, dy: i32, w: u32, h: u32) -> Result<Option<Geometry>> {
    let translate = |points: &[Point]| {
        points
            .iter()
            .map(|p| Point {
                x: p.x + f64::from(dx),
                y: p.y + f64::from(dy),
            })
            .collect::<Vec<_>>()
    };
    let shifted = match g {
        Geometry::Bbox {
            x,
            y,
            width,
            height,
        } => {
            let left = (x + f64::from(dx)).max(0.);
            let top = (y + f64::from(dy)).max(0.);
            let right = (x + width + f64::from(dx)).min(f64::from(w));
            let bottom = (y + height + f64::from(dy)).min(f64::from(h));
            if right <= left || bottom <= top {
                return Ok(None);
            }
            Geometry::Bbox {
                x: left,
                y: top,
                width: right - left,
                height: bottom - top,
            }
        }
        Geometry::Polygon { points, holes } => Geometry::Polygon {
            points: translate(points),
            holes: holes.iter().map(|h| translate(h)).collect(),
        },
        Geometry::MultiPolygon { polygons } => Geometry::MultiPolygon {
            polygons: polygons
                .iter()
                .map(|p| PolygonRings {
                    points: translate(&p.points),
                    holes: p.holes.iter().map(|h| translate(h)).collect(),
                })
                .collect(),
        },
        Geometry::Mask {
            width,
            height,
            runs,
            iscrowd,
        } => {
            let shifted = shift_mask(
                &RasterMask {
                    layer_id: String::new(),
                    width: *width,
                    height: *height,
                    runs: runs.clone(),
                },
                dx,
                dy,
                0,
            )?;
            Geometry::Mask {
                width: *width,
                height: *height,
                runs: shifted.runs,
                iscrowd: *iscrowd,
            }
        }
    };
    if a::validate_geometry(&shifted, w, h).is_err() {
        Ok(None)
    } else {
        Ok(Some(shifted))
    }
}
pub fn validate_import(s: &ProjectSession, p: &Prediction) -> Result<()> {
    let Some(o) = &p.propagation else {
        if p.model.id == MODEL {
            return Err(a::invalid(
                "Translation predictions require anchor provenance.",
            ));
        }
        return Ok(());
    };
    pair(s, &o.anchor_id, &p.media_id)?;
    let anchor = a::load(s, &o.anchor_id)?;
    let record = library::record(s, &o.anchor_id)?;
    if p.model.id != MODEL
        || p.model.version != "1"
        || p.event_label.is_some()
        || uuid::Uuid::parse_str(&o.track_id).is_err()
        || anchor.revision != o.anchor_revision
        || anchor.review_status != "approved"
        || record.sha256 != o.anchor_sha256
        || !o.mean_error.is_finite()
        || !(0. ..=32.).contains(&o.mean_error)
        || !o.match_margin.is_finite()
        || o.match_margin < 0.5
        || i64::from(o.dx).abs() > i64::from(p.width) / 4
        || i64::from(o.dy).abs() > i64::from(p.height) / 4
    {
        return Err(a::invalid(
            "Propagation provenance is stale or invalid; regenerate from an approved anchor.",
        ));
    }
    Ok(())
}
pub fn info(s: &ProjectSession, key: &str) -> Result<Option<PropagationInfo>> {
    let (kind, id) = predictions::identity(key)?;
    if kind != "review" {
        return Ok(None);
    }
    Ok(s.connection.query_row("SELECT track_id,anchor_id,anchor_revision,target_id,dx,dy,mean_error,match_margin,skipped_shapes,verified_revision,verified_at FROM propagation_links WHERE prediction_id=?1",[id],|r|Ok(PropagationInfo{key:key.into(),track_id:r.get(0)?,anchor_id:r.get(1)?,anchor_revision:r.get(2)?,target_id:r.get(3)?,dx:r.get(4)?,dy:r.get(5)?,mean_error:r.get(6)?,match_margin:r.get(7)?,skipped_shapes:r.get(8)?,verified_revision:r.get(9)?,verified_at:r.get(10)?})).optional()?)
}
fn current(s: &ProjectSession, key: &str, revision: u32) -> Result<PropagationInfo> {
    let info = info(s, key)?
        .ok_or_else(|| a::invalid("This prediction is not a propagation suggestion."))?;
    let (summary, p) = predictions::load(s, key)?;
    validate_import(s, &p)?;
    let target = a::load(s, &info.target_id)?;
    if summary.status != "pending"
        || target.review_status == "approved"
        || summary.base_revision != revision
        || target.revision != revision
    {
        return Err(AppError::new(
            ErrorCode::Conflict,
            "Propagation target changed or is approved; regenerate for its current revision.",
        ));
    }
    for id in [&info.anchor_id, &info.target_id] {
        let r = library::record(s, id)?;
        if crate::video::source_sha(&library::resolve(s, &r.path)?)? != r.sha256 {
            return Err(a::invalid("Frame source changed; regenerate propagation."));
        }
    }
    Ok(info)
}
pub fn verify(
    s: &ProjectSession,
    key: &str,
    revision: u32,
    confirmed: bool,
) -> Result<PropagationInfo> {
    if !confirmed {
        return Err(a::invalid(
            "Visually inspect the overlay and explicitly confirm verification first.",
        ));
    }
    current(s, key, revision)?;
    let (_, id) = predictions::identity(key)?;
    s.connection.execute("UPDATE propagation_links SET verified_revision=?1,verified_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE prediction_id=?2",params![revision,id])?;
    info(s, key)?.ok_or_else(|| a::invalid("Propagation audit is missing."))
}
pub fn check_approval(s: &ProjectSession, key: &str, revision: u32) -> Result<()> {
    let info = current(s, key, revision)?;
    if info.verified_revision != Some(revision) || info.verified_at.is_none() {
        return Err(a::invalid(
            "Visually verify this propagation suggestion before accepting it.",
        ));
    }
    Ok(())
}
pub fn generate(
    s: &ProjectSession,
    anchor_id: &str,
    target_ids: &[String],
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<PropagationReport> {
    if target_ids.is_empty()
        || target_ids.len() > 10
        || target_ids.iter().collect::<HashSet<_>>().len() != target_ids.len()
    {
        return Err(a::invalid("Choose 1–10 distinct target frames."));
    }
    let source = a::load(s, anchor_id)?;
    if source.review_status != "approved"
        || (source.annotations.is_empty() && source.masks.is_empty())
    {
        return Err(a::invalid("Approve an anchor with spatial labels before propagation; event-only labels cannot be propagated."));
    }
    for target in target_ids {
        pair(s, anchor_id, target)?;
    }
    let record = library::record(s, anchor_id)?;
    queue::check(cancel)?;
    let (_, image) = library::checked_load(s, &record)?;
    let profile = crate::services::profile::current(s)?;
    let job = crate::jobs::start(
        &s.connection,
        "propagate_translation",
        &a::json(&(anchor_id, target_ids))?,
    )?;
    let track = uuid::Uuid::new_v4().to_string();
    let result = (|| {
        let mut report = PropagationReport {
            created: vec![],
            skipped: vec![],
        };
        for (i, target_id) in target_ids.iter().enumerate() {
            queue::check(cancel)?;
            let result = (|| -> Result<PropagationInfo> {
                let r = library::record(s, target_id)?;
                if r.review_status == "approved" {
                    return Err(a::invalid("Target already approved."));
                }
                let (_, target) = library::checked_load(s, &r)?;
                let (dx, dy, error, margin) = register(&image, &target, cancel)?;
                let mut masks = Vec::new();
                for m in &source.masks {
                    let semantic = profile
                        .layers
                        .iter()
                        .any(|l| l.id == m.layer_id && l.layer_type == "semantic");
                    masks.push(shift_mask(m, dx, dy, if semantic { 255 } else { 0 })?);
                }
                let mut annotations = Vec::new();
                let mut skipped = 0;
                for a in &source.annotations {
                    if let Some(g) = shift_shape(&a.geometry, dx, dy, r.width, r.height)? {
                        let mut a = a.clone();
                        a.geometry = g;
                        a.source = "model_suggestion".into();
                        annotations.push(a);
                    } else {
                        skipped += 1;
                    }
                }
                if annotations.is_empty() && masks.is_empty() {
                    return Err(a::invalid(
                        "All propagated shapes fell outside target bounds.",
                    ));
                }
                let p = Prediction {
                    schema_version: 1,
                    profile_id: s.project.profile_id.clone(),
                    media_id: r.id,
                    source_sha256: r.sha256,
                    width: r.width,
                    height: r.height,
                    model: PredictionModel {
                        id: MODEL.into(),
                        name: "Translation propagation (experimental)".into(),
                        version: "1".into(),
                        sha256: None,
                        confidence: None,
                    },
                    event_label: None,
                    annotations,
                    masks,
                    propagation: Some(PropagationOrigin {
                        track_id: track.clone(),
                        anchor_id: anchor_id.into(),
                        anchor_revision: source.revision,
                        anchor_sha256: record.sha256.clone(),
                        dx,
                        dy,
                        mean_error: error,
                        match_margin: margin,
                        skipped_shapes: skipped,
                    }),
                };
                queue::check(cancel)?;
                let key = predictions::import(s, p)?;
                info(s, &key)?.ok_or_else(|| a::invalid("Missing propagation audit."))
            })();
            match result {
                Ok(value) => report.created.push(value),
                Err(e) if e.code == ErrorCode::Cancelled => return Err(e),
                Err(e) => report.skipped.push(format!("{target_id}: {e}")),
            }
            crate::jobs::progress(&s.connection, &job, i as u32 + 1, target_ids.len() as u32);
            progress(Progress {
                completed: i as u32 + 1,
                total: target_ids.len() as u32,
                message: target_id.clone(),
            });
        }
        Ok(report)
    })();
    queue::finish(s, &job, &result);
    result
}
