use crate::domain::{ImageInfo, Project, Result};
use rusqlite::{params, Connection, OptionalExtension};

pub fn insert_project(connection: &Connection, project: &Project) -> Result<()> {
    connection.execute(
        "INSERT INTO projects(id,name,profile_id,schema_version) VALUES(?1,?2,?3,?4)",
        params![
            project.id,
            project.name,
            project.profile_id,
            project.schema_version
        ],
    )?;
    Ok(())
}
pub fn project(connection: &Connection, root: String) -> Result<Project> {
    Ok(connection.query_row(
        "SELECT id,name,profile_id,schema_version FROM projects",
        [],
        |row| {
            Ok(Project {
                id: row.get(0)?,
                name: row.get(1)?,
                profile_id: row.get(2)?,
                schema_version: row.get(3)?,
                root,
            })
        },
    )?)
}
pub fn remember_image(
    connection: &mut Connection,
    project_id: &str,
    image: &mut ImageInfo,
    stored_path: &str,
) -> Result<()> {
    let tx = connection.transaction()?;
    // Reopening the same linked file preserves its stable media identity.
    let existing: Option<String> = tx
        .query_row(
            "SELECT id FROM media WHERE project_id=?1 AND path=?2",
            params![project_id, stored_path],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        let hash: String =
            tx.query_row("SELECT content_sha256 FROM media WHERE id=?1", [&id], |r| {
                r.get(0)
            })?;
        if hash != image.sha256 {
            return Err(crate::domain::AppError::new(crate::domain::ErrorCode::InvalidImage,"This linked source changed. Existing annotation identity cannot be reassigned silently."));
        }
        image.id = id;
    }
    tx.execute("INSERT INTO media(id,project_id,type,path,import_mode,content_sha256,width,height,orientation) VALUES(?1,?2,'image',?3,'linked',?4,?5,?6,?7) ON CONFLICT(project_id,path) DO UPDATE SET content_sha256=excluded.content_sha256,width=excluded.width,height=excluded.height,orientation=excluded.orientation", params![image.id, project_id, stored_path, image.sha256, image.width, image.height, image.orientation])?;
    tx.execute(
        "UPDATE workspace SET active_media_id=?1 WHERE singleton=1",
        [&image.id],
    )?;
    tx.execute(
        "UPDATE projects SET updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1",
        [project_id],
    )?;
    tx.commit()?;
    Ok(())
}
pub fn active_image(connection: &Connection) -> Result<Option<ImageInfo>> {
    Ok(connection.query_row("SELECT m.id,m.path,m.width,m.height,m.orientation,m.content_sha256 FROM media m JOIN workspace w ON w.active_media_id=m.id WHERE w.singleton=1", [], |r| Ok(ImageInfo { id: r.get(0)?, name: String::new(), path: r.get(1)?, width: r.get(2)?, height: r.get(3)?, orientation: r.get(4)?, sha256: r.get(5)? })).optional()?)
}
