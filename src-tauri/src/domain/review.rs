use super::annotation::{Annotation, Document, RasterMask};
use serde::{Deserialize, Serialize};

pub const TAGS: &[&str] = &[
    "hard_negative:stopped_vehicle",
    "hard_negative:median",
    "hard_negative:tree",
    "hard_negative:roadside_object",
    "hard_negative:occlusion",
    "hard_negative:road_damage",
    "weather:clear",
    "weather:rain",
    "weather:fog",
    "light:day",
    "light:night",
    "light:dusk",
    "occlusion:partial",
    "occlusion:heavy",
    "weather:overcast",
    "road:wet",
    "road:construction",
    "road:no_curb",
    "road:unpaved",
    "object:on_road",
];
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Trip {
    pub id: String,
    pub name: String,
    pub route: String,
    pub camera_id: String,
    pub distance_km: Option<f64>,
    pub notes: String,
    pub revision: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Metadata {
    pub video_trip_revision: Option<u32>,
    pub media_id: String,
    pub revision: u32,
    pub trip_id: Option<String>,
    pub camera_id: String,
    pub route: String,
    pub captured_at: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub tags: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PredictionModel {
    pub id: String,
    pub name: String,
    pub version: String,
    pub sha256: Option<String>,
    pub confidence: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Prediction {
    pub schema_version: u32,
    pub profile_id: String,
    pub media_id: String,
    pub source_sha256: String,
    pub width: u32,
    pub height: u32,
    pub model: PredictionModel,
    pub event_label: Option<String>,
    pub annotations: Vec<Annotation>,
    pub masks: Vec<RasterMask>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub propagation: Option<super::advanced::PropagationOrigin>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PredictionSummary {
    pub key: String,
    pub media_id: String,
    pub base_revision: u32,
    pub status: String,
    pub model_name: String,
    pub model_version: String,
    pub event_label: Option<String>,
    pub created_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassMetric {
    pub class_id: u8,
    pub intersection: u64,
    pub union: u64,
    pub iou: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerMetric {
    pub layer_id: String,
    pub evaluated_pixels: u64,
    pub ignored_pixels: u64,
    pub disagreement_pixels: u64,
    pub mean_iou: Option<f64>,
    pub classes: Vec<ClassMetric>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    pub gt_revision: u32,
    pub gt_approved: bool,
    pub gt_event: Option<String>,
    pub predicted_event: Option<String>,
    pub event_mismatch: Option<bool>,
    pub layers: Vec<LayerMetric>,
    pub missing_gt_layers: Vec<String>,
    pub has_disagreement: bool,
    pub differences: Vec<RasterMask>,
    pub difference_omitted: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub summary: PredictionSummary,
    pub prediction: Prediction,
    pub ground_truth: Document,
    pub comparison: Comparison,
}
#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewFilter {
    pub status: String,
    pub event: String,
    pub tag: String,
    pub trip_id: String,
    pub disagreements_only: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewRow {
    pub media_id: String,
    pub name: String,
    pub review_status: String,
    pub event_label: Option<String>,
    pub trip_name: Option<String>,
    pub prediction_key: Option<String>,
    pub model_name: Option<String>,
    pub disagreement: Option<bool>,
    pub analyzed: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewPage {
    pub items: Vec<ReviewRow>,
    pub total: u32,
}
