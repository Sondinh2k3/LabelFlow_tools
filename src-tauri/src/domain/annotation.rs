use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PolygonRings {
    pub points: Vec<Point>,
    pub holes: Vec<Vec<Point>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum Geometry {
    Bbox {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
    Polygon {
        points: Vec<Point>,
        holes: Vec<Vec<Point>>,
    },
    MultiPolygon {
        polygons: Vec<PolygonRings>,
    },
    Mask {
        width: u32,
        height: u32,
        runs: Vec<u32>,
        #[serde(default)]
        iscrowd: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Annotation {
    pub id: String,
    pub layer_id: String,
    pub class_id: Option<String>,
    pub geometry: Geometry,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RasterMask {
    pub layer_id: String,
    pub width: u32,
    pub height: u32,
    /// Alternating value, count pairs, covering exactly width * height pixels.
    pub runs: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Document {
    pub media_id: String,
    pub revision: u32,
    pub review_status: String,
    pub event_label: Option<String>,
    pub notes: String,
    pub split_group: String,
    pub annotations: Vec<Annotation>,
    pub masks: Vec<RasterMask>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassDefinition {
    pub id: String,
    pub namespace: String,
    pub stable_key: String,
    pub export_id: u8,
    pub display_name: String,
    pub color: String,
    pub is_ignore: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerDefinition {
    pub id: String,
    pub stable_key: String,
    pub layer_type: String,
    pub display_name: String,
    pub namespace: Option<String>,
    pub color: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub classes: Vec<ClassDefinition>,
    pub layers: Vec<LayerDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaRecord {
    pub id: String,
    pub name: String,
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub orientation: u8,
    pub sha256: String,
    pub review_status: String,
    pub import_mode: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaPage {
    pub items: Vec<MediaRecord>,
    pub total: u32,
}
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub imported: u32,
    pub duplicates: u32,
    pub warnings: Vec<String>,
    pub cancelled: bool,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub completed: u32,
    pub total: u32,
    pub message: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportOptions {
    pub format: String,
    pub approved_only: bool,
    pub seed: String,
    pub train_percent: u8,
    pub val_percent: u8,
    #[serde(default)]
    pub skip_unsupported: bool,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReport {
    pub path: String,
    pub images: u32,
    pub annotations: u32,
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassMapping {
    pub source: u8,
    pub target: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PixelMapping {
    pub source: u8,
    pub target: u8,
}
