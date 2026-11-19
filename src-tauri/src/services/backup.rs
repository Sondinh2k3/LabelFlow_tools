use crate::{
    domain::Result,
    services::{annotation as a, library, project::ProjectSession},
};
use rusqlite::params;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
pub fn backup(session: &ProjectSession, parent: &Path) -> Result<PathBuf> {
    let destination = parent
        .canonicalize()?
        .join(format!("LabelFlow-backup-{}", uuid::Uuid::new_v4()));
    let metadata = destination.join(".labelflow");
    fs::create_dir_all(metadata.join("masks"))?;
    let db = metadata.join("project.sqlite");
    session.connection.backup("main", &db, None)?;
    let copy = rusqlite::Connection::open(&db)?;
    // A backup references original media locations; masks and annotations are self-contained.
    let mut stmt = copy.prepare("SELECT id,path FROM media")?;
    let paths = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    drop(stmt);
    for (id, path) in paths {
        copy.execute(
            "UPDATE media SET path=?1 WHERE id=?2",
            params![library::resolve(session, &path)?.to_string_lossy(), id],
        )?;
    }
    drop(copy);
    let mut checksums = BTreeMap::new();
    fs::copy(
        session.root.join(".labelflow/project.json"),
        metadata.join("project.json"),
    )?;
    let mut query = session
        .connection
        .prepare("SELECT relative_path,sha256 FROM mask_assets")?;
    let masks = query
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (relative, hash) in masks {
        if relative != format!("masks/{hash}.png") {
            return Err(a::invalid("Unsafe mask backup path."));
        }
        let source = session.root.join(".labelflow").join(&relative);
        crate::security::reject_symlink(&source)?;
        let bytes = fs::read(source)?;
        if a::sha(&bytes) != hash {
            return Err(a::invalid("Cannot back up a corrupt referenced mask."));
        }
        a::atomic_file(&metadata.join(&relative), &bytes)?;
        checksums.insert(relative, hash);
    }
    for name in ["project.json", "project.sqlite"] {
        checksums.insert(name.into(), a::sha(&fs::read(metadata.join(name))?));
    }
    a::atomic_file(
        &metadata.join("backup.json"),
        a::json(&checksums)?.as_bytes(),
    )?;
    Ok(destination)
}
pub fn restore_backup(source: &Path, destination: &Path) -> Result<()> {
    let source = crate::security::metadata_dir(source)?;
    if destination.join(".labelflow").exists() {
        return Err(a::invalid(
            "Restore requires a folder without .labelflow metadata.",
        ));
    }
    let descriptor = source.join("backup.json");
    crate::security::reject_symlink(&descriptor)?;
    if fs::metadata(&descriptor)?.len() > 8 * 1024 * 1024 {
        return Err(a::invalid("Backup manifest too large."));
    }
    let files: BTreeMap<String, String> = a::parse(&fs::read_to_string(descriptor)?)?;
    if !files.contains_key("project.sqlite") || !files.contains_key("project.json") {
        return Err(a::invalid("Incomplete backup."));
    }
    for (relative, hash) in &files {
        if hash.len() != 64
            || !hash.bytes().all(|b| b.is_ascii_hexdigit())
            || !(relative == "project.sqlite"
                || relative == "project.json"
                || relative == &format!("masks/{hash}.png"))
        {
            return Err(a::invalid("Unsafe backup manifest."));
        }
        let path = source.join(relative);
        crate::security::reject_symlink(&path)?;
        if a::sha(&fs::read(path)?) != *hash {
            return Err(a::invalid("Backup checksum mismatch."));
        }
    }
    let metadata = destination.join(".labelflow");
    fs::create_dir(&metadata)?;
    fs::create_dir(metadata.join("masks"))?;
    for relative in files.keys() {
        fs::copy(source.join(relative), metadata.join(relative))?;
    }
    for folder in ["thumbnails", "backups", "logs"] {
        fs::create_dir(metadata.join(folder))?;
    }
    Ok(())
}
