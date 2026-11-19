use super::{annotation as a, project::ProjectSession};
use crate::{domain::Result, security::reject_symlink};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
pub enum CacheKind {
    Thumbnails,
    Previews,
    Video,
}
impl CacheKind {
    fn relative(self) -> &'static str {
        match self {
            Self::Thumbnails => "thumbnails",
            Self::Previews => "thumbnails/previews",
            Self::Video => "thumbnails/video",
        }
    }
    pub fn budget(self) -> u64 {
        match self {
            Self::Thumbnails => 32 * 1024 * 1024,
            Self::Previews | Self::Video => 128 * 1024 * 1024,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Thumbnails => "thumbnails",
            Self::Previews => "previews",
            Self::Video => "videoPreviews",
        }
    }
    pub fn directory(self, s: &ProjectSession) -> Result<PathBuf> {
        let metadata = crate::security::metadata_dir(&s.root)?;
        reject_symlink(&metadata.join("thumbnails"))?;
        let directory = metadata.join(self.relative());
        reject_symlink(&directory)?;
        Ok(directory)
    }
    fn recognizes(self, path: &Path) -> bool {
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            return false;
        };
        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        match self {
            Self::Thumbnails => {
                matches!(extension, "png" | "jpg") && uuid::Uuid::parse_str(stem).is_ok()
            }
            Self::Previews => {
                matches!(extension, "png" | "sha256")
                    && stem.rsplit_once('-').is_some_and(|(id, hash)| {
                        uuid::Uuid::parse_str(id).is_ok()
                            && hash.len() == 64
                            && hash.bytes().all(|b| b.is_ascii_hexdigit())
                    })
            }
            Self::Video => {
                extension == "png"
                    && stem.rsplit_once('-').is_some_and(|(id, index)| {
                        uuid::Uuid::parse_str(id).is_ok() && index.parse::<u32>().is_ok()
                    })
            }
        }
    }
}
const KINDS: [CacheKind; 3] = [CacheKind::Thumbnails, CacheKind::Previews, CacheKind::Video];
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    name: &'static str,
    bytes: u64,
    files: u64,
    budget: Option<u64>,
    rebuildable: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    categories: Vec<Category>,
    removable_bytes: u64,
    removed_bytes: u64,
    removed_files: u64,
}

fn cache_entries(
    s: &ProjectSession,
    kind: CacheKind,
) -> Result<Vec<(PathBuf, u64, Option<std::time::SystemTime>)>> {
    let directory = kind.directory(s)?;
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        // Never follow links or recurse into an unrecognized directory during cleanup.
        reject_symlink(&entry.path())?;
        let metadata = entry.metadata()?;
        if metadata.is_file() && kind.recognizes(&entry.path()) {
            entries.push((entry.path(), metadata.len(), metadata.modified().ok()));
        }
    }
    Ok(entries)
}
pub fn trim_cache(s: &ProjectSession, kind: CacheKind, keep: &Path, added: u64) -> Result<()> {
    let mut checks = s.cache_checks.borrow_mut();
    if let Some((last, pending)) = checks.get_mut(&kind) {
        *pending = pending.saturating_add(added);
        if last.elapsed() < Duration::from_secs(30) && *pending < kind.budget() / 8 {
            return Ok(());
        }
    }
    let mut entries = cache_entries(s, kind)?;
    let mut size = entries.iter().map(|e| e.1).sum::<u64>();
    entries.sort_by_key(|e| e.2);
    for (path, bytes, _) in entries {
        if size <= kind.budget() {
            break;
        }
        if path == keep || path.with_extension("png") == keep || path.with_extension("jpg") == keep
        {
            continue;
        }
        reject_symlink(&path)?;
        fs::remove_file(path)?;
        size = size.saturating_sub(bytes);
    }
    checks.insert(kind, (Instant::now(), 0));
    Ok(())
}
// Accounting is user-triggered, shallow for caches and bounded for authoritative folders.
fn tree_size(root: &Path) -> Result<(u64, u64)> {
    let mut pending = vec![(root.to_path_buf(), 0)];
    let (mut bytes, mut files, mut visited) = (0, 0, 0);
    while let Some((path, depth)) = pending.pop() {
        if !path.exists() {
            continue;
        }
        reject_symlink(&path)?;
        visited += 1;
        if visited > 200_000 || depth > 16 {
            return Err(a::invalid("Storage accounting exceeds its entry limit."));
        }
        let metadata = fs::metadata(&path)?;
        if metadata.is_file() {
            bytes += metadata.len();
            files += 1;
        } else if metadata.is_dir() {
            for entry in fs::read_dir(path)? {
                pending.push((entry?.path(), depth + 1));
            }
        }
    }
    Ok((bytes, files))
}
pub fn usage(s: &ProjectSession) -> Result<Usage> {
    let metadata = crate::security::metadata_dir(&s.root)?;
    let mut categories = Vec::new();
    let (mut db_bytes, mut db_files) = (0, 0);
    for name in ["project.sqlite", "project.sqlite-wal", "project.sqlite-shm"] {
        let (bytes, files) = tree_size(&metadata.join(name))?;
        db_bytes += bytes;
        db_files += files;
    }
    categories.push(Category {
        name: "database",
        bytes: db_bytes,
        files: db_files,
        budget: None,
        rebuildable: false,
    });
    for (name, relative) in [
        ("labels", "masks"),
        ("backups", "backups"),
        ("logs", "logs"),
    ] {
        let (bytes, files) = tree_size(&metadata.join(relative))?;
        categories.push(Category {
            name,
            bytes,
            files,
            budget: None,
            rebuildable: false,
        });
    }
    let mut removable_bytes = 0;
    for kind in KINDS {
        let entries = cache_entries(s, kind)?;
        let bytes = entries.iter().map(|e| e.1).sum::<u64>();
        removable_bytes += bytes;
        categories.push(Category {
            name: kind.name(),
            bytes,
            files: entries.len() as u64,
            budget: Some(kind.budget()),
            rebuildable: true,
        });
    }
    Ok(Usage {
        categories,
        removable_bytes,
        removed_bytes: 0,
        removed_files: 0,
    })
}
pub fn clear(s: &ProjectSession, confirmed: bool) -> Result<Usage> {
    if !confirmed {
        return Err(a::invalid(
            "Explicit cache cleanup confirmation is required.",
        ));
    }
    // Validate the whole selection before the first deletion. No caller-supplied paths.
    let mut entries = Vec::new();
    for kind in KINDS {
        entries.extend(cache_entries(s, kind)?);
    }
    let (mut removed_bytes, mut removed_files) = (0, 0);
    for (path, bytes, _) in entries {
        reject_symlink(&path)?;
        fs::remove_file(path)?;
        removed_bytes += bytes;
        removed_files += 1;
    }
    s.cache_checks.borrow_mut().clear();
    let mut result = usage(s)?;
    result.removed_bytes = removed_bytes;
    result.removed_files = removed_files;
    Ok(result)
}
