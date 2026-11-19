CREATE TABLE media_fingerprints (
 media_id TEXT PRIMARY KEY REFERENCES media(id),
 source_sha256 TEXT NOT NULL,
 algorithm TEXT NOT NULL,
 dhash TEXT,
 brightness REAL,
 sharpness REAL,
 error TEXT,
 updated_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now')),
 CHECK((error IS NULL AND length(dhash)=16 AND brightness BETWEEN 0 AND 255 AND sharpness>=0) OR error IS NOT NULL)
);
CREATE TABLE propagation_links (
 prediction_id TEXT PRIMARY KEY REFERENCES review_predictions(id),
 track_id TEXT NOT NULL,
 anchor_id TEXT NOT NULL REFERENCES media(id),
 anchor_revision INTEGER NOT NULL CHECK(anchor_revision>0),
 anchor_sha256 TEXT NOT NULL,
 target_id TEXT NOT NULL REFERENCES media(id),
 dx INTEGER NOT NULL,
 dy INTEGER NOT NULL,
 mean_error REAL NOT NULL CHECK(mean_error>=0),
 match_margin REAL NOT NULL CHECK(match_margin>=0),
 skipped_shapes INTEGER NOT NULL DEFAULT 0 CHECK(skipped_shapes>=0),
 verified_revision INTEGER,
 verified_at TEXT,
 CHECK(anchor_id<>target_id)
);
CREATE INDEX propagation_anchor ON propagation_links(anchor_id,anchor_revision);
CREATE TABLE snapshot_objects (
 sha256 TEXT PRIMARY KEY CHECK(length(sha256)=64),
 document_json TEXT NOT NULL
);
CREATE TABLE dataset_snapshots (
 id TEXT PRIMARY KEY CHECK(length(id)=64),
 name TEXT NOT NULL,
 manifest_json TEXT NOT NULL,
 image_count INTEGER NOT NULL CHECK(image_count>0),
 created_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE TRIGGER snapshot_object_update BEFORE UPDATE ON snapshot_objects BEGIN SELECT RAISE(ABORT,'Snapshot objects are immutable'); END;
CREATE TRIGGER snapshot_object_delete BEFORE DELETE ON snapshot_objects BEGIN SELECT RAISE(ABORT,'Snapshot objects are immutable'); END;
CREATE TRIGGER snapshot_update BEFORE UPDATE ON dataset_snapshots BEGIN SELECT RAISE(ABORT,'Dataset snapshots are immutable'); END;
CREATE TRIGGER snapshot_delete BEFORE DELETE ON dataset_snapshots BEGIN SELECT RAISE(ABORT,'Dataset snapshots are immutable'); END;
UPDATE projects SET schema_version=6;
