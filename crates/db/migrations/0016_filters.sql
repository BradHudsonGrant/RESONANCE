-- Whether the track shows a title of its own. The sort title falls back to the file name, so it
-- cannot say.
ALTER TABLE asset ADD COLUMN title_tagged INTEGER NOT NULL DEFAULT 0 CHECK (title_tagged IN (0, 1));

UPDATE asset SET title_tagged = COALESCE(
    (SELECT trim(user_tag.value) <> '' FROM user_tag
     WHERE user_tag.track_id = asset.track_id AND user_tag.field = 'title'),
    (SELECT trim(file_tag.value) <> '' FROM file_tag
     WHERE file_tag.asset_id = asset.id AND file_tag.field = 'title'),
    0
);

-- "No genre" and the like read the tracks without a value from an index of just those, instead
-- of walking the whole table.
CREATE INDEX asset_missing_title ON asset (id) WHERE included = 1 AND title_tagged = 0;
CREATE INDEX asset_missing_artist ON asset (id) WHERE included = 1 AND sort_artist IS NULL;
CREATE INDEX asset_missing_album ON asset (id) WHERE included = 1 AND tag_album IS NULL;
CREATE INDEX asset_missing_genre ON asset (id) WHERE included = 1 AND tag_genre IS NULL;
CREATE INDEX asset_missing_label ON asset (id) WHERE included = 1 AND tag_label IS NULL;
CREATE INDEX asset_missing_catno ON asset (id) WHERE included = 1 AND tag_catno IS NULL;
CREATE INDEX asset_missing_year ON asset (id) WHERE included = 1 AND tag_year IS NULL;
CREATE INDEX asset_missing_comment ON asset (id) WHERE included = 1 AND tag_comment IS NULL;
CREATE INDEX asset_missing_composer ON asset (id) WHERE included = 1 AND tag_composer IS NULL;
CREATE INDEX asset_missing_remixer ON asset (id) WHERE included = 1 AND tag_remixer IS NULL;
CREATE INDEX asset_missing_mix ON asset (id) WHERE included = 1 AND tag_mix IS NULL;
CREATE INDEX asset_missing_bpm ON asset (id) WHERE included = 1 AND tag_bpm IS NULL;
CREATE INDEX asset_missing_key ON asset (id) WHERE included = 1 AND tag_key IS NULL;
CREATE INDEX asset_missing_duration ON asset (id) WHERE included = 1 AND duration_ms IS NULL;
CREATE INDEX asset_missing_bitrate ON asset (id) WHERE included = 1 AND bitrate_kbps IS NULL;
CREATE INDEX asset_missing_sample_rate ON asset (id) WHERE included = 1 AND sample_rate IS NULL;
CREATE INDEX asset_missing_bit_depth ON asset (id) WHERE included = 1 AND bit_depth IS NULL;
CREATE INDEX asset_missing_channels ON asset (id) WHERE included = 1 AND channels IS NULL;
CREATE INDEX asset_missing_artwork ON asset (track_id, id) WHERE included = 1 AND artwork_hash IS NULL;

-- The tags without a column of their own that each track lacks, the other half of
-- `asset_tag_value`, so "no album artist" reads a short list instead of testing every track.
CREATE TABLE asset_tag_missing (
    asset_id INTEGER NOT NULL REFERENCES asset (id) ON DELETE CASCADE,
    field TEXT NOT NULL,
    included INTEGER NOT NULL DEFAULT 1 CHECK (included IN (0, 1)),
    PRIMARY KEY (field, asset_id)
) WITHOUT ROWID;

CREATE INDEX asset_tag_missing_asset ON asset_tag_missing (asset_id);

CREATE TRIGGER asset_tag_missing_included AFTER UPDATE OF included ON asset
WHEN new.included IS NOT old.included
BEGIN
    UPDATE asset_tag_missing SET included = new.included WHERE asset_id = new.id;
END;

INSERT INTO asset_tag_missing (asset_id, field, included)
SELECT asset.id, generic.field, asset.included
FROM asset
CROSS JOIN (
    SELECT 'album_artist' AS field UNION ALL SELECT 'track_number' UNION ALL SELECT 'subgenre'
) AS generic
WHERE NOT EXISTS (
    SELECT 1 FROM asset_tag_value
    WHERE asset_tag_value.asset_id = asset.id AND asset_tag_value.field = generic.field
);

-- How often each filter was added and when last, for recommending quick filters. `term` is the
-- filter's key from `crates/core/src/filter.rs`.
CREATE TABLE filter_use (
    term TEXT PRIMARY KEY,
    uses INTEGER NOT NULL CHECK (uses > 0),
    last_used_ms INTEGER NOT NULL
) STRICT, WITHOUT ROWID;

CREATE TABLE quick_filter (
    position INTEGER PRIMARY KEY,
    term TEXT NOT NULL UNIQUE
) STRICT;

-- Recommended filters the user removed.
CREATE TABLE quick_filter_dismissed (
    term TEXT PRIMARY KEY
) STRICT, WITHOUT ROWID;

-- A saved filter set. It holds filters, never tracks, so new tracks that match show in it.
CREATE TABLE smart_view (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL COLLATE NOCASE UNIQUE CHECK (length(trim(name)) > 0),
    position INTEGER NOT NULL,
    search TEXT,
    pill TEXT CHECK (pill IS NULL OR pill IN ('favourites', 'recent')),
    terms TEXT NOT NULL,
    created_ns INTEGER NOT NULL
) STRICT;
