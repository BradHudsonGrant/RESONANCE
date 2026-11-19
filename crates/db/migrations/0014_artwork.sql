-- A picture kept once in the data folder's `artwork` folder, named by the BLAKE3 hash of its
-- bytes and the extension of its kind.
CREATE TABLE artwork (
    hash TEXT PRIMARY KEY CHECK (length(hash) = 64),
    mime TEXT NOT NULL CHECK (mime IN ('image/jpeg', 'image/png')),
    byte_len INTEGER NOT NULL
) STRICT, WITHOUT ROWID;

-- The front cover the file held when it was last read, NULL when it has none. Files scanned
-- before pictures were read have `artwork_read` 0, so the next scan reads them once.
ALTER TABLE asset ADD COLUMN artwork_hash TEXT REFERENCES artwork (hash);
ALTER TABLE asset ADD COLUMN artwork_read INTEGER NOT NULL DEFAULT 0 CHECK (artwork_read IN (0, 1));

-- The picture the user picked or accepted, shown in place of the file's. `dirty_on_disk` stays
-- set until it is written into the file.
CREATE TABLE user_artwork (
    track_id INTEGER PRIMARY KEY REFERENCES track (id) ON DELETE CASCADE,
    artwork_hash TEXT NOT NULL REFERENCES artwork (hash),
    dirty_on_disk INTEGER NOT NULL DEFAULT 1 CHECK (dirty_on_disk IN (0, 1)),
    updated_ns INTEGER NOT NULL
) STRICT;

-- The release picture identification found for a track, waiting for the user's decision.
CREATE TABLE proposed_artwork (
    track_id INTEGER PRIMARY KEY REFERENCES track (id) ON DELETE CASCADE,
    artwork_hash TEXT NOT NULL REFERENCES artwork (hash),
    provider TEXT NOT NULL,
    source_url TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'accepted', 'skipped')),
    updated_ns INTEGER NOT NULL
) STRICT;

CREATE INDEX proposed_artwork_source ON proposed_artwork (source_url);
