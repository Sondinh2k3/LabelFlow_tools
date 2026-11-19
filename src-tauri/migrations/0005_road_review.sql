CREATE TABLE trips (
 id TEXT PRIMARY KEY, name TEXT NOT NULL, route TEXT NOT NULL DEFAULT '', camera_id TEXT NOT NULL DEFAULT '',
 distance_km REAL CHECK(distance_km>0), notes TEXT NOT NULL DEFAULT '', revision INTEGER NOT NULL CHECK(revision>=1)
);
ALTER TABLE videos ADD COLUMN trip_id TEXT REFERENCES trips(id);
ALTER TABLE videos ADD COLUMN trip_revision INTEGER NOT NULL DEFAULT 0;
CREATE TABLE media_review_metadata (
 media_id TEXT PRIMARY KEY REFERENCES media(id), revision INTEGER NOT NULL CHECK(revision>=1),
 trip_id TEXT REFERENCES trips(id), camera_id TEXT NOT NULL DEFAULT '', route TEXT NOT NULL DEFAULT '',
 captured_at TEXT NOT NULL DEFAULT '', latitude REAL CHECK(latitude BETWEEN -90 AND 90),
 longitude REAL CHECK(longitude BETWEEN -180 AND 180),
 CHECK((latitude IS NULL)=(longitude IS NULL))
);
CREATE INDEX review_trip ON media_review_metadata(trip_id,media_id);
CREATE INDEX media_tags_lookup ON media_tags(tag,media_id);
CREATE TABLE review_predictions (
 id TEXT PRIMARY KEY, media_id TEXT NOT NULL REFERENCES media(id), base_revision INTEGER NOT NULL CHECK(base_revision>=0),
 source_sha256 TEXT NOT NULL, prediction_json TEXT NOT NULL CHECK(json_valid(prediction_json)),
 model_name TEXT NOT NULL, model_version TEXT NOT NULL, event_label TEXT CHECK(event_label IN ('positive','negative','uncertain')),
 status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','accepted','rejected')),
 accepted_revision INTEGER, operation_id TEXT UNIQUE,
 created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')), reviewed_at TEXT,
 CHECK((status='accepted' AND accepted_revision IS NOT NULL AND operation_id IS NOT NULL)
    OR (status!='accepted' AND accepted_revision IS NULL AND operation_id IS NULL))
);
CREATE INDEX review_prediction_media ON review_predictions(media_id,created_at DESC,id DESC);
ALTER TABLE ai_suggestions ADD COLUMN model_name TEXT NOT NULL DEFAULT '';
ALTER TABLE ai_suggestions ADD COLUMN model_version TEXT NOT NULL DEFAULT '';
ALTER TABLE ai_suggestions ADD COLUMN confidence REAL;
UPDATE ai_suggestions SET model_name=json_extract(prediction_json,'$.model.name'),
 model_version=json_extract(prediction_json,'$.model.version'),confidence=json_extract(prediction_json,'$.confidence');
CREATE INDEX ai_review_listing ON ai_suggestions(media_id,(status='pending') DESC,created_at DESC,id DESC,base_revision,status,model_name,model_version,confidence,accepted_revision);
CREATE VIEW review_prediction_index AS
 SELECT 'review:'||id AS key,media_id,base_revision,status,model_name,model_version,event_label,created_at FROM review_predictions
 UNION ALL
 SELECT 'ai:'||id,media_id,base_revision,status,model_name,model_version,NULL,created_at FROM ai_suggestions;
CREATE TABLE review_comparisons (
 prediction_key TEXT PRIMARY KEY, media_id TEXT NOT NULL REFERENCES media(id), gt_revision INTEGER NOT NULL,
 disagreement INTEGER NOT NULL CHECK(disagreement IN (0,1)), report_json TEXT NOT NULL CHECK(json_valid(report_json)),
 scored INTEGER NOT NULL DEFAULT 0 CHECK(scored IN (0,1)),
 updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX comparison_filter ON review_comparisons(media_id,gt_revision,disagreement);
UPDATE projects SET schema_version=5;
