use labelflow_lib::{
    database,
    domain::{ErrorCode, SCHEMA_VERSION},
    security::ProjectLock,
    services::{media, project},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures")
        .join(name)
}

#[test]
fn create_reopen_unicode_project_and_migrate_idempotently() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Dự án đường phố");
    fs::create_dir(&root).unwrap();
    let session = project::create(&root, "  Đường phố  ").unwrap();
    let id = session.project.id.clone();
    assert_eq!(session.project.name, "Đường phố");
    assert_eq!(session.project.schema_version, SCHEMA_VERSION);
    let version: u32 = session
        .connection
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION);
    let fk: u32 = session
        .connection
        .pragma_query_value(None, "foreign_keys", |r| r.get(0))
        .unwrap();
    assert_eq!(fk, 1);
    let mode: String = session
        .connection
        .pragma_query_value(None, "journal_mode", |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
    drop(session);
    let reopened = project::open(&root).unwrap();
    assert_eq!(reopened.project.id, id);
    assert!(media::restore(&reopened).unwrap().is_none());
    assert_eq!(
        fs::read_dir(root.join(".labelflow/backups"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn lock_rejects_second_writer_and_releases_on_drop() {
    let temp = tempfile::tempdir().unwrap();
    let session = project::create(temp.path(), "Lock test").unwrap();
    assert_eq!(
        project::open(temp.path()).err().unwrap().code,
        ErrorCode::ProjectLocked
    );
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "lock_child_process", "--ignored"])
        .env("LABELFLOW_TEST_LOCK_DIR", temp.path().join(".labelflow"))
        .env("LABELFLOW_TEST_LOCK_PROBE", "1")
        .status()
        .unwrap();
    assert!(
        status.success(),
        "A second process must observe the live lock"
    );
    drop(session);
    assert!(project::open(temp.path()).is_ok());
}

#[test]
fn lock_is_released_after_process_exit() {
    let temp = tempfile::tempdir().unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "lock_child_process", "--ignored"])
        .env("LABELFLOW_TEST_LOCK_DIR", temp.path())
        .spawn()
        .unwrap();
    assert!(child.wait().unwrap().success());
    assert!(ProjectLock::acquire(temp.path()).is_ok());
}

#[test]
#[ignore = "spawned only by parent lock tests"]
fn lock_child_process() {
    let path = std::env::var_os("LABELFLOW_TEST_LOCK_DIR").unwrap();
    if std::env::var_os("LABELFLOW_TEST_LOCK_PROBE").is_some() {
        let error = ProjectLock::acquire(Path::new(&path)).err().unwrap();
        assert_eq!(error.code, ErrorCode::ProjectLocked);
        return;
    }
    let _lock = ProjectLock::acquire(Path::new(&path)).unwrap();
    // Simulate abrupt exit: destructors do not run.
    std::process::exit(0);
}

#[test]
fn create_never_overwrites_existing_project() {
    let temp = tempfile::tempdir().unwrap();
    let session = project::create(temp.path(), "First").unwrap();
    drop(session);
    let path = temp.path().join(".labelflow/project.json");
    let before = fs::read(&path).unwrap();
    assert_eq!(
        project::create(temp.path(), "Second").err().unwrap().code,
        ErrorCode::InvalidProject
    );
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn invalid_names_do_not_create_metadata() {
    let temp = tempfile::tempdir().unwrap();
    for name in [" ", "bad\nname", &"x".repeat(121)] {
        assert_eq!(
            project::create(temp.path(), name).err().unwrap().code,
            ErrorCode::InvalidInput
        );
    }
    assert!(!temp.path().join(".labelflow").exists());
}

#[test]
fn missing_or_invalid_database_is_not_replaced() {
    let temp = tempfile::tempdir().unwrap();
    drop(project::create(temp.path(), "Test").unwrap());
    let db = temp.path().join(".labelflow/project.sqlite");
    fs::remove_file(&db).unwrap();
    assert_eq!(
        project::open(temp.path()).err().unwrap().code,
        ErrorCode::InvalidProject
    );
    assert!(!db.exists());
    fs::write(&db, b"corrupted").unwrap();
    assert!(project::open(temp.path()).is_err());
    assert_eq!(fs::read(db).unwrap(), b"corrupted");
}

#[test]
fn future_schema_and_format_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let session = project::create(temp.path(), "Test").unwrap();
    session
        .connection
        .pragma_update(None, "user_version", 999)
        .unwrap();
    drop(session);
    assert_eq!(
        project::open(temp.path()).err().unwrap().code,
        ErrorCode::UnsupportedVersion
    );
    let path = temp.path().join(".labelflow/project.json");
    let mut json: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    json["formatVersion"] = 999.into();
    fs::write(path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert_eq!(
        project::open(temp.path()).err().unwrap().code,
        ErrorCode::UnsupportedVersion
    );
}

#[test]
fn manifest_database_mismatch_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    drop(project::create(temp.path(), "Test").unwrap());
    let path = temp.path().join(".labelflow/project.json");
    let mut json: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    json["name"] = "Changed".into();
    fs::write(path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert_eq!(
        project::open(temp.path()).err().unwrap().code,
        ErrorCode::InvalidProject
    );
}

#[test]
fn migration_backs_up_existing_database_and_rolls_back_on_failure() {
    let temp = tempfile::tempdir().unwrap();
    let mut connection = database::connect(&temp.path().join("old.sqlite"), true).unwrap();
    connection
        .execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE legacy(value TEXT); INSERT INTO legacy VALUES('keep');")
        .unwrap();
    database::migrate(&mut connection, temp.path()).unwrap();
    let backup = fs::read_dir(temp.path().join("backups"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let restored = rusqlite::Connection::open(backup).unwrap();
    assert_eq!(
        restored
            .query_row("SELECT value FROM legacy", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "keep"
    );
    assert_eq!(
        restored
            .pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        0
    );
    let mut bad = database::connect(&temp.path().join("bad.sqlite"), true).unwrap();
    bad.execute_batch("CREATE TABLE media(id TEXT)").unwrap();
    assert!(database::migrate(&mut bad, temp.path()).is_err());
    assert_eq!(
        bad.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        bad.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='projects'",
            [],
            |r| r.get::<_, u32>(0)
        )
        .unwrap(),
        0
    );
}

#[test]
fn database_constraints_reject_invalid_dimensions_and_references() {
    let temp = tempfile::tempdir().unwrap();
    let session = project::create(temp.path(), "Test").unwrap();
    assert!(session
        .connection
        .execute("UPDATE workspace SET active_media_id='missing'", [])
        .is_err());
    assert!(session.connection.execute("INSERT INTO media(id,project_id,type,path,import_mode,content_sha256,width,height,orientation) VALUES('x',?1,'image','p','linked',?2,0,1,1)", rusqlite::params![session.project.id, "0".repeat(64)]).is_err());
}

#[test]
fn image_1080p_is_linked_and_restored_without_modifying_source() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("Ảnh đường phố.png");
    fs::copy(fixture("calibration-1920x1080.png"), &source).unwrap();
    let before = fs::read(&source).unwrap();
    let mut session = project::create(temp.path(), "Test").unwrap();
    let preview = media::open(&mut session, &source).unwrap();
    assert_eq!((preview.info.width, preview.info.height), (1920, 1080));
    assert_eq!(preview.info.orientation, 1);
    let id = preview.info.id.clone();
    let again = media::open(&mut session, &source).unwrap();
    assert_eq!(again.info.id, id);
    let stored: String = session
        .connection
        .query_row("SELECT path FROM media", [], |r| r.get(0))
        .unwrap();
    assert_eq!(stored, "Ảnh đường phố.png");
    drop(session);
    let reopened = project::open(temp.path()).unwrap();
    let restored = media::restore(&reopened).unwrap().unwrap();
    assert_eq!(restored.info, preview.info);
    assert_eq!(fs::read(source).unwrap(), before);
}

#[test]
fn corrupt_missing_and_changed_linked_images_fail_without_losing_reference() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("image.png");
    fs::copy(fixture("calibration-1920x1080.png"), &source).unwrap();
    let mut session = project::create(temp.path(), "Test").unwrap();
    let original = media::open(&mut session, &source).unwrap();
    assert_eq!(
        media::open(&mut session, &fixture("corrupt.png"))
            .unwrap_err()
            .code,
        ErrorCode::InvalidImage
    );
    image::RgbaImage::new(20, 10).save(&source).unwrap();
    assert_eq!(
        media::restore(&session).unwrap_err().code,
        ErrorCode::InvalidImage
    );
    fs::remove_file(&source).unwrap();
    assert_eq!(media::restore(&session).unwrap_err().code, ErrorCode::Io);
    assert_eq!(
        labelflow_lib::repositories::active_image(&session.connection)
            .unwrap()
            .unwrap()
            .id,
        original.info.id
    );
}

#[test]
fn relative_path_traversal_is_rejected_on_restore() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = project::create(temp.path(), "Test").unwrap();
    media::open(&mut session, &fixture("calibration-1920x1080.png")).unwrap();
    session
        .connection
        .execute("UPDATE media SET path='../escape.png'", [])
        .unwrap();
    assert_eq!(
        media::restore(&session).unwrap_err().code,
        ErrorCode::InvalidProject
    );
}

#[test]
fn exif_rotated_jpeg_uses_canonical_dimensions_and_preserves_original() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("portrait.jpg");
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
        .encode(&[255; 3 * 12 * 8], 12, 8, image::ExtendedColorType::Rgb8)
        .unwrap();
    // JPEG APP1: Exif + little-endian TIFF, orientation=6 (90 degrees clockwise).
    let exif: &[u8] = b"Exif\0\0II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0";
    let mut bytes = jpeg[..2].to_vec();
    bytes.extend_from_slice(&[0xff, 0xe1]);
    bytes.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
    bytes.extend_from_slice(exif);
    bytes.extend_from_slice(&jpeg[2..]);
    fs::write(&path, &bytes).unwrap();
    let preview = media::decode(&path).unwrap();
    assert_eq!(
        (
            preview.info.width,
            preview.info.height,
            preview.info.orientation
        ),
        (8, 12, 6)
    );
    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[test]
fn unsupported_and_oversized_images_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("large.png");
    let file = fs::File::create(&path).unwrap();
    file.set_len(media::MAX_FILE_BYTES + 1).unwrap();
    assert_eq!(
        media::decode(&path).unwrap_err().code,
        ErrorCode::InvalidImage
    );
    fs::write(&path, b"GIF89a").unwrap();
    assert_eq!(
        media::decode(&path).unwrap_err().code,
        ErrorCode::InvalidImage
    );
}

#[test]
fn wire_contract_is_camel_case_with_typed_errors() {
    let error = labelflow_lib::domain::AppError::new(ErrorCode::ProjectLocked, "busy");
    assert_eq!(
        serde_json::to_value(error).unwrap(),
        serde_json::json!({"code":"projectLocked","message":"busy"})
    );
    let temp = tempfile::tempdir().unwrap();
    let session = project::create(temp.path(), "Test").unwrap();
    let json = serde_json::to_value(&session.project).unwrap();
    assert_eq!(json["schemaVersion"], SCHEMA_VERSION);
    assert_eq!(json["profileId"], "generic-v1");
    assert!(json.get("schema_version").is_none());
}

#[test]
fn schema_one_upgrade_preserves_media_workspace_and_backup() {
    let tmp = tempfile::tempdir().unwrap();
    let metadata = tmp.path().join(".labelflow");
    fs::create_dir(&metadata).unwrap();
    let project_id = uuid::Uuid::new_v4().to_string();
    let media_id = uuid::Uuid::new_v4().to_string();
    let connection = rusqlite::Connection::open(metadata.join("project.sqlite")).unwrap();
    connection
        .execute_batch(include_str!("../migrations/0001_foundation.sql"))
        .unwrap();
    connection.execute("INSERT INTO projects(id,name,profile_id,schema_version) VALUES(?1,'Legacy','generic-v1',1)", [&project_id]).unwrap();
    connection.execute("INSERT INTO media(id,project_id,type,path,import_mode,content_sha256,width,height,orientation) VALUES(?1,?2,'image','legacy.png','linked',?3,1920,1080,1)", rusqlite::params![media_id,project_id,"0".repeat(64)]).unwrap();
    connection
        .execute("UPDATE workspace SET active_media_id=?1", [&media_id])
        .unwrap();
    connection.pragma_update(None, "user_version", 1).unwrap();
    drop(connection);
    fs::write(metadata.join("project.json"), serde_json::to_vec(&serde_json::json!({"formatVersion":1,"id":project_id,"name":"Legacy","profileId":"generic-v1"})).unwrap()).unwrap();
    let upgraded = project::open(tmp.path()).unwrap();
    assert_eq!(upgraded.project.schema_version, SCHEMA_VERSION);
    let restored = labelflow_lib::repositories::active_image(&upgraded.connection)
        .unwrap()
        .unwrap();
    assert_eq!(restored.id, media_id);
    assert_eq!((restored.width, restored.height), (1920, 1080));
    let path = fs::read_dir(metadata.join("backups"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let backup = rusqlite::Connection::open(path).unwrap();
    assert_eq!(
        backup
            .pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        backup
            .query_row("SELECT id FROM media", [], |r| r.get::<_, String>(0))
            .unwrap(),
        media_id
    );
}
