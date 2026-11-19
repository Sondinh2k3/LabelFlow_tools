use std::{fs, io::Write, path::PathBuf};
use tauri::{AppHandle, Manager};
pub fn config_dir(app: &AppHandle) -> Option<PathBuf> {
    std::env::var_os("LABELFLOW_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| app.path().app_config_dir().ok())
}
fn file(app: &AppHandle) -> Option<PathBuf> {
    config_dir(app).map(|p| p.join("session.json"))
}
pub fn last_project(app: &AppHandle) -> Option<PathBuf> {
    let path = file(app)?;
    if fs::metadata(&path).ok()?.len() > 64 * 1024 {
        return None;
    }
    let value: Option<String> = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    value.map(PathBuf::from)
}
pub fn remember(app: &AppHandle, root: Option<&std::path::Path>) {
    let Some(path) = file(app) else {
        return;
    };
    let result = (|| -> std::io::Result<()> {
        fs::create_dir_all(path.parent().unwrap())?;
        let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        let mut output = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        let bytes = serde_json::to_vec(&root.map(|r| r.to_string_lossy().into_owned()))?;
        output.write_all(&bytes)?;
        output.sync_all()?;
        drop(output);
        fs::rename(temporary, path)?;
        Ok(())
    })();
    if let Err(e) = result {
        eprintln!("Could not remember optional session settings: {e}");
    }
}
