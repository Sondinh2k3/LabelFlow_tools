use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub sha256: String,
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub rotation: i32,
    pub time_base_num: i64,
    pub time_base_den: i64,
    pub start_pts: i64,
    pub duration_ms: f64,
    pub average_fps: f64,
    pub frame_count: u32,
    pub split_group: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameInfo {
    pub index: u32,
    pub pts: i64,
    pub time_ms: f64,
    pub duration_ms: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtractionOptions {
    pub video_id: String,
    pub indices: Vec<u32>,
    pub start_ms: f64,
    pub end_ms: f64,
    pub interval_ms: Option<f64>,
    pub format: String,
    pub quality: u8,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoPage {
    pub video: VideoInfo,
    pub frames: Vec<FrameInfo>,
}
