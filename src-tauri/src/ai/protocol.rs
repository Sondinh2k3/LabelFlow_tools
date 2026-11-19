use crate::{domain::Result, services::annotation::invalid};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const VERSION: u32 = 1;
pub const MAX_MESSAGE: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Model {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub license: String,
    pub file: String,
    pub sha256: String,
    pub adapter: String,
    pub task: String,
    pub layer_id: String,
    pub class_map: Vec<String>,
    pub input: Value,
    pub output: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PredictedBox {
    pub class_index: usize,
    pub confidence: f64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Processing {
    pub milliseconds: f64,
    pub provider: String,
    pub device: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Prediction {
    pub width: u32,
    pub height: u32,
    pub orientation: u8,
    pub coordinate_convention: String,
    pub class_map: Vec<String>,
    pub model: Model,
    pub boxes: Vec<PredictedBox>,
    pub mask: Option<Vec<u32>>,
    pub confidence: Option<f64>,
    pub processing: Processing,
}
pub fn result(value: Value, request_id: &str) -> Result<Value> {
    if value.get("protocol_version").and_then(Value::as_u64) != Some(u64::from(VERSION))
        || value.get("requestId").and_then(Value::as_str) != Some(request_id)
    {
        return Err(invalid(
            "AI worker protocol version or request ID mismatch.",
        ));
    }
    match (value.get("result"), value.get("error")) {
        (Some(result), None) => Ok(result.clone()),
        (None, Some(error)) => Err(invalid(format!(
            "AI worker: {}",
            error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("Unknown error")
        ))),
        _ => Err(invalid("Malformed AI worker response.")),
    }
}
