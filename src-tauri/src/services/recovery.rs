use super::{annotation as a, project::ProjectSession};
use crate::domain::Result;
use std::{collections::HashSet, fs};

// Never delete an orphan automatically: it can be evidence of an interrupted save.
pub fn inspect(session: &ProjectSession) -> Result<Vec<String>> {
    let mut warnings = Vec::new();
    let mut known = HashSet::new();
    let mut statement = session
        .connection
        .prepare("SELECT relative_path,sha256 FROM mask_assets")?;
    let rows = statement.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (relative, hash) = row?;
        if relative != format!("masks/{hash}.png")
            || hash.len() != 64
            || !hash.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(a::invalid("Unsafe mask reference in database."));
        }
        known.insert(format!("{hash}.png"));
        let path = session.root.join(".labelflow").join(&relative);
        crate::security::reject_symlink(&path)?;
        match fs::read(&path) {
            Ok(bytes) if a::sha(&bytes)==hash => {},
            Ok(_) => warnings.push(format!("Mask checksum mismatch: {relative}. Restore a complete backup before editing this image.")),
            Err(error) => warnings.push(format!("Missing/unreadable mask {relative}: {error}. Restore a complete backup.")),
        }
    }
    let masks = session.root.join(".labelflow/masks");
    crate::security::reject_symlink(&masks)?;
    if masks.is_dir() {
        for entry in fs::read_dir(masks)? {
            let entry = entry?;
            if !known.contains(&entry.file_name().to_string_lossy().into_owned()) {
                warnings.push(format!(
                    "Unreferenced mask/staging asset preserved for recovery: {}",
                    entry.file_name().to_string_lossy()
                ));
            }
        }
    }
    let mut jobs = session.connection.prepare("SELECT id,error FROM jobs WHERE status='failed' AND error LIKE 'Interrupted by process exit.%'")?;
    for row in jobs.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (id, error) = row?;
        warnings.push(format!("Job {id}: {error}"));
    }
    if !warnings.is_empty() {
        let logs = session.root.join(".labelflow/logs");
        crate::security::reject_symlink(&logs)?;
        fs::create_dir_all(&logs)?;
        a::atomic_file(
            &logs.join(format!("recovery-{}.json", uuid::Uuid::new_v4())),
            &serde_json::to_vec_pretty(&warnings).map_err(|e| a::invalid(e.to_string()))?,
        )?;
    }
    Ok(warnings)
}
