CREATE TABLE videos (
 id TEXT PRIMARY KEY NOT NULL, project_id TEXT NOT NULL REFERENCES projects(id), path TEXT NOT NULL,
 content_sha256 TEXT NOT NULL CHECK(length(content_sha256)=64), codec TEXT NOT NULL,
 width INTEGER NOT NULL CHECK(width>0), height INTEGER NOT NULL CHECK(height>0),
 rotation INTEGER NOT NULL, time_base_num INTEGER NOT NULL CHECK(time_base_num>0),
 time_base_den INTEGER NOT NULL CHECK(time_base_den>0), start_pts INTEGER NOT NULL,
 duration_ms REAL NOT NULL CHECK(duration_ms>=0), average_fps REAL NOT NULL CHECK(average_fps>=0),
 frame_count INTEGER NOT NULL CHECK(frame_count>0), split_group_id TEXT NOT NULL,
 created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
 UNIQUE(project_id,path), UNIQUE(project_id,content_sha256)
);
CREATE TABLE video_frames (
 video_id TEXT NOT NULL REFERENCES videos(id), frame_index INTEGER NOT NULL CHECK(frame_index>=0),
 pts INTEGER NOT NULL, time_ms REAL NOT NULL CHECK(time_ms>=0), duration_ms REAL NOT NULL CHECK(duration_ms>=0),
 PRIMARY KEY(video_id,frame_index)
);
CREATE INDEX video_frame_seek ON video_frames(video_id,time_ms,frame_index);
-- Rebuild instead of ALTER: schema 2's placeholder source_video_id referenced
-- media(id), while schema 3 stores source videos in their own authoritative table.
CREATE TABLE media_v3 (
 id TEXT PRIMARY KEY NOT NULL, project_id TEXT NOT NULL REFERENCES projects(id),
 type TEXT NOT NULL CHECK(type='image'), path TEXT NOT NULL,
 import_mode TEXT NOT NULL CHECK(import_mode IN ('linked','copied')),
 content_sha256 TEXT NOT NULL CHECK(length(content_sha256)=64),
 width INTEGER NOT NULL CHECK(width>0), height INTEGER NOT NULL CHECK(height>0),
 orientation INTEGER NOT NULL CHECK(orientation BETWEEN 1 AND 8),
 review_status TEXT NOT NULL DEFAULT 'unlabeled' CHECK(review_status IN ('unlabeled','in_progress','needs_review','approved','rejected')),
 created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
 timestamp TEXT, gps_json TEXT CHECK(gps_json IS NULL OR json_valid(gps_json)),
 source_video_id TEXT REFERENCES videos(id), frame_index INTEGER CHECK(frame_index>=0),
 frame_time_ms REAL CHECK(frame_time_ms>=0), split_group_id TEXT NOT NULL DEFAULT '',
 frame_pts INTEGER, extraction_json TEXT CHECK(extraction_json IS NULL OR json_valid(extraction_json)),
 CHECK((source_video_id IS NULL AND frame_index IS NULL AND frame_pts IS NULL) OR (source_video_id IS NOT NULL AND frame_index IS NOT NULL AND frame_pts IS NOT NULL AND frame_time_ms IS NOT NULL)),
 UNIQUE(project_id,path)
);
INSERT INTO media_v3 SELECT *,NULL,NULL FROM media;
DROP TABLE media;
ALTER TABLE media_v3 RENAME TO media;
CREATE INDEX media_project ON media(project_id,created_at,id);
CREATE INDEX media_review ON media(project_id,review_status,id);
CREATE UNIQUE INDEX extracted_frame_identity ON media(source_video_id,frame_index) WHERE source_video_id IS NOT NULL;
CREATE INDEX media_group_review ON media(project_id,split_group_id,review_status);
CREATE TABLE annotations_v3 (
 id TEXT PRIMARY KEY NOT NULL, media_id TEXT NOT NULL REFERENCES media(id), layer_id TEXT NOT NULL REFERENCES layers(id),
 class_id TEXT REFERENCES classes(id), geometry_type TEXT NOT NULL CHECK(geometry_type IN ('bbox','polygon','multiPolygon','mask','semantic_layer','region')),
 geometry_json TEXT NOT NULL CHECK(json_valid(geometry_json)), mask_asset_id TEXT REFERENCES mask_assets(id),
 source TEXT NOT NULL CHECK(source IN ('manual','imported','model_suggestion','model_approved')),
 confidence REAL CHECK(confidence BETWEEN 0 AND 1), review_status TEXT NOT NULL CHECK(review_status IN ('unlabeled','in_progress','needs_review','approved','rejected')),
 revision INTEGER NOT NULL CHECK(revision>=1), created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
 updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
INSERT INTO annotations_v3 SELECT * FROM annotations;
DROP TABLE annotations;
ALTER TABLE annotations_v3 RENAME TO annotations;
CREATE INDEX annotations_media ON annotations(media_id,layer_id);
UPDATE projects SET schema_version=3;
