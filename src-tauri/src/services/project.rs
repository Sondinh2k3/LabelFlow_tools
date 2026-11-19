use crate::{
    database,
    domain::{AppError, ErrorCode, Manifest, Project, Result, FORMAT_VERSION, SCHEMA_VERSION},
    repositories,
    security::{metadata_dir, ProjectLock},
};
use rusqlite::Connection;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub struct ProjectSession {
    pub(crate) cache_checks: std::cell::RefCell<
        std::collections::HashMap<super::storage::CacheKind, (std::time::Instant, u64)>,
    >,
    pub project: Project,
    pub root: PathBuf,
    pub connection: Connection,
    _lock: ProjectLock,
    pub pending_yolo: Option<super::interchange::PreparedYolo>,
    pub pending_mask: Option<super::interchange::PreparedMask>,
    pub pending_mask_folder: Option<crate::import::masks::PreparedMaskFolder>,
    pub pending_template: Option<crate::import::masks::PreparedTemplate>,
}

pub fn validate_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 120 || name.chars().any(char::is_control) {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "Project name must contain 1–120 characters without control characters.",
        ));
    }
    Ok(name.to_owned())
}

pub fn create(root: &Path, name: &str) -> Result<ProjectSession> {
    create_with_profile(root, name, "generic-v1")
}
pub fn create_with_profile(root: &Path, name: &str, profile_id: &str) -> Result<ProjectSession> {
    create_with_classes(root, name, profile_id, None)
}
/// `semantic` optionally replaces the default Cityscapes semantic classes (one name per line).
pub fn create_with_classes(
    root: &Path,
    name: &str,
    profile_id: &str,
    semantic: Option<&[String]>,
) -> Result<ProjectSession> {
    if let Some(lines) = semantic {
        // Validate before touching the folder so a bad list leaves nothing behind.
        super::profile::custom_semantic(lines)?;
    }
    if !["generic-v1", "road-encroachment-v1"].contains(&profile_id) {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "Unknown project profile.",
        ));
    }
    let name = validate_name(name)?;
    let root = root.canonicalize()?;
    let metadata = metadata_dir(&root)?;
    // Exclusive directory creation refuses existing and interrupted projects; never overwrite.
    fs::create_dir(&metadata).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            AppError::new(ErrorCode::InvalidProject, "This folder already contains .labelflow metadata. Open it or choose another folder.")
        } else { e.into() }
    })?;
    let lock = ProjectLock::acquire(&metadata)?;
    for folder in ["thumbnails", "masks", "backups", "logs"] {
        fs::create_dir(metadata.join(folder))?;
    }
    let project = Project {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        profile_id: profile_id.into(),
        schema_version: SCHEMA_VERSION,
        root: root.to_string_lossy().into_owned(),
    };
    let mut connection = database::connect(&metadata.join("project.sqlite"), true)?;
    database::migrate(&mut connection, &metadata)?;
    repositories::insert_project(&connection, &project)?;
    super::profile::ensure_with(&mut connection, &project.id, profile_id, semantic)?;
    connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
    let manifest = Manifest {
        format_version: FORMAT_VERSION,
        id: project.id.clone(),
        name: project.name.clone(),
        profile_id: project.profile_id.clone(),
    };
    let bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|e| AppError::new(ErrorCode::Internal, e.to_string()))?;
    let temporary = metadata.join("project.json.tmp");
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(temporary, metadata.join("project.json"))?;
    Ok(ProjectSession {
        cache_checks: Default::default(),
        project,
        root,
        connection,
        _lock: lock,
        pending_yolo: None,
        pending_mask: None,
        pending_mask_folder: None,
        pending_template: None,
    })
}

pub fn open(root: &Path) -> Result<ProjectSession> {
    let root = root.canonicalize()?;
    let metadata = metadata_dir(&root)?;
    // Refuse missing files rather than creating an empty replacement database.
    if !metadata.join("project.json").is_file() || !metadata.join("project.sqlite").is_file() {
        return Err(AppError::new(ErrorCode::InvalidProject, "Project manifest or database is missing. Select the project root folder; preserve interrupted projects for recovery."));
    }
    let lock = ProjectLock::acquire(&metadata)?;
    use std::io::Read;
    let mut manifest_bytes = Vec::new();
    fs::File::open(metadata.join("project.json"))?
        .take(64 * 1024 + 1)
        .read_to_end(&mut manifest_bytes)?;
    if manifest_bytes.len() > 64 * 1024 {
        return Err(AppError::new(
            ErrorCode::InvalidProject,
            "Project manifest is too large.",
        ));
    }
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes).map_err(|e| {
        AppError::new(
            ErrorCode::InvalidProject,
            format!("Invalid project manifest: {e}"),
        )
    })?;
    if manifest.format_version != FORMAT_VERSION {
        return Err(AppError::new(
            ErrorCode::UnsupportedVersion,
            "Unsupported project format. Use a compatible LabelFlow version.",
        ));
    }
    validate_name(&manifest.name)?;
    uuid::Uuid::parse_str(&manifest.id)
        .map_err(|_| AppError::new(ErrorCode::InvalidProject, "Invalid project identifier."))?;
    let mut connection = database::connect(&metadata.join("project.sqlite"), false)?;
    database::migrate(&mut connection, &metadata)?;
    let project = repositories::project(&connection, root.to_string_lossy().into_owned())?;
    let count: i64 = connection.query_row("SELECT count(*) FROM projects", [], |r| r.get(0))?;
    if count != 1
        || project.id != manifest.id
        || project.name != manifest.name
        || project.profile_id != manifest.profile_id
        || project.schema_version != SCHEMA_VERSION
    {
        return Err(AppError::new(
            ErrorCode::InvalidProject,
            "Manifest and database disagree. Preserve the files and restore a consistent backup.",
        ));
    }
    let violations: i64 =
        connection.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
            r.get(0)
        })?;
    if violations != 0 {
        return Err(AppError::new(
            ErrorCode::Database,
            "Database contains invalid references.",
        ));
    }
    super::profile::ensure(&mut connection, &project.id, &project.profile_id)?;
    crate::jobs::recover(&connection)?;
    Ok(ProjectSession {
        cache_checks: Default::default(),
        project,
        root,
        connection,
        _lock: lock,
        pending_yolo: None,
        pending_mask: None,
        pending_mask_folder: None,
        pending_template: None,
    })
}
