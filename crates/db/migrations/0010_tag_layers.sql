-- What identification proposes for a field, one row per field. Accepting copies the value into
-- `user_tag`, and a later run only resets a decision when it proposes a different value.
CREATE TABLE proposed_tag (
    track_id INTEGER NOT NULL REFERENCES track (id) ON DELETE CASCADE,
    field TEXT NOT NULL,
    value TEXT NOT NULL,
    provider TEXT NOT NULL,
    confidence REAL NOT NULL,
    audio_matched INTEGER NOT NULL CHECK (audio_matched IN (0, 1)),
    status TEXT NOT NULL CHECK (status IN ('pending', 'accepted', 'skipped')),
    updated_ns INTEGER NOT NULL,
    PRIMARY KEY (track_id, field)
) STRICT, WITHOUT ROWID;

CREATE INDEX proposed_tag_pending ON proposed_tag (track_id) WHERE status = 'pending';

-- What the user typed or accepted. Identification never writes here. An empty value blanks the
-- file's value. `dirty_on_disk` stays set until the value is written into the file.
CREATE TABLE user_tag (
    track_id INTEGER NOT NULL REFERENCES track (id) ON DELETE CASCADE,
    field TEXT NOT NULL,
    value TEXT NOT NULL,
    dirty_on_disk INTEGER NOT NULL DEFAULT 1 CHECK (dirty_on_disk IN (0, 1)),
    updated_ns INTEGER NOT NULL,
    PRIMARY KEY (track_id, field)
) STRICT, WITHOUT ROWID;
