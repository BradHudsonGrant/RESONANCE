-- Every change the app makes to files on disk, one row per change. `forward_json` is what was
-- asked for and `inverse_json` what it takes to put the files back, filled in file by file as
-- the change runs, so a change that stops half way can still be read.
CREATE TABLE operation (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('pending', 'applied', 'failed', 'undone')),
    summary TEXT NOT NULL,
    forward_json TEXT NOT NULL,
    inverse_json TEXT NOT NULL,
    created_ns INTEGER NOT NULL,
    applied_ns INTEGER,
    undone_ns INTEGER
) STRICT;

CREATE INDEX operation_recent ON operation (created_ns DESC);

CREATE INDEX user_tag_unwritten ON user_tag (track_id) WHERE dirty_on_disk = 1;
