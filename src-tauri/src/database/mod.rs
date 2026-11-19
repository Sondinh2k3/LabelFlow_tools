use crate::domain::{AppError, ErrorCode, Result, SCHEMA_VERSION};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

pub fn connect(path: &Path, create: bool) -> Result<Connection> {
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
        | if create {
            OpenFlags::SQLITE_OPEN_CREATE
        } else {
            OpenFlags::empty()
        };
    let connection = Connection::open_with_flags(path, flags)?;
    connection.busy_timeout(std::time::Duration::from_secs(2))?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    Ok(connection)
}

pub fn migrate(connection: &mut Connection, metadata: &Path) -> Result<()> {
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(AppError::new(
            ErrorCode::UnsupportedVersion,
            "This database requires a newer version of LabelFlow.",
        ));
    }
    let integrity: String = connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(AppError::new(
            ErrorCode::Database,
            "Database integrity check failed. Preserve the project and restore a backup.",
        ));
    }
    if version < SCHEMA_VERSION {
        let count: i64 = connection.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table'",
            [],
            |r| r.get(0),
        )?;
        if count > 0 {
            let backups = metadata.join("backups");
            crate::security::reject_symlink(&backups)?;
            std::fs::create_dir_all(&backups)?;
            let destination =
                backups.join(format!("schema-{version}-{}.sqlite", uuid::Uuid::new_v4()));
            connection.backup("main", &destination, None)?;
        }
        connection.pragma_update(None, "foreign_keys", "OFF")?;
        let result = (|| -> Result<()> {
            let tx = connection.transaction()?;
            if version < 1 {
                tx.execute_batch(include_str!("../../migrations/0001_foundation.sql"))?;
            }
            if version < 2 {
                tx.execute_batch(include_str!("../../migrations/0002_manual_labeling.sql"))?;
            }
            if version < 3 {
                tx.execute_batch(include_str!(
                    "../../migrations/0003_video_interoperability.sql"
                ))?;
            }
            if version < 4 {
                tx.execute_batch(include_str!("../../migrations/0004_ai_suggestions.sql"))?;
            }
            if version < 5 {
                tx.execute_batch(include_str!("../../migrations/0005_road_review.sql"))?;
            }
            if version < 6 {
                tx.execute_batch(include_str!("../../migrations/0006_advanced_workflows.sql"))?;
            }
            let violations: i64 =
                tx.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
                    r.get(0)
                })?;
            if violations != 0 {
                return Err(AppError::new(
                    ErrorCode::Database,
                    "Migration would create invalid references.",
                ));
            }
            tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            tx.commit()?;
            Ok(())
        })();
        connection.pragma_update(None, "foreign_keys", "ON")?;
        result?;
    }
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "FULL")?;
    Ok(())
}
