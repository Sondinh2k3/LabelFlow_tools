use crate::{
    domain::Result,
    services::annotation::{atomic_file, invalid},
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};
use tauri::Manager;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Toolchain {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}
impl Toolchain {
    pub fn discover() -> Result<Self> {
        fn find(name: &str, variable: &str) -> Result<PathBuf> {
            if let Some(path) = std::env::var_os(variable) {
                return Ok(PathBuf::from(path).canonicalize()?);
            }
            for directory in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
                let path = directory.join(if cfg!(windows) {
                    format!("{name}.exe")
                } else {
                    name.into()
                });
                if path.is_file() {
                    return Ok(path.canonicalize()?);
                }
            }
            Err(invalid("FFmpeg/FFprobe were not found. Choose the folder containing both tools in Video tools. Image labeling remains available."))
        }
        Ok(Self {
            ffmpeg: find("ffmpeg", "LABELFLOW_FFMPEG")?,
            ffprobe: find("ffprobe", "LABELFLOW_FFPROBE")?,
        })
    }
    pub fn from_folder(path: &Path) -> Result<Self> {
        let extension = if cfg!(windows) { ".exe" } else { "" };
        Ok(Self {
            ffmpeg: path.join(format!("ffmpeg{extension}")).canonicalize()?,
            ffprobe: path.join(format!("ffprobe{extension}")).canonicalize()?,
        })
    }
    pub fn validate(&self) -> Result<String> {
        let mut versions = Vec::new();
        for (path, name) in [(&self.ffmpeg, "ffmpeg"), (&self.ffprobe, "ffprobe")] {
            let output = super::process::run(
                path,
                &["-version".into()],
                &AtomicBool::new(false),
                128 * 1024,
                Duration::from_secs(10),
            )?;
            let line = String::from_utf8_lossy(&output)
                .lines()
                .next()
                .unwrap_or_default()
                .to_owned();
            if !line.starts_with(&format!("{name} version")) {
                return Err(invalid(
                    "Selected executable did not identify itself as FFmpeg/FFprobe.",
                ));
            }
            versions.push(line);
        }
        Ok(versions.join("\n"))
    }
}
fn settings(app: &tauri::AppHandle) -> Result<PathBuf> {
    let root = std::env::var_os("LABELFLOW_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or(
            app.path()
                .app_config_dir()
                .map_err(|e| invalid(e.to_string()))?,
        );
    Ok(root.join("video-tools.json"))
}
pub fn for_app(app: &tauri::AppHandle) -> Result<Toolchain> {
    if let Ok(tools) = Toolchain::discover() {
        return Ok(tools);
    }
    let path = settings(app)?;
    if std::fs::metadata(&path).is_ok_and(|m| m.len() < 64 * 1024) {
        return serde_json::from_slice(&std::fs::read(path)?).map_err(|e| invalid(e.to_string()));
    }
    Toolchain::discover()
}
pub fn remember(app: &tauri::AppHandle, tools: &Toolchain) -> Result<()> {
    let path = settings(app)?;
    std::fs::create_dir_all(path.parent().unwrap())?;
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    atomic_file(
        &temporary,
        &serde_json::to_vec(tools).map_err(|e| invalid(e.to_string()))?,
    )?;
    std::fs::rename(temporary, path)?;
    Ok(())
}
