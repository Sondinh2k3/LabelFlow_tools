use crate::domain::Result;
use rusqlite::{params, Connection};

pub fn start(connection: &Connection, kind: &str, parameters: &str) -> Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    connection.execute(
        "INSERT INTO jobs(id,job_type,params_json,status,progress) VALUES(?1,?2,?3,'running',0)",
        params![id, kind, parameters],
    )?;
    Ok(id)
}
pub fn progress(connection: &Connection, id: &str, completed: u32, total: u32) {
    let fraction = if total == 0 {
        0.0
    } else {
        f64::from(completed) / f64::from(total)
    };
    let _ = connection.execute(
        "UPDATE jobs SET progress=?1,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?2",
        params![fraction.clamp(0.0, 1.0), id],
    );
}
pub fn finish(connection: &Connection, id: &str, status: &str, error: Option<&str>) {
    if let Err(e) = connection.execute("UPDATE jobs SET status=?1,error=?2,progress=CASE WHEN ?1='completed' THEN 1 ELSE progress END,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?3", params![status,error,id]) {
        eprintln!("Could not finalize background job record: {e}");
    }
}
pub fn recover(connection: &Connection) -> Result<()> {
    connection.execute("UPDATE jobs SET status='failed',error='Interrupted by process exit. Re-run import to resume with deduplication; export must be restarted into a new output directory.',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE status IN ('pending','running')",[])?;
    Ok(())
}
