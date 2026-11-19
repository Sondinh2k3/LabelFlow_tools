use super::{metadata, predictions, queue};
use crate::{
    domain::{annotation::Progress, review::*, Result},
    services::{annotation as a, library, profile::SEMANTIC_LAYER, project::ProjectSession},
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelMetrics {
    model: Option<PredictionModel>,
    samples: u32,
    approved_samples: u32,
    true_positive: u32,
    false_positive: u32,
    true_negative: u32,
    false_negative: u32,
    excluded_event_samples: u32,
    event_recall: Option<f64>,
    event_precision: Option<f64>,
    false_positive_frames_per_km: Option<f64>,
    false_trigger_episodes_per_km: Option<f64>,
    distance_km: Option<f64>,
    layers: BTreeMap<String, Vec<ClassMetric>>,
    layer_mean_iou: BTreeMap<String, Option<f64>>,
    #[serde(skip)]
    trips: HashSet<String>,
    #[serde(skip)]
    missing_trip: bool,
}
fn cell(value: &str) -> String {
    let safe = if value.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{value}")
    } else {
        value.into()
    };
    format!("\"{}\"", safe.replace('"', "\"\""))
}
pub fn export(
    s: &ProjectSession,
    parent: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<PathBuf> {
    let job = crate::jobs::start(&s.connection, "review_metrics_export", &a::json(&parent)?)?;
    let result = export_inner(s, parent, cancel, |p| {
        crate::jobs::progress(&s.connection, &job, p.completed, p.total);
        progress(p);
    });
    queue::finish(s, &job, &result);
    result
}
fn export_inner(
    s: &ProjectSession,
    parent: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<PathBuf> {
    let keys = queue::keys(s)?;
    if keys.is_empty() {
        return Err(a::invalid("No stored predictions to export."));
    }
    // Latest sample for each image/model name/version; repeated inference must not inflate event metrics.
    let mut newest = BTreeMap::new();
    for key in keys {
        queue::check(cancel)?;
        let (summary, p) = predictions::load(s, &key)?;
        newest.insert(
            (
                summary.media_id,
                p.model.id,
                p.model.version,
                p.model.sha256,
            ),
            key,
        );
    }
    let name = format!("LabelFlow-review-{}", uuid::Uuid::new_v4());
    let parent = parent.canonicalize()?;
    let staging = parent.join(format!(".{name}.tmp"));
    fs::create_dir(&staging)?;
    for directory in ["predictions", "ground-truth"] {
        fs::create_dir(staging.join(directory))?;
    }
    let mut hashes = BTreeMap::new();
    let mut models: BTreeMap<String, ModelMetrics> = BTreeMap::new();
    let trips = metadata::trips(s)?;
    let mut csv=String::from("media_id,model,version,ground_truth_event,predicted_event,ground_truth_approved,disagreement,trip_id\n");
    let write = |name: &str, data: &[u8], hashes: &mut BTreeMap<String, String>| -> Result<()> {
        a::atomic_file(&staging.join(name), data)?;
        hashes.insert(name.into(), a::sha(data));
        Ok(())
    };
    for (index, key) in newest.values().enumerate() {
        queue::check(cancel)?;
        let (summary, p) = predictions::load(s, key)?;
        let record = library::record(s, &summary.media_id)?;
        let source = library::resolve(s, &record.path)?;
        if crate::video::source_sha(&source)? != record.sha256 {
            return Err(a::invalid("Review export source changed or is missing."));
        }
        let doc = a::load(s, &summary.media_id)?;
        let meta = metadata::load(s, &summary.media_id)?;
        let comparison = super::metrics::compare(&doc, &p, false)?;
        let model = models
            .entry(a::json(&(&p.model.id, &p.model.version, &p.model.sha256))?)
            .or_default();
        model.model = Some(p.model.clone());
        model.samples += 1;
        if comparison.gt_approved {
            model.approved_samples += 1;
            let scored = match (doc.event_label.as_deref(), p.event_label.as_deref()) {
                (Some("positive"), Some("positive")) => {
                    model.true_positive += 1;
                    true
                }
                (Some("positive"), Some("negative")) => {
                    model.false_negative += 1;
                    true
                }
                (Some("negative"), Some("positive")) => {
                    model.false_positive += 1;
                    true
                }
                (Some("negative"), Some("negative")) => {
                    model.true_negative += 1;
                    true
                }
                _ => {
                    model.excluded_event_samples += 1;
                    false
                }
            };
            if scored {
                if let Some(trip) = &meta.trip_id {
                    model.trips.insert(trip.clone());
                } else {
                    model.missing_trip = true;
                }
            }
            for layer in &comparison.layers {
                // Semantic comparisons list only classes present in GT or prediction,
                // so totals are matched by class ID rather than by position.
                let classes = model.layers.entry(layer.layer_id.clone()).or_default();
                for row in &layer.classes {
                    let index = match classes.binary_search_by_key(&row.class_id, |c| c.class_id) {
                        Ok(i) => i,
                        Err(i) => {
                            classes.insert(
                                i,
                                ClassMetric {
                                    class_id: row.class_id,
                                    intersection: 0,
                                    union: 0,
                                    iou: None,
                                },
                            );
                            i
                        }
                    };
                    classes[index].intersection += row.intersection;
                    classes[index].union += row.union;
                }
            }
        } else {
            model.excluded_event_samples += 1;
        }
        let id = key.replace(':', "-");
        write(
            &format!("predictions/{id}.json"),
            a::json(&p)?.as_bytes(),
            &mut hashes,
        )?;
        let truth = serde_json::json!({"schemaVersion":1,"profileId":s.project.profile_id,"sourceSha256":record.sha256,"width":record.width,"height":record.height,"document":doc,"metadata":meta,"comparison":comparison});
        write(
            &format!("ground-truth/{id}.json"),
            a::json(&truth)?.as_bytes(),
            &mut hashes,
        )?;
        csv.push_str(
            &[
                &summary.media_id,
                &summary.model_name,
                &summary.model_version,
                doc.event_label.as_deref().unwrap_or(""),
                p.event_label.as_deref().unwrap_or(""),
                &comparison.gt_approved.to_string(),
                &comparison.has_disagreement.to_string(),
                meta.trip_id.as_deref().unwrap_or(""),
            ]
            .into_iter()
            .map(cell)
            .collect::<Vec<_>>()
            .join(","),
        );
        csv.push('\n');
        progress(Progress {
            completed: index as u32 + 1,
            total: newest.len() as u32,
            message: "Exporting independent predictions / ground truth and metrics".into(),
        });
    }
    for model in models.values_mut() {
        let positive = model.true_positive + model.false_negative;
        let predicted = model.true_positive + model.false_positive;
        model.event_recall = if positive > 0 {
            Some(f64::from(model.true_positive) / f64::from(positive))
        } else {
            None
        };
        model.event_precision = if predicted > 0 {
            Some(f64::from(model.true_positive) / f64::from(predicted))
        } else {
            None
        };
        if !model.missing_trip && !model.trips.is_empty() {
            let distances = model
                .trips
                .iter()
                .map(|id| {
                    trips
                        .iter()
                        .find(|t| &t.id == id)
                        .and_then(|t| t.distance_km)
                })
                .collect::<Option<Vec<_>>>();
            if let Some(distances) = distances {
                let km = distances.iter().sum::<f64>();
                model.distance_km = Some(km);
                model.false_positive_frames_per_km = Some(f64::from(model.false_positive) / km);
            }
        }
        for (layer, classes) in &mut model.layers {
            for c in classes.iter_mut() {
                c.iou = if c.union > 0 {
                    Some(c.intersection as f64 / c.union as f64)
                } else {
                    None
                };
            }
            let values = classes
                .iter()
                .filter(|c| layer == SEMANTIC_LAYER || c.class_id == 1)
                .filter_map(|c| c.iou)
                .collect::<Vec<_>>();
            model.layer_mean_iou.insert(
                layer.clone(),
                if values.is_empty() {
                    None
                } else {
                    Some(values.iter().sum::<f64>() / values.len() as f64)
                },
            );
        }
    }
    write("metrics.csv", csv.as_bytes(), &mut hashes)?;
    write("metrics.json",a::json(&serde_json::json!({"schemaVersion":1,"profileId":s.project.profile_id,"models":models,"trips":trips,"policy":{"eligibleGroundTruth":"approved only","event":"positive/negative only; uncertain/missing excluded","semantic":"project semantic class IDs 0..254; ground-truth ignore 255 excluded; predicted ignore counts as an error","regions":"foreground IoU; absent GT layer unscored, explicit all-zero GT is scored","sampling":"latest prediction per image/model ID/version/SHA-256","distance":"false-positive reviewed frames per supplied trip km; not event episodes or validated continuous exposure","falseTriggerEpisodesPerKm":"unavailable without continuous event/distance coverage"}}))?.as_bytes(),&mut hashes)?;
    queue::check(cancel)?;
    for (name, hash) in &hashes {
        if a::sha(&fs::read(staging.join(name))?) != *hash {
            return Err(a::invalid("Review export checksum failed."));
        }
    }
    a::atomic_file(
        &staging.join("checksums.json"),
        a::json(&hashes)?.as_bytes(),
    )?;
    queue::check(cancel)?;
    let destination = parent.join(name);
    fs::rename(staging, &destination)?;
    Ok(destination)
}
