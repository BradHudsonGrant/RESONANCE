ALTER TABLE source ADD COLUMN include_subfolders INTEGER NOT NULL DEFAULT 1 CHECK (include_subfolders IN (0, 1));
ALTER TABLE source ADD COLUMN watch INTEGER NOT NULL DEFAULT 1 CHECK (watch IN (0, 1));

CREATE TABLE source_excluded_folder (
    source_id INTEGER NOT NULL REFERENCES source (id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    PRIMARY KEY (source_id, path)
) STRICT, WITHOUT ROWID;

CREATE TABLE ignored_folder (
    position INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
) STRICT;

INSERT INTO ignored_folder (position, name) VALUES
    (0, '_replaced'),
    (1, '_wrong_match'),
    (2, '_trash'),
    (3, '_converted_originals');

ALTER TABLE asset ADD COLUMN included INTEGER NOT NULL DEFAULT 1 CHECK (included IN (0, 1));

CREATE INDEX asset_included_source ON asset (source_id) WHERE included = 1;
