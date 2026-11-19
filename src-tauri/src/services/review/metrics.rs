use crate::{
    domain::{annotation::*, review::*, Result},
    services::annotation as a,
};
use std::collections::BTreeSet;

fn layer_pixels(doc: &Document, layer: &str, width: u32, height: u32) -> Result<Option<Vec<u8>>> {
    let mut result = doc
        .masks
        .iter()
        .find(|m| m.layer_id == layer)
        .map(a::unpack)
        .transpose()?;
    for shape in doc.annotations.iter().filter(|a| a.layer_id == layer) {
        let shape = crate::domain::geometry::rasterize(&shape.geometry, width, height)?;
        let pixels = result.get_or_insert_with(|| vec![0; width as usize * height as usize]);
        for (p, v) in pixels.iter_mut().zip(shape) {
            *p |= v;
        }
    }
    Ok(result)
}
pub fn compare(gt: &Document, pred: &Prediction, include_difference: bool) -> Result<Comparison> {
    let predicted = Document {
        media_id: pred.media_id.clone(),
        revision: 0,
        review_status: "unlabeled".into(),
        event_label: pred.event_label.clone(),
        notes: String::new(),
        split_group: String::new(),
        annotations: pred.annotations.clone(),
        masks: pred.masks.clone(),
    };
    let mut keys: BTreeSet<String> = pred.masks.iter().map(|m| m.layer_id.clone()).collect();
    keys.extend(
        pred.annotations
            .iter()
            .filter(|a| a.class_id.is_none())
            .map(|a| a.layer_id.clone()),
    );
    let mut layers = Vec::new();
    let mut missing = Vec::new();
    let mut differences = Vec::new();
    let mut omitted = false;
    for key in keys {
        let Some(human) = layer_pixels(gt, &key, pred.width, pred.height)? else {
            missing.push(key);
            continue;
        };
        let proposed = layer_pixels(&predicted, &key, pred.width, pred.height)?
            .ok_or_else(|| a::invalid("Missing prediction raster."))?;
        if human.len() != proposed.len() {
            return Err(a::invalid("Comparison dimensions mismatch."));
        }
        let semantic = key == crate::services::profile::SEMANTIC_LAYER;
        // Semantic IDs are project-defined (0..254, 255 = ignore); regions are binary.
        let count = if semantic { 255 } else { 2 };
        let mut intersections = vec![0u64; count];
        let mut gt_counts = vec![0u64; count];
        let mut pred_counts = vec![0u64; count];
        let mut evaluated = 0;
        let mut ignored = 0;
        let mut disagree = 0;
        let mut difference = if include_difference {
            vec![0u8; human.len()]
        } else {
            vec![]
        };
        for (index, (&g, &p)) in human.iter().zip(&proposed).enumerate() {
            if semantic && g == 255 {
                ignored += 1;
                continue;
            }
            if g as usize >= count || (p as usize >= count && !(semantic && p == 255)) {
                return Err(a::invalid("Invalid comparison class ID."));
            }
            evaluated += 1;
            gt_counts[g as usize] += 1;
            if (p as usize) < count {
                pred_counts[p as usize] += 1;
            }
            if g == p {
                intersections[g as usize] += 1;
            } else {
                disagree += 1;
                if include_difference {
                    difference[index] = 1;
                }
            }
        }
        let classes = (0..count)
            .filter(|&i| !semantic || gt_counts[i] + pred_counts[i] > 0)
            .map(|i| {
                let union = gt_counts[i] + pred_counts[i] - intersections[i];
                ClassMetric {
                    class_id: i as u8,
                    intersection: intersections[i],
                    union,
                    iou: if union > 0 {
                        Some(intersections[i] as f64 / union as f64)
                    } else {
                        None
                    },
                }
            })
            .collect::<Vec<_>>();
        let values = classes
            .iter()
            .filter(|c| semantic || c.class_id == 1)
            .filter_map(|c| c.iou)
            .collect::<Vec<_>>();
        let mean = if values.is_empty() {
            None
        } else {
            Some(values.iter().sum::<f64>() / values.len() as f64)
        };
        layers.push(LayerMetric {
            layer_id: key.clone(),
            evaluated_pixels: evaluated,
            ignored_pixels: ignored,
            disagreement_pixels: disagree,
            mean_iou: mean,
            classes,
        });
        if include_difference && disagree > 0 {
            let runs = a::pack(key.clone(), pred.width, pred.height, &difference);
            if runs.runs.len() <= 1_000_000 {
                differences.push(runs);
            } else {
                omitted = true;
            }
        }
    }
    let mismatch = gt
        .event_label
        .as_ref()
        .zip(pred.event_label.as_ref())
        .map(|(g, p)| g != p);
    let disagrees = mismatch == Some(true) || layers.iter().any(|m| m.disagreement_pixels > 0);
    Ok(Comparison {
        gt_revision: gt.revision,
        gt_approved: gt.review_status == "approved",
        gt_event: gt.event_label.clone(),
        predicted_event: pred.event_label.clone(),
        event_mismatch: mismatch,
        layers,
        missing_gt_layers: missing,
        has_disagreement: disagrees,
        differences,
        difference_omitted: omitted,
    })
}
