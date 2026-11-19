-- Model output never enters annotation_documents until explicit, revision-checked approval.
CREATE TABLE ai_suggestions (
    id TEXT PRIMARY KEY,
    media_id TEXT NOT NULL REFERENCES media(id),
    base_revision INTEGER NOT NULL CHECK(base_revision>=0),
    source_sha256 TEXT NOT NULL,
    prediction_json TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','accepted','rejected')),
    accepted_revision INTEGER,
    operation_id TEXT UNIQUE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    reviewed_at TEXT,
    CHECK((status='accepted' AND accepted_revision IS NOT NULL AND operation_id IS NOT NULL)
       OR (status!='accepted' AND accepted_revision IS NULL AND operation_id IS NULL))
);
CREATE INDEX ai_suggestions_media ON ai_suggestions(media_id,created_at DESC);
UPDATE projects SET schema_version=4;
