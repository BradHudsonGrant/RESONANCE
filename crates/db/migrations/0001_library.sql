CREATE TABLE source (
    id INTEGER PRIMARY KEY,
    folder TEXT NOT NULL UNIQUE
) STRICT;

CREATE TABLE track (
    id INTEGER PRIMARY KEY
) STRICT;

CREATE TABLE asset (
    id INTEGER PRIMARY KEY,
    track_id INTEGER NOT NULL REFERENCES track (id),
    source_id INTEGER NOT NULL REFERENCES source (id),
    path TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    modified_ns INTEGER NOT NULL,
    format TEXT NOT NULL,
    duration_ms INTEGER,
    sample_rate INTEGER,
    channels INTEGER,
    bit_depth INTEGER,
    bitrate_kbps INTEGER,
    UNIQUE (source_id, path)
) STRICT;

CREATE INDEX asset_track ON asset (track_id);

CREATE TABLE file_tag (
    asset_id INTEGER NOT NULL REFERENCES asset (id) ON DELETE CASCADE,
    field TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (asset_id, field)
) STRICT, WITHOUT ROWID;
