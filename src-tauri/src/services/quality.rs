use crate::{
    domain::{annotation::ExportOptions, Result},
    services::{annotation as a, library, project::ProjectSession},
};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
pub fn group(s: &ProjectSession, id: &str) -> Result<String> {
    Ok(s.connection.query_row("SELECT CASE WHEN COALESCE(v.trip_id,r.trip_id) IS NOT NULL THEN 'trip:'||COALESCE(v.trip_id,r.trip_id) WHEN m.source_video_id IS NOT NULL THEN COALESCE(v.split_group_id,'video:'||m.source_video_id) ELSE COALESCE(NULLIF(m.split_group_id,''),m.content_sha256) END FROM media m LEFT JOIN videos v ON v.id=m.source_video_id LEFT JOIN media_review_metadata r ON r.media_id=m.id WHERE m.id=?1",[id],|r|r.get(0))?)
}
pub fn origin(s: &ProjectSession, id: &str) -> Result<Option<serde_json::Value>> {
    Ok(s.connection.query_row("SELECT m.source_video_id,m.frame_index,m.frame_time_ms,m.frame_pts,v.time_base_num,v.time_base_den FROM media m JOIN videos v ON v.id=m.source_video_id WHERE m.id=?1",[id],|r|Ok(serde_json::json!({"videoId":r.get::<_,String>(0)?,"frameIndex":r.get::<_,u32>(1)?,"timeMs":r.get::<_,f64>(2)?,"pts":r.get::<_,i64>(3)?,"timeBase":[r.get::<_,i64>(4)?,r.get::<_,i64>(5)?]}))).optional()?)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityReport {
    pub images: u32,
    pub groups: Vec<GroupCount>,
    pub review_counts: BTreeMap<String, u32>,
    pub split_counts: BTreeMap<String, u32>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub class_mapping: Vec<crate::domain::annotation::ClassDefinition>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupCount {
    pub group: String,
    pub images: u32,
    pub split: String,
}
pub fn inspect(
    s: &ProjectSession,
    options: &ExportOptions,
    deep: bool,
    cancel: &AtomicBool,
    mut progress: impl FnMut(crate::domain::annotation::Progress),
) -> Result<QualityReport> {
    let records = library::all(s)?;
    let mut groups = BTreeMap::new();
    let mut review_counts = BTreeMap::new();
    let mut split_counts = BTreeMap::new();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut images = 0;
    let mut video_splits = HashMap::new();
    crate::export::split_for(
        "",
        &options.seed,
        options.train_percent,
        options.val_percent,
    )?;
    for (i, record) in records.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(crate::domain::AppError::new(
                crate::domain::ErrorCode::Cancelled,
                "Dataset validation cancelled.",
            ));
        }
        *review_counts
            .entry(record.review_status.clone())
            .or_insert(0) += 1;
        if options.approved_only && record.review_status != "approved" {
            continue;
        }
        images += 1;
        let group = group(s, &record.id)?;
        *groups.entry(group.clone()).or_insert(0) += 1;
        let split = crate::export::split_for(
            &group,
            &options.seed,
            options.train_percent,
            options.val_percent,
        )?;
        *split_counts.entry(split.to_owned()).or_insert(0) += 1;
        if let Some(origin) = origin(s, &record.id)? {
            let video = origin["videoId"].as_str().unwrap().to_owned();
            if video_splits
                .insert(video, split)
                .is_some_and(|old| old != split)
            {
                errors.push("Source video leaks across dataset splits.".into());
            }
        }
        let path = library::resolve(s, &record.path)?;
        if !path.is_file() {
            errors.push(format!(
                "{}: source is missing; use Repair media.",
                record.name
            ));
        }
        if deep {
            let result = (|| -> Result<()> {
                library::checked_load(s, record)?;
                let doc = a::load(s, &record.id)?;
                let profile = crate::services::profile::current(s)?;
                for shape in &doc.annotations {
                    let layer = profile
                        .layers
                        .iter()
                        .find(|l| l.id == shape.layer_id)
                        .ok_or_else(|| a::invalid("Unknown annotation layer."))?;
                    if layer.layer_type == "instance"
                        && !profile.classes.iter().any(|c| {
                            Some(&c.id) == shape.class_id.as_ref()
                                && Some(&c.namespace) == layer.namespace.as_ref()
                                && !c.is_ignore
                        })
                    {
                        return Err(a::invalid("Missing or unknown instance class."));
                    }
                    if options.format == "yoloSeg" && layer.layer_type == "instance" {
                        crate::export::instances::yolo_seg_row(
                            &shape.geometry,
                            record.width,
                            record.height,
                            0,
                        )
                        .or_else(|error| {
                            if options.skip_unsupported {
                                Ok(error.to_string())
                            } else {
                                Err(error)
                            }
                        })?;
                    }
                    a::validate_geometry(&shape.geometry, record.width, record.height)?;
                }
                Ok(())
            })();
            if let Err(e) = result {
                errors.push(format!("{}: {e}", record.name));
            }
        }
        progress(crate::domain::annotation::Progress {
            completed: i as u32 + 1,
            total: records.len() as u32,
            message: record.name.clone(),
        });
    }
    for (split, percent) in [
        ("train", options.train_percent),
        ("val", options.val_percent),
        ("test", 100 - options.train_percent - options.val_percent),
    ] {
        if percent > 0 && split_counts.get(split).copied().unwrap_or(0) == 0 {
            warnings.push(format!("{split} is empty; grouped splitting never divides a video/trip. Adjust seed/ratios or add groups."));
        }
    }
    let groups = groups
        .into_iter()
        .map(|(group, images)| {
            Ok(GroupCount {
                split: crate::export::split_for(
                    &group,
                    &options.seed,
                    options.train_percent,
                    options.val_percent,
                )?
                .into(),
                group,
                images,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(QualityReport {
        images,
        groups,
        review_counts,
        split_counts,
        errors,
        warnings,
        class_mapping: crate::services::profile::current(s)?.classes,
    })
}
pub fn repair(s: &mut ProjectSession, id: &str, path: &Path) -> Result<()> {
    let record = library::record(s, id)?;
    let path = path.canonicalize()?;
    let (info, _) = crate::services::media::load(&path)?;
    if (info.sha256, info.width, info.height, info.orientation)
        != (
            record.sha256,
            record.width,
            record.height,
            record.orientation,
        )
    {
        return Err(a::invalid("Replacement must have exactly the original SHA-256, canonical dimensions and orientation. Annotations were preserved."));
    }
    let stored = path
        .strip_prefix(&s.root)
        .unwrap_or(&path)
        .to_string_lossy()
        .into_owned();
    s.connection.execute(
        "UPDATE media SET path=?1,import_mode='linked' WHERE id=?2",
        params![stored, id],
    )?;
    Ok(())
}
