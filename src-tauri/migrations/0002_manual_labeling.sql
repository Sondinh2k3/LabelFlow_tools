-- foreign_keys is disabled by the migration runner only for this transactional rebuild.
CREATE TABLE media_v2 (
    id TEXT PRIMARY KEY NOT NULL, project_id TEXT NOT NULL REFERENCES projects(id),
    type TEXT NOT NULL CHECK(type='image'), path TEXT NOT NULL,
    import_mode TEXT NOT NULL CHECK(import_mode IN ('linked','copied')),
    content_sha256 TEXT NOT NULL CHECK(length(content_sha256)=64),
    width INTEGER NOT NULL CHECK(width>0), height INTEGER NOT NULL CHECK(height>0),
    orientation INTEGER NOT NULL CHECK(orientation BETWEEN 1 AND 8),
    review_status TEXT NOT NULL DEFAULT 'unlabeled' CHECK(review_status IN ('unlabeled','in_progress','needs_review','approved','rejected')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    timestamp TEXT, gps_json TEXT CHECK(gps_json IS NULL OR json_valid(gps_json)),
    source_video_id TEXT REFERENCES media(id), frame_index INTEGER CHECK(frame_index>=0),
    frame_time_ms REAL CHECK(frame_time_ms>=0), split_group_id TEXT NOT NULL DEFAULT '',
    UNIQUE(project_id,path)
);
INSERT INTO media_v2(id,project_id,type,path,import_mode,content_sha256,width,height,orientation,review_status,created_at)
 SELECT id,project_id,type,path,import_mode,content_sha256,width,height,orientation,review_status,created_at FROM media;
DROP TABLE media;
ALTER TABLE media_v2 RENAME TO media;
CREATE INDEX media_project ON media(project_id,created_at,id);
CREATE INDEX media_review ON media(project_id,review_status,id);
CREATE TABLE classes (
 id TEXT PRIMARY KEY NOT NULL, project_id TEXT NOT NULL REFERENCES projects(id), namespace TEXT NOT NULL,
 stable_key TEXT NOT NULL, export_id INTEGER NOT NULL CHECK(export_id BETWEEN 0 AND 255),
 display_name TEXT NOT NULL, color TEXT NOT NULL, is_ignore INTEGER NOT NULL CHECK(is_ignore IN (0,1)),
 sort_order INTEGER NOT NULL, UNIQUE(project_id,namespace,stable_key), UNIQUE(project_id,namespace,export_id)
);
CREATE TABLE layers (
 id TEXT PRIMARY KEY NOT NULL, project_id TEXT NOT NULL REFERENCES projects(id), stable_key TEXT NOT NULL,
 layer_type TEXT NOT NULL CHECK(layer_type IN ('instance','semantic','region')), display_name TEXT NOT NULL,
 namespace TEXT, color TEXT NOT NULL, visible INTEGER NOT NULL DEFAULT 1 CHECK(visible IN (0,1)),
 opacity REAL NOT NULL DEFAULT 0.45 CHECK(opacity BETWEEN 0 AND 1), export_role TEXT NOT NULL,
 UNIQUE(project_id,stable_key)
);
CREATE TABLE annotation_documents (
 media_id TEXT PRIMARY KEY NOT NULL REFERENCES media(id), revision INTEGER NOT NULL CHECK(revision>=1),
 state_json TEXT NOT NULL CHECK(json_valid(state_json)), updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE TABLE mask_assets (
 id TEXT PRIMARY KEY NOT NULL, relative_path TEXT NOT NULL UNIQUE, sha256 TEXT NOT NULL UNIQUE CHECK(length(sha256)=64),
 width INTEGER NOT NULL CHECK(width>0), height INTEGER NOT NULL CHECK(height>0),
 encoding TEXT NOT NULL CHECK(encoding='png8'), source_sha256 TEXT NOT NULL CHECK(length(source_sha256)=64), created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX mask_source ON mask_assets(source_sha256);
CREATE TABLE annotations (
 id TEXT PRIMARY KEY NOT NULL, media_id TEXT NOT NULL REFERENCES media(id), layer_id TEXT NOT NULL REFERENCES layers(id),
 class_id TEXT REFERENCES classes(id), geometry_type TEXT NOT NULL CHECK(geometry_type IN ('bbox','polygon','semantic_layer','region')),
 geometry_json TEXT NOT NULL CHECK(json_valid(geometry_json)), mask_asset_id TEXT REFERENCES mask_assets(id),
 source TEXT NOT NULL CHECK(source IN ('manual','imported','model_suggestion','model_approved')),
 confidence REAL CHECK(confidence BETWEEN 0 AND 1), review_status TEXT NOT NULL CHECK(review_status IN ('unlabeled','in_progress','needs_review','approved','rejected')),
 revision INTEGER NOT NULL CHECK(revision>=1), created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
 updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX annotations_media ON annotations(media_id,layer_id);
CREATE TABLE image_events (id TEXT PRIMARY KEY NOT NULL, media_id TEXT NOT NULL UNIQUE REFERENCES media(id), event_type TEXT NOT NULL CHECK(event_type IN ('positive','negative','uncertain')), notes TEXT NOT NULL, reviewed_at TEXT);
CREATE TABLE media_tags (id TEXT PRIMARY KEY NOT NULL, media_id TEXT NOT NULL REFERENCES media(id), tag TEXT NOT NULL, UNIQUE(media_id,tag));
CREATE TABLE dataset_splits (media_id TEXT PRIMARY KEY NOT NULL REFERENCES media(id), split_name TEXT NOT NULL CHECK(split_name IN ('train','val','test')), split_group_id TEXT NOT NULL, assignment_method TEXT NOT NULL, seed TEXT NOT NULL);
CREATE TABLE jobs (id TEXT PRIMARY KEY NOT NULL, job_type TEXT NOT NULL, params_json TEXT NOT NULL CHECK(json_valid(params_json)), status TEXT NOT NULL CHECK(status IN ('pending','running','completed','failed','cancelled')), progress REAL NOT NULL CHECK(progress BETWEEN 0 AND 1), error TEXT, created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')), updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')));
CREATE TABLE annotation_operations (id TEXT PRIMARY KEY NOT NULL, media_id TEXT NOT NULL REFERENCES media(id), annotation_id TEXT, operation TEXT NOT NULL, before_json TEXT NOT NULL CHECK(json_valid(before_json)), after_json TEXT NOT NULL CHECK(json_valid(after_json)), payload_sha256 TEXT NOT NULL, revision INTEGER NOT NULL CHECK(revision>=1), created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')), UNIQUE(media_id,revision));
CREATE INDEX operation_media ON annotation_operations(media_id,revision);
UPDATE projects SET schema_version=2;
