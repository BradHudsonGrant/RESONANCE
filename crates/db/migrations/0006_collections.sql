CREATE TABLE collection (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL COLLATE NOCASE UNIQUE CHECK (length(trim(name)) > 0),
    position INTEGER NOT NULL DEFAULT 0,
    created_ns INTEGER NOT NULL
) STRICT;

CREATE TABLE collection_item (
    collection_id INTEGER NOT NULL REFERENCES collection (id) ON DELETE CASCADE,
    track_id INTEGER NOT NULL REFERENCES track (id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    added_ns INTEGER NOT NULL,
    PRIMARY KEY (collection_id, track_id)
) STRICT;

CREATE INDEX collection_item_order ON collection_item (collection_id, position, track_id);
CREATE INDEX collection_item_track ON collection_item (track_id);
