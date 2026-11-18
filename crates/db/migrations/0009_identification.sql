CREATE TABLE provider_cache (
    provider TEXT NOT NULL,
    request_hash BLOB NOT NULL CHECK (length(request_hash) = 32),
    status INTEGER NOT NULL,
    body TEXT NOT NULL,
    fetched_ns INTEGER NOT NULL,
    PRIMARY KEY (provider, request_hash)
) STRICT, WITHOUT ROWID;

CREATE TABLE track_identification (
    track_id INTEGER PRIMARY KEY REFERENCES track (id) ON DELETE CASCADE,
    online INTEGER NOT NULL CHECK (online IN (0, 1)),
    confidence REAL,
    audio_matched INTEGER NOT NULL CHECK (audio_matched IN (0, 1)),
    known_json TEXT NOT NULL,
    best_json TEXT,
    steps_json TEXT NOT NULL,
    updated_ns INTEGER NOT NULL
) STRICT;
