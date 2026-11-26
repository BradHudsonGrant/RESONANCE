-- Another DJ app's library the user set up, by the file it is read from. For M3U that file is
-- the playlist itself.
CREATE TABLE external_library (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('m3u')),
    location TEXT NOT NULL UNIQUE,
    added_ns INTEGER NOT NULL,
    read_ns INTEGER NOT NULL
) STRICT;

-- A collection that mirrors a library's playlist. It is read-only, follows the playlist when
-- it is read again, and goes with the library.
ALTER TABLE collection ADD COLUMN external_library_id INTEGER
    REFERENCES external_library (id) ON DELETE CASCADE;

CREATE UNIQUE INDEX collection_external_library ON collection (external_library_id)
    WHERE external_library_id IS NOT NULL;

-- A file a library still names by a path that a rename or replace has since changed. One row
-- per library and track: `old_path` is the path the library holds, `new_path` the one it
-- should hold, both full paths with forward slashes. A later change to the same track moves
-- `new_path` on, and a change back removes the row.
CREATE TABLE pending_external_change (
    id INTEGER PRIMARY KEY,
    external_library_id INTEGER NOT NULL REFERENCES external_library (id) ON DELETE CASCADE,
    track_id INTEGER NOT NULL REFERENCES track (id) ON DELETE CASCADE,
    old_path TEXT NOT NULL,
    new_path TEXT NOT NULL,
    queued_ns INTEGER NOT NULL,
    UNIQUE (external_library_id, track_id)
) STRICT;
