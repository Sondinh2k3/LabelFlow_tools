use serde::{Deserialize, Serialize};

pub const FORMAT_VERSION: u32 = 1;
pub const SCHEMA_VERSION: u32 = 6;
pub mod advanced;
pub mod geometry;
pub mod review;
pub mod video;

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCode {
    Io,
    Database,
    InvalidProject,
    ProjectLocked,
    UnsupportedVersion,
    InvalidInput,
    InvalidImage,
    NoProject,
    Internal,
    Conflict,
    Cancelled,
}

#[derive(Debug, Serialize)]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
}
impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for AppError {}
impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        Self::new(ErrorCode::Io, value.to_string())
    }
}
impl From<rusqlite::Error> for AppError {
    fn from(value: rusqlite::Error) -> Self {
        Self::new(ErrorCode::Database, value.to_string())
    }
}
pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub format_version: u32,
    pub id: String,
    pub name: String,
    pub profile_id: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub profile_id: String,
    pub schema_version: u32,
    pub root: String,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImageInfo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub orientation: u8,
    pub sha256: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImagePreview {
    pub info: ImageInfo,
    pub data_url: String,
}

pub mod annotation;
