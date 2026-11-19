-- One row per file a rename operation moved. A row is written in the same transaction as the
-- asset's new path, so the journal and the library always agree, and a batch of any size costs
-- one small insert per file rather than rewriting the operation's JSON.
CREATE TABLE operation_rename (
    operation_id INTEGER NOT NULL REFERENCES operation (id) ON DELETE CASCADE,
    asset_id INTEGER NOT NULL REFERENCES asset (id) ON DELETE CASCADE,
    from_path TEXT NOT NULL,
    to_path TEXT NOT NULL,
    undone INTEGER NOT NULL DEFAULT 0 CHECK (undone IN (0, 1)),
    PRIMARY KEY (operation_id, asset_id)
) STRICT;
