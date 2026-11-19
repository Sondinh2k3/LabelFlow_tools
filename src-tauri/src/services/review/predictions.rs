use crate::{
    domain::{annotation::*, review::*, AppError, ErrorCode, Result},
    services::{annotation as a, library, profile, project::ProjectSession},
};
use rusqlite::params;
use std::collections::HashSet;
use std::io::Read;

pub fn import_file(
    s: &ProjectSession,
    path: &std::path::Path,
    expected_media: &str,
) -> Result<String> {
    let file = std::fs::File::open(path)?;
    const LIMIT: u64 = 32 * 1024 * 1024;
    if file.metadata()?.len() > LIMIT {
        return Err(a::invalid("Review prediction exceeds 32 MiB limit."));
    }
    let mut data = Vec::new();
    file.take(LIMIT + 1).read_to_end(&mut data)?;
    if data.len() as u64 > LIMIT {
        return Err(a::invalid("Review prediction exceeds 32 MiB limit."));
    }
    let bytes = data.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&data);
    let prediction: Prediction =
        serde_json::from_slice(bytes).map_err(|e| a::invalid(e.to_string()))?;
    if prediction.media_id != expected_media {
        return Err(a::invalid(
            "Prediction belongs to another image. Select its matching image first.",
        ));
    }
    import(s, prediction)
}

pub fn validate(s: &ProjectSession, p: &Prediction) -> Result<()> {
    let record = library::record(s, &p.media_id)?;
    if p.schema_version != 1
        || p.profile_id != s.project.profile_id
        || p.source_sha256 != record.sha256
        || (p.width, p.height) != (record.width, record.height)
        || p.model.id.is_empty()
        || p.model.id.len() > 256
        || p.model.name.is_empty()
        || p.model.name.len() > 256
        || p.model.version.is_empty()
        || p.model.version.len() > 256
        || p.model
            .sha256
            .as_ref()
            .is_some_and(|s| s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
        || p.model
            .confidence
            .is_some_and(|v| !v.is_finite() || !(0. ..=1.).contains(&v))
        || p.event_label
            .as_ref()
            .is_some_and(|e| !["positive", "negative", "uncertain"].contains(&e.as_str()))
        || p.annotations.len() > 10_000
    {
        return Err(a::invalid(
            "Prediction identity, profile, dimensions or model metadata mismatch.",
        ));
    }
    let profile = profile::current(s)?;
    let mut layers = HashSet::new();
    for mask in &p.masks {
        let layer = profile
            .layers
            .iter()
            .find(|l| l.id == mask.layer_id && l.layer_type != "instance")
            .ok_or_else(|| a::invalid("Prediction targets an unknown raster layer."))?;
        if !layers.insert(&mask.layer_id)
            || (mask.width, mask.height) != (p.width, p.height)
            || mask.runs.len() > 1_000_000
        {
            return Err(a::invalid(
                "Prediction raster dimensions, duplicate layer or run budget invalid.",
            ));
        }
        let pixels = a::unpack(mask)?;
        if layer.layer_type == "region" {
            if pixels.iter().any(|v| *v > 1) {
                return Err(a::invalid(
                    "Region predictions must be independent binary masks.",
                ));
            }
        } else if pixels.iter().any(|v| {
            !profile
                .classes
                .iter()
                .any(|c| Some(&c.namespace) == layer.namespace.as_ref() && c.export_id == *v)
        }) {
            return Err(a::invalid("Unmapped semantic train ID in prediction."));
        }
    }
    let mut ids = HashSet::new();
    for shape in &p.annotations {
        uuid::Uuid::parse_str(&shape.id)
            .map_err(|_| a::invalid("Invalid prediction object ID."))?;
        if !ids.insert(&shape.id) {
            return Err(a::invalid("Duplicate prediction object ID."));
        }
        let layer = profile
            .layers
            .iter()
            .find(|l| l.id == shape.layer_id && l.layer_type != "semantic")
            .ok_or_else(|| a::invalid("Prediction object layer invalid."))?;
        match (&layer.namespace, &shape.class_id) {
            (None, None) => {}
            (Some(ns), Some(id))
                if profile
                    .classes
                    .iter()
                    .any(|c| &c.namespace == ns && &c.id == id && !c.is_ignore) => {}
            _ => return Err(a::invalid("Prediction object class namespace mismatch.")),
        }
        if layer.layer_type == "region" && matches!(shape.geometry, Geometry::Bbox { .. }) {
            return Err(a::invalid(
                "Region predictions use polygon/binary masks, not object boxes.",
            ));
        }
        a::validate_geometry(&shape.geometry, p.width, p.height)?;
    }
    Ok(())
}
pub fn import(s: &ProjectSession, mut p: Prediction) -> Result<String> {
    validate(s, &p)?;
    crate::services::advanced::propagation::validate_import(s, &p)?;
    for shape in &mut p.annotations {
        shape.source = "model_suggestion".into();
    }
    let value = a::json(&p)?;
    if value.len() > 32 * 1024 * 1024 {
        return Err(a::invalid("Prediction exceeds 32 MiB budget."));
    }
    let revision = a::load(s, &p.media_id)?.revision;
    let id = uuid::Uuid::new_v4().to_string();
    let tx = s.connection.unchecked_transaction()?;
    s.connection.execute("INSERT INTO review_predictions(id,media_id,base_revision,source_sha256,prediction_json,model_name,model_version,event_label) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![id,p.media_id,revision,p.source_sha256,value,p.model.name,p.model.version,p.event_label])?;
    if let Some(o) = &p.propagation {
        s.connection.execute("INSERT INTO propagation_links(prediction_id,track_id,anchor_id,anchor_revision,anchor_sha256,target_id,dx,dy,mean_error,match_margin,skipped_shapes) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",params![id,o.track_id,o.anchor_id,o.anchor_revision,o.anchor_sha256,p.media_id,o.dx,o.dy,o.mean_error,o.match_margin,o.skipped_shapes])?;
    }
    tx.commit()?;
    Ok(format!("review:{id}"))
}
pub fn summaries(s: &ProjectSession, media: &str) -> Result<Vec<PredictionSummary>> {
    library::record(s, media)?;
    let mut q=s.connection.prepare("SELECT key,media_id,base_revision,status,model_name,model_version,event_label,created_at FROM review_prediction_index WHERE media_id=?1 ORDER BY created_at DESC,key DESC LIMIT 100")?;
    let rows = q
        .query_map([media], summary_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}
fn summary_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<PredictionSummary> {
    Ok(PredictionSummary {
        key: r.get(0)?,
        media_id: r.get(1)?,
        base_revision: r.get(2)?,
        status: r.get(3)?,
        model_name: r.get(4)?,
        model_version: r.get(5)?,
        event_label: r.get(6)?,
        created_at: r.get(7)?,
    })
}
pub fn identity(key: &str) -> Result<(&str, &str)> {
    let (kind, id) = key
        .split_once(':')
        .ok_or_else(|| a::invalid("Invalid prediction key."))?;
    if !["ai", "review"].contains(&kind) || uuid::Uuid::parse_str(id).is_err() {
        return Err(a::invalid("Invalid prediction key."));
    }
    Ok((kind, id))
}
pub fn load(s: &ProjectSession, key: &str) -> Result<(PredictionSummary, Prediction)> {
    let (kind, id) = identity(key)?;
    let summary=s.connection.query_row("SELECT key,media_id,base_revision,status,model_name,model_version,event_label,created_at FROM review_prediction_index WHERE key=?1",[key],summary_row)?;
    let prediction = if kind == "review" {
        let value: String = s.connection.query_row(
            "SELECT prediction_json FROM review_predictions WHERE id=?1",
            [id],
            |r| r.get(0),
        )?;
        a::parse::<Prediction>(&value)?
    } else {
        let preview = crate::ai::suggestions::preview(s, id)?;
        let record = library::record(s, &summary.media_id)?;
        Prediction {
            schema_version: 1,
            profile_id: s.project.profile_id.clone(),
            media_id: summary.media_id.clone(),
            source_sha256: record.sha256,
            width: record.width,
            height: record.height,
            model: PredictionModel {
                id: preview.prediction.model.id,
                name: preview.prediction.model.name,
                version: preview.prediction.model.version,
                sha256: Some(preview.prediction.model.sha256),
                confidence: preview.prediction.confidence,
            },
            event_label: None,
            annotations: preview.annotations,
            masks: preview.masks,
            propagation: None,
        }
    };
    validate(s, &prediction)?;
    Ok((summary, prediction))
}
pub fn snapshot(s: &ProjectSession, key: &str) -> Result<Snapshot> {
    let (summary, prediction) = load(s, key)?;
    let ground_truth = a::load(s, &summary.media_id)?;
    let comparison = super::metrics::compare(&ground_truth, &prediction, true)?;
    Ok(Snapshot {
        summary,
        prediction,
        ground_truth,
        comparison,
    })
}
pub fn reject(s: &ProjectSession, key: &str) -> Result<()> {
    let (kind, id) = identity(key)?;
    if kind == "ai" {
        return crate::ai::suggestions::reject(s, id);
    }
    let (summary, _) = load(s, key)?;
    if summary.status == "rejected" {
        return Ok(());
    }
    if summary.status != "pending" {
        return Err(a::invalid("Only pending predictions can be rejected."));
    }
    s.connection.execute("UPDATE review_predictions SET status='rejected',reviewed_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1 AND status='pending'",[id])?;
    Ok(())
}
pub fn approve(s: &mut ProjectSession, key: &str, revision: u32) -> Result<Document> {
    let (kind, id) = identity(key)?;
    if kind == "ai" {
        return crate::ai::suggestions::approve(s, id, revision);
    }
    let (summary, p) = load(s, key)?;
    if p.propagation.is_some() {
        crate::services::advanced::propagation::check_approval(s, key, revision)?;
    }
    let mut doc = a::load(s, &summary.media_id)?;
    if summary.status != "pending" {
        return Err(a::invalid("Prediction has already been reviewed."));
    }
    if doc.revision != revision || revision != summary.base_revision {
        return Err(AppError::new(
            ErrorCode::Conflict,
            "Ground truth changed after prediction import. Reimport for the current revision.",
        ));
    }
    if doc.review_status == "approved" {
        return Err(a::invalid(
            "Approved ground truth is protected; reopen review and reimport first.",
        ));
    }
    let record = library::record(s, &doc.media_id)?;
    if crate::video::source_sha(&library::resolve(s, &record.path)?)? != p.source_sha256 {
        return Err(a::invalid(
            "Original source changed after prediction import.",
        ));
    }
    let profile = profile::current(s)?;
    for mut shape in p.annotations {
        shape.id = uuid::Uuid::new_v4().to_string();
        shape.source = "model_approved".into();
        doc.annotations.push(shape);
    }
    for mask in p.masks {
        if let Some(current) = doc.masks.iter_mut().find(|m| m.layer_id == mask.layer_id) {
            let mut next = a::unpack(&mask)?;
            let old = a::unpack(current)?;
            let semantic = profile
                .layers
                .iter()
                .any(|l| l.id == mask.layer_id && l.layer_type == "semantic");
            for (new, old) in next.iter_mut().zip(old) {
                if semantic {
                    if old != 255 {
                        *new = old;
                    }
                } else {
                    *new |= old;
                }
            }
            *current = a::pack(mask.layer_id, mask.width, mask.height, &next);
        } else {
            doc.masks.push(mask);
        }
    }
    if doc.event_label.is_none() {
        doc.event_label = p.event_label;
    }
    doc.review_status = "needs_review".into();
    let media = doc.media_id.clone();
    a::save_review_prediction(s, doc, &uuid::Uuid::new_v4().to_string(), id)?;
    a::load(s, &media)
}
