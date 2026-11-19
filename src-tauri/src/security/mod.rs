use crate::domain::{AppError, ErrorCode, Result};
pub mod resources;
use fs2::FileExt;
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
};

// Keep the inode/file permanently in place. Unlinking a held lock creates a race.
pub struct ProjectLock {
    _file: File,
}
impl ProjectLock {
    pub fn acquire(metadata: &Path) -> Result<Self> {
        let path = metadata.join("project.lock");
        reject_symlink(&path)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        file.try_lock_exclusive().map_err(|e| {
            if e.kind() == std::io::ErrorKind::WouldBlock
                || e.raw_os_error() == fs2::lock_contended_error().raw_os_error()
            {
                AppError::new(
                    ErrorCode::ProjectLocked,
                    "This project is open in another process. Close it there and try again.",
                )
            } else {
                e.into()
            }
        })?;
        Ok(Self { _file: file })
    }
}
pub fn reject_symlink(path: &Path) -> Result<()> {
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() {
            return Err(AppError::new(
                ErrorCode::InvalidProject,
                "Project metadata cannot contain symbolic links.",
            ));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return Err(AppError::new(
                    ErrorCode::InvalidProject,
                    "Project metadata cannot contain reparse points.",
                ));
            }
        }
    }
    Ok(())
}
pub fn metadata_dir(root: &Path) -> Result<PathBuf> {
    let canonical = root.canonicalize()?;
    if canonical.to_str().is_none() {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "Project paths must be valid UTF-8.",
        ));
    }
    if !canonical.is_dir() {
        return Err(AppError::new(
            ErrorCode::InvalidProject,
            "Select a project directory.",
        ));
    }
    let metadata = canonical.join(".labelflow");
    reject_symlink(&metadata)?;
    for file in [
        "project.json",
        "project.sqlite",
        "project.sqlite-wal",
        "project.sqlite-shm",
        "project.sqlite-journal",
        "backups",
        "thumbnails",
        "masks",
        "logs",
    ] {
        reject_symlink(&metadata.join(file))?;
    }
    Ok(metadata)
}
