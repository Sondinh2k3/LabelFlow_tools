use crate::{
    domain::{
        annotation::{ImportReport, MediaPage, MediaRecord, Progress},
        AppError, ErrorCode, ImageInfo, ImagePreview, Result,
    },
    services::{
        annotation::{atomic_file, invalid},
        media,
        project::ProjectSession,
    },
};
use base64::Engine;
use rusqlite::{params, OptionalExtension};
use std::{
    collections::HashSet,
    fs,
    io::Cursor,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

pub fn record(session: &ProjectSession, id: &str) -> Result<MediaRecord> {
    Ok(session.connection.query_row("SELECT id,path,width,height,orientation,content_sha256,review_status,import_mode FROM media WHERE id=?1 AND project_id=?2",params![id,session.project.id],row_record)?)
}
fn row_record(r: &rusqlite::Row<'_>) -> rusqlite::Result<MediaRecord> {
    let path: String = r.get(1)?;
    Ok(MediaRecord {
        id: r.get(0)?,
        name: Path::new(&path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        path,
        width: r.get(2)?,
        height: r.get(3)?,
        orientation: r.get(4)?,
        sha256: r.get(5)?,
        review_status: r.get(6)?,
        import_mode: r.get(7)?,
    })
}
#[derive(Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct LibraryFilter {
    pub event: String,
    pub group: String,
    pub annotated: Option<bool>,
}
pub fn page(
    session: &ProjectSession,
    offset: u32,
    limit: u32,
    query: &str,
    status: &str,
) -> Result<MediaPage> {
    filtered_page(
        session,
        offset,
        limit,
        query,
        status,
        &LibraryFilter::default(),
    )
}
pub fn filtered_page(
    session: &ProjectSession,
    offset: u32,
    limit: u32,
    query: &str,
    status: &str,
    filter: &LibraryFilter,
) -> Result<MediaPage> {
    if filter.group.len() > 512
        || (!filter.event.is_empty()
            && !["positive", "negative", "uncertain", "none"].contains(&filter.event.as_str()))
    {
        return Err(invalid("Invalid advanced filter."));
    }
    if limit == 0
        || limit > 200
        || query.len() > 200
        || (!status.is_empty() && !super::annotation::valid_status(status))
    {
        return Err(invalid("Invalid library filter."));
    }
    let pattern = format!(
        "%{}%",
        query
            .replace('!', "!!")
            .replace('%', "!%")
            .replace('_', "!_")
    );
    let sql="FROM media m LEFT JOIN image_events e ON e.media_id=m.id WHERE m.project_id=?1 AND m.path LIKE ?2 ESCAPE '!' AND (?3='' OR m.review_status=?3) AND (?4='' OR (?4='none' AND e.event_type IS NULL) OR e.event_type=?4) AND (?5='' OR m.split_group_id=?5 OR m.source_video_id=?5) AND (?6 IS NULL OR ?6=EXISTS(SELECT 1 FROM annotations a WHERE a.media_id=m.id))";
    let total = session.connection.query_row(
        &format!("SELECT count(*) {sql}"),
        params![
            session.project.id,
            pattern,
            status,
            filter.event,
            filter.group,
            filter.annotated
        ],
        |r| r.get(0),
    )?;
    let mut stmt=session.connection.prepare(&format!("SELECT m.id,m.path,m.width,m.height,m.orientation,m.content_sha256,m.review_status,m.import_mode {sql} ORDER BY m.created_at,m.id LIMIT ?7 OFFSET ?8"))?;
    let items = stmt
        .query_map(
            params![
                session.project.id,
                pattern,
                status,
                filter.event,
                filter.group,
                filter.annotated,
                limit,
                offset
            ],
            row_record,
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(MediaPage { items, total })
}
pub fn all(session: &ProjectSession) -> Result<Vec<MediaRecord>> {
    let mut statement=session.connection.prepare("SELECT id,path,width,height,orientation,content_sha256,review_status,import_mode FROM media ORDER BY created_at,id")?;
    let records = statement
        .query_map([], row_record)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(records)
}
pub fn resolve(session: &ProjectSession, stored: &str) -> Result<PathBuf> {
    let path = Path::new(stored);
    if !path.is_absolute()
        && path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(invalid("Unsafe relative media path."));
    }
    Ok(if path.is_absolute() {
        path.to_path_buf()
    } else {
        session.root.join(path)
    })
}
pub fn checked_load(
    session: &ProjectSession,
    record: &MediaRecord,
) -> Result<(ImageInfo, image::DynamicImage)> {
    let (mut info, decoded) = media::load(&resolve(session, &record.path)?)?;
    if info.sha256 != record.sha256
        || (info.width, info.height, info.orientation)
            != (record.width, record.height, record.orientation)
    {
        return Err(AppError::new(ErrorCode::InvalidImage,"Linked image changed; annotations were preserved. Restore the original source before editing."));
    }
    info.id = record.id.clone();
    Ok((info, decoded))
}
pub fn select(session: &mut ProjectSession, id: &str) -> Result<ImagePreview> {
    let saved = record(session, id)?;
    let (info, decoded) = checked_load(session, &saved)?;
    let mut encoded = Cursor::new(Vec::new());
    decoded
        .write_to(&mut encoded, image::ImageFormat::Png)
        .map_err(|e| invalid(e.to_string()))?;
    session.connection.execute(
        "UPDATE workspace SET active_media_id=?1 WHERE singleton=1",
        [id],
    )?;
    Ok(ImagePreview {
        info,
        data_url: format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(encoded.into_inner())
        ),
    })
}
fn thumbnail_path(session: &ProjectSession, id: &str) -> Result<PathBuf> {
    uuid::Uuid::parse_str(id).map_err(|_| invalid("Invalid media ID."))?;
    let dir = super::storage::CacheKind::Thumbnails.directory(session)?;
    crate::security::reject_symlink(&dir)?;
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{id}.jpg"));
    crate::security::reject_symlink(&path)?;
    Ok(path)
}
pub fn thumbnail(session: &ProjectSession, id: &str) -> Result<String> {
    let path = thumbnail_path(session, id)?;
    let bytes = if let Ok(bytes) = fs::read(&path) {
        if bytes.len() < 512 * 1024
            && image::load_from_memory(&bytes).is_ok_and(|i| i.width() <= 200 && i.height() <= 140)
        {
            bytes
        } else {
            let (_, decoded) = checked_load(session, &record(session, id)?)?;
            write_thumbnail(&path, &decoded)?
        }
    } else {
        let (_, decoded) = checked_load(session, &record(session, id)?)?;
        write_thumbnail(&path, &decoded)?
    };
    super::storage::trim_cache(
        session,
        super::storage::CacheKind::Thumbnails,
        &path,
        bytes.len() as u64,
    )?;
    Ok(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}
fn write_thumbnail(path: &Path, image: &image::DynamicImage) -> Result<Vec<u8>> {
    let mut output = Cursor::new(Vec::new());
    // Lossy compression is confined to the small library preview, never labels/source.
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, 78)
        .encode_image(&image.thumbnail(200, 140).to_rgb8())
        .map_err(|e| invalid(e.to_string()))?;
    let bytes = output.into_inner();
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    fs::write(&temp, &bytes)?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(temp, path)?;
    let legacy = path.with_extension("png");
    crate::security::reject_symlink(&legacy)?;
    if legacy.is_file() {
        fs::remove_file(legacy)?;
    }
    Ok(bytes)
}
pub fn import_paths(
    session: &mut ProjectSession,
    roots: &[PathBuf],
    copied: bool,
    cancel: &AtomicBool,
    progress: impl FnMut(Progress),
) -> Result<ImportReport> {
    let parameters = super::annotation::json(&(roots, copied))?;
    let job = crate::jobs::start(&session.connection, "import_media", &parameters)?;
    let result = import_inner(session, roots, copied, cancel, &job, progress);
    match &result {
        Ok(report) => crate::jobs::finish(
            &session.connection,
            &job,
            if report.cancelled {
                "cancelled"
            } else {
                "completed"
            },
            None,
        ),
        Err(error) => crate::jobs::finish(
            &session.connection,
            &job,
            "failed",
            Some(&error.to_string()),
        ),
    }
    result
}
fn import_inner(
    session: &mut ProjectSession,
    roots: &[PathBuf],
    copied: bool,
    cancel: &AtomicBool,
    job: &str,
    mut progress: impl FnMut(Progress),
) -> Result<ImportReport> {
    let mut report = ImportReport::default();
    let mut files = Vec::new();
    let mut visited = HashSet::new();
    let mut pending = roots.to_vec();
    while let Some(path) = pending.pop() {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            return Ok(report);
        }
        let metadata = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) => {
                report.warnings.push(format!("{}: {e}", path.display()));
                continue;
            }
        };
        if metadata.file_type().is_symlink() || crate::security::reject_symlink(&path).is_err() {
            report
                .warnings
                .push(format!("Skipped symbolic link: {}", path.display()));
            continue;
        }
        if metadata.is_dir() {
            if path.file_name().is_some_and(|n| n == ".labelflow") {
                continue;
            }
            let canonical = path.canonicalize()?;
            if !visited.insert(canonical) {
                continue;
            }
            for entry in fs::read_dir(path)? {
                match entry {
                    Ok(entry) => pending.push(entry.path()),
                    Err(e) => report.warnings.push(e.to_string()),
                }
            }
        } else if metadata.is_file()
            && path.extension().is_some_and(|e| {
                ["jpg", "jpeg", "png", "webp"]
                    .contains(&e.to_string_lossy().to_lowercase().as_str())
            })
        {
            files.push(path);
        }
        if files.len() + pending.len() > 100_000 {
            return Err(invalid(
                "Import exceeds 100000 entries. Import smaller folders.",
            ));
        }
    }
    files.sort();
    let total = files.len() as u32;
    for (index, source) in files.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        let result = (|| -> Result<bool> {
            let canonical = source.canonicalize()?;
            let relative = canonical
                .strip_prefix(&session.root)
                .unwrap_or(&canonical)
                .to_string_lossy()
                .into_owned();
            let existing: Option<String> = session
                .connection
                .query_row(
                    "SELECT id FROM media WHERE project_id=?1 AND path=?2",
                    params![session.project.id, relative],
                    |r| r.get(0),
                )
                .optional()?;
            if existing.is_some() {
                return Ok(false);
            }
            let (mut info, decoded) = media::load(&canonical)?;
            // Content-addressed managed files also deduplicate across repeated imports.
            let (stored, mode) = if copied {
                let directory = session.root.join("media");
                crate::security::reject_symlink(&directory)?;
                fs::create_dir_all(&directory)?;
                let extension = source
                    .extension()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase();
                let path = directory.join(format!("{}.{}", info.sha256, extension));
                if path != canonical {
                    let bytes = fs::read(&canonical)?;
                    if super::annotation::sha(&bytes) != info.sha256 {
                        return Err(invalid(
                            "Source changed during import; retry after the source is stable.",
                        ));
                    }
                    atomic_file(&path, &bytes)?;
                }
                let stored = path
                    .strip_prefix(&session.root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                (stored, "copied")
            } else {
                (relative, "linked")
            };
            let duplicate: bool = session.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM media WHERE path=?1)",
                [&stored],
                |r| r.get(0),
            )?;
            if duplicate {
                return Ok(false);
            }
            let group = canonical
                .parent()
                .unwrap_or(Path::new(""))
                .to_string_lossy()
                .into_owned();
            let tx = session.connection.transaction()?;
            tx.execute("INSERT INTO media(id,project_id,type,path,import_mode,content_sha256,width,height,orientation,split_group_id) VALUES(?1,?2,'image',?3,?4,?5,?6,?7,?8,?9)",params![info.id,session.project.id,stored,mode,info.sha256,info.width,info.height,info.orientation,group])?;
            tx.commit()?;
            info.path = stored;
            let cache = thumbnail_path(session, &info.id)?;
            if let Err(e) = write_thumbnail(&cache, &decoded).and_then(|bytes| {
                super::storage::trim_cache(
                    session,
                    super::storage::CacheKind::Thumbnails,
                    &cache,
                    bytes.len() as u64,
                )
            }) {
                report
                    .warnings
                    .push(format!("Thumbnail can be rebuilt: {e}"));
            }
            Ok(true)
        })();
        match result {
            Ok(true) => report.imported += 1,
            Ok(false) => report.duplicates += 1,
            Err(e) => report.warnings.push(format!("{}: {e}", source.display())),
        }
        crate::jobs::progress(&session.connection, job, index as u32 + 1, total);
        progress(Progress {
            completed: index as u32 + 1,
            total,
            message: source
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        });
    }
    Ok(report)
}
