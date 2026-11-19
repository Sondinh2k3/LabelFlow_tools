//! Team discussion through a shared folder (LAN share, OneDrive, Google Drive, Syncthing, …).
//!
//! No server and no shared database: every comment or status change is one immutable JSON file
//! written only by its author into `entries/<author-id>/`. Sync tools therefore never see two
//! writers on the same file. Threads reference images by content SHA-256, so they match across
//! separate projects and machines regardless of local paths.
use crate::domain::{
    annotation::{Document, Geometry, Point, Profile},
    AppError, ErrorCode, Result,
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

pub const MARKER: &str = "labelflow-team.json";
const ENTRY_VERSION: u32 = 1;
const MAX_ENTRY_BYTES: u64 = 512 * 1024;
const MAX_PREVIEW_BYTES: u64 = 6 * 1024 * 1024;
const MAX_FILES: usize = 50_000;
const MAX_BODY: usize = 4000;
const MAX_LABELS: usize = 300;
const MAX_POINTS: usize = 64;
const PREVIEW_SIDE: u32 = 1600;

fn invalid(message: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::InvalidInput, message)
}
fn is_uuid(value: &str) -> bool {
    value.len() == 36 && uuid::Uuid::parse_str(value).is_ok()
}
fn is_sha(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
fn clean_text(value: &str, max: usize) -> String {
    value
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .take(max)
        .collect::<String>()
        .trim()
        .to_owned()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Author {
    pub id: String,
    pub name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MediaRef {
    pub sha256: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LabelShape {
    pub name: String,
    pub color: String,
    /// `box` uses two corner points; `polygon` uses its exterior ring.
    pub kind: String,
    pub points: Vec<[f64; 2]>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub version: u32,
    pub id: String,
    /// `comment` or `status`.
    pub kind: String,
    pub thread_id: String,
    pub author: Author,
    pub created_at_ms: u64,
    #[serde(default)]
    pub media: Option<MediaRef>,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub pin: Option<Point>,
    #[serde(default)]
    pub labels: Vec<LabelShape>,
    /// For `status` entries: `open` or `resolved`.
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub project: String,
}

fn validate_entry(entry: &Entry) -> std::result::Result<(), &'static str> {
    if entry.version != ENTRY_VERSION {
        return Err("unsupported version");
    }
    if !is_uuid(&entry.id) || !is_uuid(&entry.thread_id) || !is_uuid(&entry.author.id) {
        return Err("invalid identifier");
    }
    if entry.author.name.chars().count() > 80 || entry.project.chars().count() > 200 {
        return Err("text too long");
    }
    match entry.kind.as_str() {
        "comment" => {
            if entry.body.chars().count() > MAX_BODY || entry.status.is_some() {
                return Err("invalid comment");
            }
        }
        "status" => {
            if !matches!(entry.status.as_deref(), Some("open" | "resolved")) {
                return Err("invalid status");
            }
        }
        _ => return Err("unknown kind"),
    }
    if let Some(media) = &entry.media {
        if !is_sha(&media.sha256)
            || media.name.chars().count() > 260
            || media.width == 0
            || media.height == 0
            || media.width > 100_000
            || media.height > 100_000
        {
            return Err("invalid media reference");
        }
    }
    if let Some(pin) = &entry.pin {
        if !pin.x.is_finite() || !pin.y.is_finite() {
            return Err("invalid pin");
        }
    }
    if entry.labels.len() > MAX_LABELS
        || entry.labels.iter().any(|l| {
            l.points.len() > MAX_POINTS
                || l.points.is_empty()
                || l.name.chars().count() > 80
                || l.color.len() > 9
                || !matches!(l.kind.as_str(), "box" | "polygon")
                || l.points.iter().flatten().any(|v| !v.is_finite())
        })
    {
        return Err("invalid labels");
    }
    Ok(())
}

// ---------------------------------------------------------------- local per-user configuration

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TeamConfig {
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub user_name: String,
    /// Project ID → team folder. Each person may mount the shared folder at a different path.
    #[serde(default)]
    pub folders: BTreeMap<String, String>,
}
pub fn load_config(dir: &Path) -> TeamConfig {
    let path = dir.join("team.json");
    let mut config: TeamConfig = fs::metadata(&path)
        .ok()
        .filter(|m| m.len() < 256 * 1024)
        .and_then(|_| fs::read(&path).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let mut changed = false;
    if !is_uuid(&config.user_id) {
        config.user_id = uuid::Uuid::new_v4().to_string();
        changed = true;
    }
    if config.user_name.trim().is_empty() {
        config.user_name = std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .map(|n| clean_text(&n, 80))
            .ok()
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "Labeler".into());
        changed = true;
    }
    if changed {
        let _ = save_config(dir, &config);
    }
    config
}
pub fn save_config(dir: &Path, config: &TeamConfig) -> Result<()> {
    fs::create_dir_all(dir)?;
    let bytes = serde_json::to_vec_pretty(config).map_err(|e| invalid(e.to_string()))?;
    write_atomic(&dir.join("team.json"), &bytes, true)
}
pub fn set_user_name(config: &mut TeamConfig, name: &str) -> Result<()> {
    let name = clean_text(name, 80);
    if name.is_empty() {
        return Err(invalid("Enter your display name (1–80 characters)."));
    }
    config.user_name = name;
    Ok(())
}

fn write_atomic(path: &Path, bytes: &[u8], replace: bool) -> Result<()> {
    let parent = path.parent().ok_or_else(|| invalid("Invalid path."))?;
    let temporary = parent.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        if !replace && path.exists() {
            return Ok(());
        }
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    result
}

// ---------------------------------------------------------------- shared folder

/// Accepts an existing team folder or initializes an empty/new one. Never deletes anything.
pub fn prepare_folder(folder: &Path) -> Result<PathBuf> {
    let folder = folder.canonicalize()?;
    if !folder.is_dir() {
        return Err(invalid("Choose a folder for team discussions."));
    }
    let marker = folder.join(MARKER);
    if marker.is_file() {
        let bytes = fs::read(&marker)?;
        let value: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|_| invalid("labelflow-team.json is not valid JSON."))?;
        if value.get("format").and_then(|v| v.as_str()) != Some("labelflow-team") {
            return Err(invalid(
                "This folder has an unrelated labelflow-team.json file.",
            ));
        }
    } else {
        let bytes = serde_json::to_vec_pretty(&serde_json::json!({
            "format": "labelflow-team",
            "version": 1,
            "note": "Shared LabelFlow discussions. Each person writes only to entries/<their id>/. Do not edit files by hand."
        }))
        .map_err(|e| invalid(e.to_string()))?;
        write_atomic(&marker, &bytes, false)?;
    }
    fs::create_dir_all(folder.join("entries"))?;
    fs::create_dir_all(folder.join("previews"))?;
    Ok(folder)
}
fn check_folder(folder: &Path) -> Result<()> {
    if !folder.join(MARKER).is_file() {
        return Err(AppError::new(
            ErrorCode::Io,
            "Team folder is unavailable. Reconnect the shared drive or choose the folder again.",
        ));
    }
    Ok(())
}

pub fn write_entry(folder: &Path, entry: &Entry) -> Result<()> {
    check_folder(folder)?;
    validate_entry(entry).map_err(invalid)?;
    let dir = folder.join("entries").join(&entry.author.id);
    fs::create_dir_all(&dir)?;
    let bytes = serde_json::to_vec_pretty(entry).map_err(|e| invalid(e.to_string()))?;
    if bytes.len() as u64 > MAX_ENTRY_BYTES {
        return Err(invalid("Comment is too large; share fewer labels."));
    }
    write_atomic(&dir.join(format!("{}.json", entry.id)), &bytes, false)
}

pub struct NewComment {
    pub thread_id: Option<String>,
    pub media: Option<MediaRef>,
    pub body: String,
    pub pin: Option<Point>,
    pub labels: Vec<LabelShape>,
    pub project: String,
}
pub fn comment(folder: &Path, config: &TeamConfig, input: NewComment) -> Result<Entry> {
    let body = clean_text(&input.body, MAX_BODY);
    if body.is_empty() {
        return Err(invalid("Write a comment first."));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let thread_id = match input.thread_id {
        Some(t) if is_uuid(&t) => t,
        Some(_) => return Err(invalid("Invalid discussion ID.")),
        None if input.media.is_some() => id.clone(),
        None => return Err(invalid("A new discussion needs an image.")),
    };
    let pin = input.pin.and_then(|p| {
        input.media.as_ref().map(|m| Point {
            x: p.x.clamp(0.0, f64::from(m.width)),
            y: p.y.clamp(0.0, f64::from(m.height)),
        })
    });
    let entry = Entry {
        version: ENTRY_VERSION,
        id,
        kind: "comment".into(),
        thread_id,
        author: Author {
            id: config.user_id.clone(),
            name: config.user_name.clone(),
        },
        created_at_ms: now_ms(),
        media: input.media,
        body,
        pin,
        labels: input.labels.into_iter().take(MAX_LABELS).collect(),
        status: None,
        project: clean_text(&input.project, 200),
    };
    write_entry(folder, &entry)?;
    Ok(entry)
}
pub fn set_status(
    folder: &Path,
    config: &TeamConfig,
    thread_id: &str,
    resolved: bool,
) -> Result<Entry> {
    if !is_uuid(thread_id) {
        return Err(invalid("Invalid discussion ID."));
    }
    let entry = Entry {
        version: ENTRY_VERSION,
        id: uuid::Uuid::new_v4().to_string(),
        kind: "status".into(),
        thread_id: thread_id.into(),
        author: Author {
            id: config.user_id.clone(),
            name: config.user_name.clone(),
        },
        created_at_ms: now_ms(),
        media: None,
        body: String::new(),
        pin: None,
        labels: Vec::new(),
        status: Some(if resolved { "resolved" } else { "open" }.into()),
        project: String::new(),
    };
    write_entry(folder, &entry)?;
    Ok(entry)
}

/// Content-addressed preview so teammates without the source file can still see the case.
pub fn write_preview(folder: &Path, sha256: &str, image: &image::DynamicImage) -> Result<()> {
    if !is_sha(sha256) {
        return Err(invalid("Invalid image hash."));
    }
    check_folder(folder)?;
    let dir = folder.join("previews");
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.jpg", sha256.to_ascii_lowercase()));
    if path.is_file() {
        return Ok(());
    }
    let resized = if image.width() > PREVIEW_SIDE || image.height() > PREVIEW_SIDE {
        image.resize(
            PREVIEW_SIDE,
            PREVIEW_SIDE,
            image::imageops::FilterType::Triangle,
        )
    } else {
        image.clone()
    };
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 84)
        .encode_image(&resized.to_rgb8())
        .map_err(|e| invalid(e.to_string()))?;
    write_atomic(&path, &bytes.into_inner(), false)
}
pub fn read_preview(folder: &Path, sha256: &str) -> Result<Option<String>> {
    if !is_sha(sha256) {
        return Err(invalid("Invalid image hash."));
    }
    let path = folder
        .join("previews")
        .join(format!("{}.jpg", sha256.to_ascii_lowercase()));
    let Ok(meta) = fs::symlink_metadata(&path) else {
        return Ok(None);
    };
    if !meta.is_file() || meta.len() > MAX_PREVIEW_BYTES {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    fs::File::open(&path)?
        .take(MAX_PREVIEW_BYTES)
        .read_to_end(&mut bytes)?;
    if !bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Ok(None);
    }
    Ok(Some(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )))
}

// ---------------------------------------------------------------- reading the feed

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub id: String,
    pub author: Author,
    pub created_at_ms: u64,
    pub body: String,
    pub pin: Option<Point>,
    pub labels: Vec<LabelShape>,
    pub project: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub id: String,
    pub status: String,
    pub status_by: Option<Author>,
    pub media: MediaRef,
    pub local_media_id: Option<String>,
    pub comments: Vec<Comment>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    /// Number of entries (comments + status changes); used for unread tracking.
    pub activity: u32,
}
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Feed {
    pub threads: Vec<Thread>,
    pub warnings: Vec<String>,
}

type CachedFile = (SystemTime, u64, Option<Entry>);
#[derive(Default)]
pub struct Cache {
    inner: Mutex<(PathBuf, HashMap<PathBuf, CachedFile>)>,
}
impl Cache {
    /// Lists the folder and parses only new or changed files.
    pub fn scan(&self, folder: &Path) -> Result<(Vec<Entry>, Vec<String>)> {
        check_folder(folder)?;
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| AppError::new(ErrorCode::Internal, "Team cache unavailable."))?;
        if guard.0 != folder {
            *guard = (folder.to_path_buf(), HashMap::new());
        }
        let mut seen = HashMap::new();
        let mut warnings = Vec::new();
        let mut count = 0usize;
        let entries = folder.join("entries");
        let authors = match fs::read_dir(&entries) {
            Ok(list) => list,
            Err(_) => return Ok((Vec::new(), warnings)),
        };
        'outer: for author in authors.flatten() {
            let author_id = author.file_name().to_string_lossy().into_owned();
            if !is_uuid(&author_id) || !author.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let Ok(files) = fs::read_dir(author.path()) else {
                continue;
            };
            for file in files.flatten() {
                let name = file.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') || !name.ends_with(".json") {
                    continue;
                }
                count += 1;
                if count > MAX_FILES {
                    warnings.push(format!(
                        "Only the first {MAX_FILES} discussion files were read."
                    ));
                    break 'outer;
                }
                let path = file.path();
                let Ok(meta) = file.metadata() else { continue };
                if !meta.is_file() {
                    continue;
                }
                let modified = meta.modified().unwrap_or(UNIX_EPOCH);
                let cached = guard
                    .1
                    .get(&path)
                    .filter(|c| c.0 == modified && c.1 == meta.len())
                    .cloned();
                let value = match cached {
                    Some(c) => c,
                    None => {
                        let parsed = parse_file(&path, meta.len(), &author_id, &name);
                        if let Err(problem) = &parsed {
                            if warnings.len() < 20 {
                                warnings.push(format!("Skipped {author_id}/{name}: {problem}."));
                            }
                        }
                        (modified, meta.len(), parsed.ok())
                    }
                };
                seen.insert(path, value);
            }
        }
        guard.1 = seen;
        let list = guard.1.values().filter_map(|c| c.2.clone()).collect();
        Ok((list, warnings))
    }
}
fn parse_file(
    path: &Path,
    len: u64,
    author_id: &str,
    name: &str,
) -> std::result::Result<Entry, &'static str> {
    if len > MAX_ENTRY_BYTES {
        return Err("file too large");
    }
    let bytes = fs::read(path).map_err(|_| "unreadable")?;
    let entry: Entry = serde_json::from_slice(&bytes).map_err(|_| "invalid JSON")?;
    validate_entry(&entry)?;
    if entry.author.id != author_id || name != format!("{}.json", entry.id) {
        return Err("author or file name mismatch");
    }
    Ok(entry)
}

pub fn threads(entries: Vec<Entry>) -> Vec<Thread> {
    let mut groups: HashMap<String, Vec<Entry>> = HashMap::new();
    for e in entries {
        groups.entry(e.thread_id.clone()).or_default().push(e);
    }
    let mut result = groups
        .into_iter()
        .filter_map(|(id, mut list)| {
            list.sort_by(|a, b| (a.created_at_ms, &a.id).cmp(&(b.created_at_ms, &b.id)));
            // The opening comment defines the image; replies synced earlier wait for it.
            let media = list
                .iter()
                .find(|e| e.kind == "comment" && e.id == id)
                .or_else(|| list.iter().find(|e| e.kind == "comment"))
                .and_then(|e| e.media.clone())?;
            let last_status = list.iter().rev().find(|e| e.kind == "status");
            let comments = list
                .iter()
                .filter(|e| e.kind == "comment")
                .map(|e| Comment {
                    id: e.id.clone(),
                    author: e.author.clone(),
                    created_at_ms: e.created_at_ms,
                    body: e.body.clone(),
                    pin: e.pin.clone(),
                    labels: e.labels.clone(),
                    project: e.project.clone(),
                })
                .collect::<Vec<_>>();
            Some(Thread {
                id,
                status: last_status
                    .and_then(|e| e.status.clone())
                    .unwrap_or_else(|| "open".into()),
                status_by: last_status.map(|e| e.author.clone()),
                media,
                local_media_id: None,
                created_at_ms: list.first().map_or(0, |e| e.created_at_ms),
                updated_at_ms: list.last().map_or(0, |e| e.created_at_ms),
                activity: list.len() as u32,
                comments,
            })
        })
        .collect::<Vec<_>>();
    result.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms).then(a.id.cmp(&b.id)));
    result
}

/// Vector labels to show next to a question. Raster masks are summarized by the preview only.
pub fn label_shapes(document: &Document, profile: &Profile) -> Vec<LabelShape> {
    let subsample = |points: &[Point]| -> Vec<[f64; 2]> {
        let step = points.len().div_ceil(MAX_POINTS).max(1);
        points
            .iter()
            .step_by(step)
            .take(MAX_POINTS)
            .map(|p| [(p.x * 10.0).round() / 10.0, (p.y * 10.0).round() / 10.0])
            .collect()
    };
    let mut shapes = Vec::new();
    for a in &document.annotations {
        let class = a
            .class_id
            .as_ref()
            .and_then(|id| profile.classes.iter().find(|c| &c.id == id));
        let layer = profile.layers.iter().find(|l| l.id == a.layer_id);
        let name = class
            .map(|c| c.display_name.clone())
            .or_else(|| layer.map(|l| l.display_name.clone()))
            .unwrap_or_default();
        let color = class
            .map(|c| c.color.clone())
            .or_else(|| layer.map(|l| l.color.clone()))
            .unwrap_or_else(|| "#ffffff".into());
        let mut push = |kind: &str, points: Vec<[f64; 2]>| {
            if points.len() >= 2 {
                shapes.push(LabelShape {
                    name: name.chars().take(80).collect(),
                    color: color.chars().take(9).collect(),
                    kind: kind.into(),
                    points,
                });
            }
        };
        match &a.geometry {
            Geometry::Bbox {
                x,
                y,
                width,
                height,
            } => push("box", vec![[*x, *y], [x + width, y + height]]),
            Geometry::Polygon { points, .. } => push("polygon", subsample(points)),
            Geometry::MultiPolygon { polygons } => {
                for p in polygons {
                    push("polygon", subsample(&p.points));
                }
            }
            Geometry::Mask { .. } => {}
        }
    }
    shapes.truncate(MAX_LABELS);
    shapes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(name: &str) -> TeamConfig {
        TeamConfig {
            user_id: uuid::Uuid::new_v4().to_string(),
            user_name: name.into(),
            folders: BTreeMap::new(),
        }
    }
    fn media() -> MediaRef {
        MediaRef {
            sha256: "a".repeat(64),
            name: "frame.png".into(),
            width: 100,
            height: 50,
        }
    }

    #[test]
    fn two_people_discuss_and_resolve_without_shared_files() {
        let dir = tempfile::tempdir().unwrap();
        let folder = prepare_folder(dir.path()).unwrap();
        let (alice, bob) = (config("Alice"), config("Bob"));
        let question = comment(
            &folder,
            &alice,
            NewComment {
                thread_id: None,
                media: Some(media()),
                body: "Pile or stall?".into(),
                pin: Some(Point { x: 500.0, y: 10.0 }),
                labels: vec![],
                project: "P".into(),
            },
        )
        .unwrap();
        assert_eq!(question.pin, Some(Point { x: 100.0, y: 10.0 }));
        comment(
            &folder,
            &bob,
            NewComment {
                thread_id: Some(question.thread_id.clone()),
                media: None,
                body: "Stall: it has a canopy.".into(),
                pin: None,
                labels: vec![],
                project: String::new(),
            },
        )
        .unwrap();
        set_status(&folder, &bob, &question.thread_id, true).unwrap();
        let cache = Cache::default();
        let (entries, warnings) = cache.scan(&folder).unwrap();
        assert!(warnings.is_empty());
        let list = threads(entries);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].comments.len(), 2);
        assert_eq!(list[0].status, "resolved");
        assert_eq!(list[0].activity, 3);
        assert_eq!(list[0].media, media());
        // Each author only ever wrote inside their own folder.
        for person in [&alice, &bob] {
            assert!(folder.join("entries").join(&person.user_id).is_dir());
        }
        // Reopen, then rescan uses the cache for unchanged files.
        set_status(&folder, &alice, &question.thread_id, false).unwrap();
        let list = threads(cache.scan(&folder).unwrap().0);
        assert_eq!(list[0].status, "open");
        assert_eq!(list[0].status_by.as_ref().unwrap().name, "Alice");
    }

    #[test]
    fn forged_or_corrupt_files_are_skipped_with_warnings() {
        let dir = tempfile::tempdir().unwrap();
        let folder = prepare_folder(dir.path()).unwrap();
        let alice = config("Alice");
        let entry = comment(
            &folder,
            &alice,
            NewComment {
                thread_id: None,
                media: Some(media()),
                body: "ok".into(),
                pin: None,
                labels: vec![],
                project: String::new(),
            },
        )
        .unwrap();
        // Copy Alice's entry into another author's folder: author mismatch is rejected.
        let mallory = uuid::Uuid::new_v4().to_string();
        let forged_dir = folder.join("entries").join(&mallory);
        fs::create_dir_all(&forged_dir).unwrap();
        fs::copy(
            folder
                .join("entries")
                .join(&alice.user_id)
                .join(format!("{}.json", entry.id)),
            forged_dir.join(format!("{}.json", entry.id)),
        )
        .unwrap();
        fs::write(forged_dir.join("broken.json"), b"{nope").unwrap();
        fs::write(forged_dir.join(".partial.tmp"), b"ignored").unwrap();
        let (entries, warnings) = Cache::default().scan(&folder).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn previews_are_content_addressed_and_validated() {
        let dir = tempfile::tempdir().unwrap();
        let folder = prepare_folder(dir.path()).unwrap();
        let image = image::DynamicImage::new_rgb8(3000, 1000);
        let sha = "b".repeat(64);
        write_preview(&folder, &sha, &image).unwrap();
        write_preview(&folder, &sha, &image).unwrap();
        let url = read_preview(&folder, &sha).unwrap().unwrap();
        assert!(url.starts_with("data:image/jpeg;base64,"));
        let stored = image::open(folder.join("previews").join(format!("{sha}.jpg"))).unwrap();
        assert_eq!((stored.width(), stored.height()), (1600, 533));
        assert!(read_preview(&folder, &"c".repeat(64)).unwrap().is_none());
        assert!(read_preview(&folder, "../escape").is_err());
    }

    #[test]
    fn unrelated_marker_and_missing_folder_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(MARKER), b"{\"format\":\"other\"}").unwrap();
        assert!(prepare_folder(dir.path()).is_err());
        let gone = dir.path().join("gone");
        assert!(set_status(&gone, &config("A"), &uuid::Uuid::new_v4().to_string(), true).is_err());
    }
}
