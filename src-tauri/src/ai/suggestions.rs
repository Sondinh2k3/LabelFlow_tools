use super::protocol::{Model, Prediction};
use crate::{
    domain::{annotation::*, AppError, ErrorCode, Result},
    services::{annotation as a, library, profile, project::ProjectSession},
};
use rusqlite::params;
use serde::Serialize;
use std::collections::HashSet;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub media_id: String,
    pub base_revision: u32,
    pub status: String,
    pub model_name: String,
    pub model_version: String,
    pub confidence: Option<f64>,
    pub created_at: String,
    pub accepted_revision: Option<u32>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub prediction: Prediction,
    pub annotations: Vec<Annotation>,
    pub masks: Vec<RasterMask>,
}
pub fn validate_model(s: &ProjectSession, model: &Model) -> Result<()> {
    let p = profile::current(s)?;
    let layer = p
        .layers
        .iter()
        .find(|l| l.id == model.layer_id)
        .ok_or_else(|| a::invalid("AI model targets an unknown layer."))?;
    let semantic = model.task == "semantic"
        && ["onnx_semantic", "pidnet_semantic"].contains(&model.adapter.as_str());
    let boxes = model.task == "boxes" && model.adapter == "onnx_boxes";
    if model.schema_version != 1
        || model.id.is_empty()
        || model.name.is_empty()
        || model.version.is_empty()
        || model.license.is_empty()
        || model.sha256.len() != 64
        || !model.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        || !(semantic && layer.layer_type == "semantic" || boxes && layer.layer_type == "instance")
        || model.class_map.is_empty()
        || model.class_map.len() > 255
    {
        return Err(a::invalid(
            "Unsupported AI model identity, task, or target layer.",
        ));
    }
    let mut ids = HashSet::new();
    for id in &model.class_map {
        if !ids.insert(id)
            || !p.classes.iter().any(|c| {
                &c.id == id && Some(&c.namespace) == layer.namespace.as_ref() && !c.is_ignore
            })
        {
            return Err(a::invalid(
                "AI class map disagrees with the project's stable class IDs / namespace.",
            ));
        }
    }
    // A PIDNet head has one logit channel per project semantic class, in export-ID order
    // (19 Cityscapes channels by default, or the project's own class set).
    if model.adapter == "pidnet_semantic"
        && (model.class_map
            != p.classes
                .iter()
                .filter(|c| Some(&c.namespace) == layer.namespace.as_ref() && !c.is_ignore)
                .map(|c| c.id.clone())
                .collect::<Vec<_>>()
            || model.output["format"] != "logits_nchw")
    {
        return Err(a::invalid("Invalid PIDNet output contract."));
    }
    Ok(())
}
pub fn validate(
    s: &ProjectSession,
    media: &str,
    prediction: &Prediction,
    loaded: &Model,
) -> Result<()> {
    validate_model(s, loaded)?;
    let record = library::record(s, media)?;
    if prediction.model != *loaded
        || prediction.class_map != loaded.class_map
        || (prediction.width, prediction.height) != (record.width, record.height)
        || prediction.orientation != 1
        || prediction.coordinate_convention != "canonical-pixel-xyxy"
        || prediction.processing.provider != "CPUExecutionProvider"
        || prediction.processing.device != "cpu"
        || !prediction.processing.milliseconds.is_finite()
        || prediction.processing.milliseconds < 0.
        || prediction
            .confidence
            .is_some_and(|v| !v.is_finite() || !(0. ..=1.).contains(&v))
        || prediction.boxes.len() > 10_000
    {
        return Err(a::invalid("AI response model, dimensions, class map, coordinates or processing metadata mismatch."));
    }
    match loaded.task.as_str() {
        "boxes" if prediction.mask.is_none() => {
            for b in &prediction.boxes {
                if b.class_index >= loaded.class_map.len()
                    || !b.confidence.is_finite()
                    || !(0. ..=1.).contains(&b.confidence)
                {
                    return Err(a::invalid("Invalid AI detection class or confidence."));
                }
                a::validate_geometry(
                    &Geometry::Bbox {
                        x: b.x,
                        y: b.y,
                        width: b.width,
                        height: b.height,
                    },
                    record.width,
                    record.height,
                )?;
            }
        }
        "semantic" if prediction.boxes.is_empty() => {
            let runs = prediction
                .mask
                .as_ref()
                .ok_or_else(|| a::invalid("AI response has no semantic raster."))?;
            if runs.len() > 1_000_000 {
                return Err(a::invalid("AI raster exceeds RLE budget."));
            }
            let pixels = a::unpack(&RasterMask {
                layer_id: loaded.layer_id.clone(),
                width: record.width,
                height: record.height,
                runs: runs.clone(),
            })?;
            if pixels.iter().any(|&p| p as usize >= loaded.class_map.len()) {
                return Err(a::invalid("AI raster contains an unmapped class."));
            }
        }
        _ => {
            return Err(a::invalid(
                "AI response geometry disagrees with selected model task.",
            ))
        }
    }
    Ok(())
}
pub fn store(
    s: &ProjectSession,
    media: &str,
    base_revision: u32,
    source_sha: &str,
    prediction: Prediction,
    loaded: &Model,
) -> Result<String> {
    validate(s, media, &prediction, loaded)?;
    let record = library::record(s, media)?;
    if record.sha256 != source_sha {
        return Err(a::invalid("Source identity changed during AI inference."));
    }
    if base_revision > a::load(s, media)?.revision {
        return Err(a::invalid(
            "AI prediction references a future document revision.",
        ));
    }
    let count: u32 = s.connection.query_row(
        "SELECT count(*) FROM ai_suggestions WHERE media_id=?1 AND status='pending'",
        [media],
        |r| r.get(0),
    )?;
    if count >= 10 {
        return Err(a::invalid(
            "Review the ten pending suggestions before generating more for this image.",
        ));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let value = a::json(&prediction)?;
    if value.len() > super::protocol::MAX_MESSAGE {
        return Err(a::invalid("Suggestion exceeds storage budget."));
    }
    s.connection.execute("INSERT INTO ai_suggestions(id,media_id,base_revision,source_sha256,prediction_json,model_name,model_version,confidence) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![id,media,base_revision,source_sha,value,loaded.name,loaded.version,prediction.confidence])?;
    Ok(id)
}
pub fn list(s: &ProjectSession, media: &str) -> Result<Vec<Summary>> {
    library::record(s, media)?;
    let mut statement=s.connection.prepare("SELECT id,media_id,base_revision,status,model_name,model_version,confidence,created_at,accepted_revision FROM ai_suggestions WHERE media_id=?1 ORDER BY (status='pending') DESC,created_at DESC,id DESC LIMIT 100")?;
    let rows = statement
        .query_map([media], |r| {
            Ok(Summary {
                id: r.get(0)?,
                media_id: r.get(1)?,
                base_revision: r.get(2)?,
                status: r.get(3)?,
                model_name: r.get(4)?,
                model_version: r.get(5)?,
                confidence: r.get(6)?,
                created_at: r.get(7)?,
                accepted_revision: r.get(8)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}
fn row(s: &ProjectSession, id: &str) -> Result<(String, u32, String, String, Prediction)> {
    uuid::Uuid::parse_str(id).map_err(|_| a::invalid("Invalid suggestion ID."))?;
    let (media,revision,sha,status,value):(String,u32,String,String,String)=s.connection.query_row("SELECT media_id,base_revision,source_sha256,status,prediction_json FROM ai_suggestions WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
    let prediction: Prediction = a::parse(&value)?;
    validate(s, &media, &prediction, &prediction.model)?;
    Ok((media, revision, sha, status, prediction))
}
pub fn preview(s: &ProjectSession, id: &str) -> Result<Preview> {
    let (_, _, _, _, prediction) = row(s, id)?;
    let p = profile::current(s)?;
    let annotations = prediction
        .boxes
        .iter()
        .map(|b| Annotation {
            id: uuid::Uuid::new_v4().to_string(),
            layer_id: prediction.model.layer_id.clone(),
            class_id: Some(prediction.class_map[b.class_index].clone()),
            geometry: Geometry::Bbox {
                x: b.x,
                y: b.y,
                width: b.width,
                height: b.height,
            },
            source: "model_suggestion".into(),
        })
        .collect();
    let masks = if let Some(runs) = &prediction.mask {
        let raw = RasterMask {
            layer_id: prediction.model.layer_id.clone(),
            width: prediction.width,
            height: prediction.height,
            runs: runs.clone(),
        };
        let mut pixels = a::unpack(&raw)?;
        let map = prediction
            .class_map
            .iter()
            .map(|id| {
                p.classes
                    .iter()
                    .find(|c| &c.id == id)
                    .map(|c| c.export_id)
                    .ok_or_else(|| a::invalid("Missing AI class mapping."))
            })
            .collect::<Result<Vec<_>>>()?;
        for pixel in &mut pixels {
            *pixel = map[*pixel as usize];
        }
        vec![a::pack(raw.layer_id, raw.width, raw.height, &pixels)]
    } else {
        vec![]
    };
    Ok(Preview {
        prediction,
        annotations,
        masks,
    })
}
pub fn reject(s: &ProjectSession, id: &str) -> Result<()> {
    let (_, _, _, status, _) = row(s, id)?;
    if status == "rejected" {
        return Ok(());
    }
    if status != "pending" {
        return Err(a::invalid("Only pending suggestions can be rejected."));
    }
    s.connection.execute("UPDATE ai_suggestions SET status='rejected',reviewed_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1 AND status='pending'",[id])?;
    Ok(())
}
pub fn approve(s: &mut ProjectSession, id: &str, expected_revision: u32) -> Result<Document> {
    let (media, base, sha, status, _) = row(s, id)?;
    if status != "pending" {
        return Err(a::invalid("Suggestion has already been reviewed."));
    }
    let mut doc = a::load(s, &media)?;
    if doc.revision != expected_revision || doc.revision != base {
        return Err(AppError::new(
            ErrorCode::Conflict,
            "The document changed since this prediction. Reject it and run AI again.",
        ));
    }
    if doc.review_status == "approved" {
        return Err(a::invalid("Reopen this image's review status before accepting AI suggestions. Approved labels are protected."));
    }
    // Recheck original identity even when inference used a cached canonical image.
    let record = library::record(s, &media)?;
    if record.sha256 != sha || crate::video::source_sha(&library::resolve(s, &record.path)?)? != sha
    {
        return Err(a::invalid(
            "Source changed since AI inference. Repair its original reference.",
        ));
    }
    let value = preview(s, id)?;
    for mut shape in value.annotations {
        shape.source = "model_approved".into();
        doc.annotations.push(shape);
    }
    for mask in value.masks {
        let mut proposed = a::unpack(&mask)?;
        if let Some(existing) = doc.masks.iter_mut().find(|m| m.layer_id == mask.layer_id) {
            let current = a::unpack(existing)?;
            // Approval fills only ignore pixels; existing human/imported pixels are immutable here.
            for (p, old) in proposed.iter_mut().zip(current) {
                if old != 255 {
                    *p = old;
                }
            }
            *existing = a::pack(mask.layer_id, mask.width, mask.height, &proposed);
        } else {
            doc.masks.push(mask);
        }
    }
    doc.review_status = "needs_review".into();
    let op = uuid::Uuid::new_v4().to_string();
    a::save_suggestion(s, doc, &op, id)?;
    a::load(s, &media)
}
