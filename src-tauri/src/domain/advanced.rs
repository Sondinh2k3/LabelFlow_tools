use super::{annotation::Profile, review::Metadata};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotOptions {
    pub name: String,
    pub seed: String,
    pub train_percent: u8,
    pub val_percent: u8,
    pub approved_only: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrozenImage {
    pub id: String,
    pub source: String,
    pub source_sha256: String,
    pub width: u32,
    pub height: u32,
    pub orientation: u8,
    pub document_sha256: String,
    pub group: String,
    pub split: String,
    pub metadata: Metadata,
    pub source_video: Option<serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotManifest {
    pub format_version: u32,
    pub project_id: String,
    pub profile: Profile,
    pub seed: String,
    pub train_percent: u8,
    pub val_percent: u8,
    pub approved_only: bool,
    pub trips: Vec<super::review::Trip>,
    pub images: Vec<FrozenImage>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotSummary {
    pub id: String,
    pub name: String,
    pub images: u32,
    pub created_at: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotCheck {
    pub id: String,
    pub images: u32,
    pub split_counts: BTreeMap<String, u32>,
    pub source_checked: bool,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameCandidate {
    pub media_id: String,
    pub name: String,
    pub time_ms: f64,
    pub delta_ms: f64,
    pub review_status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PropagationInfo {
    pub key: String,
    pub track_id: String,
    pub anchor_id: String,
    pub anchor_revision: u32,
    pub target_id: String,
    pub dx: i32,
    pub dy: i32,
    pub mean_error: f64,
    pub match_margin: f64,
    pub skipped_shapes: u32,
    pub verified_revision: Option<u32>,
    pub verified_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropagationOrigin {
    pub track_id: String,
    pub anchor_id: String,
    pub anchor_revision: u32,
    pub anchor_sha256: String,
    pub dx: i32,
    pub dy: i32,
    pub mean_error: f64,
    pub match_margin: f64,
    pub skipped_shapes: u32,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PropagationReport {
    pub created: Vec<PropagationInfo>,
    pub skipped: Vec<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexReport {
    pub indexed: u32,
    pub reused: u32,
    pub errors: Vec<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateCandidate {
    pub media_id: String,
    pub name: String,
    pub distance: u32,
    pub same_source_hash: bool,
    pub same_group: bool,
    pub brightness: f64,
    pub sharpness: f64,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    pub images: u32,
    pub review_counts: BTreeMap<String, u32>,
    pub event_counts: BTreeMap<String, u32>,
    pub tag_counts: BTreeMap<String, u32>,
    pub group_count: u32,
    pub trip_count: u32,
    pub indexed: u32,
    pub index_errors: u32,
    pub unindexed: u32,
    pub low_detail_candidates: u32,
    pub pending_predictions: u32,
    pub pending_propagations: u32,
    pub snapshots: u32,
}
