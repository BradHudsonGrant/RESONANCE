ALTER TABLE track ADD COLUMN favourite INTEGER NOT NULL DEFAULT 0 CHECK (favourite IN (0, 1));
ALTER TABLE track ADD COLUMN added_ns INTEGER NOT NULL DEFAULT 0;

UPDATE track
SET added_ns = COALESCE(
    (SELECT max(asset.modified_ns) FROM asset WHERE asset.track_id = track.id),
    0
);

CREATE INDEX track_added_ns ON track (added_ns DESC, id DESC);
CREATE INDEX track_favourite ON track (id) WHERE favourite = 1;

ALTER TABLE asset ADD COLUMN sort_title TEXT NOT NULL COLLATE NOCASE DEFAULT '';
ALTER TABLE asset ADD COLUMN sort_artist TEXT COLLATE NOCASE;
ALTER TABLE asset ADD COLUMN tag_album TEXT COLLATE NOCASE;
ALTER TABLE asset ADD COLUMN tag_genre TEXT COLLATE NOCASE;
ALTER TABLE asset ADD COLUMN tag_label TEXT COLLATE NOCASE;
ALTER TABLE asset ADD COLUMN tag_key TEXT;
ALTER TABLE asset ADD COLUMN sort_key TEXT;
ALTER TABLE asset ADD COLUMN tag_bpm REAL;
ALTER TABLE asset ADD COLUMN tag_year INTEGER;

UPDATE asset
SET sort_title = COALESCE(
        (SELECT nullif(trim(value), '') FROM file_tag WHERE asset_id = asset.id AND field = 'title'),
        asset.path
    ),
    sort_artist = (SELECT nullif(trim(value), '') FROM file_tag WHERE asset_id = asset.id AND field = 'artist'),
    tag_album = (SELECT nullif(trim(value), '') FROM file_tag WHERE asset_id = asset.id AND field = 'album'),
    tag_genre = (SELECT nullif(trim(value), '') FROM file_tag WHERE asset_id = asset.id AND field = 'genre'),
    tag_label = (SELECT nullif(trim(value), '') FROM file_tag WHERE asset_id = asset.id AND field = 'label'),
    tag_key = (SELECT nullif(trim(value), '') FROM file_tag WHERE asset_id = asset.id AND field = 'key'),
    tag_bpm = (
        SELECT CASE
            WHEN CAST(replace(trim(value), ',', '.') AS REAL) > 0.0 THEN CAST(replace(trim(value), ',', '.') AS REAL)
            ELSE NULL
        END
        FROM file_tag WHERE asset_id = asset.id AND field = 'bpm'
    ),
    tag_year = (
        SELECT CASE
            WHEN substr(trim(value), 1, 4) GLOB '[0-9][0-9][0-9][0-9]'
                THEN CAST(substr(trim(value), 1, 4) AS INTEGER)
            ELSE NULL
        END
        FROM file_tag WHERE asset_id = asset.id AND field = 'year'
    );

CREATE INDEX asset_included_source_path ON asset (source_id, path, id) WHERE included = 1;
CREATE INDEX asset_included_track ON asset (track_id, id) WHERE included = 1;
CREATE INDEX asset_included_sort_title ON asset (sort_title, id) WHERE included = 1;
CREATE INDEX asset_included_sort_artist_asc ON asset ((sort_artist IS NULL) ASC, sort_artist ASC, id ASC) WHERE included = 1;
CREATE INDEX asset_included_sort_artist_desc ON asset ((sort_artist IS NULL) ASC, sort_artist DESC, id DESC) WHERE included = 1;
CREATE INDEX asset_included_bpm_asc ON asset ((tag_bpm IS NULL) ASC, tag_bpm ASC, id ASC) WHERE included = 1;
CREATE INDEX asset_included_bpm_desc ON asset ((tag_bpm IS NULL) ASC, tag_bpm DESC, id DESC) WHERE included = 1;
CREATE INDEX asset_included_sort_key_asc ON asset ((sort_key IS NULL) ASC, sort_key ASC, id ASC) WHERE included = 1;
CREATE INDEX asset_included_sort_key_desc ON asset ((sort_key IS NULL) ASC, sort_key DESC, id DESC) WHERE included = 1;
CREATE INDEX asset_included_duration_asc ON asset ((duration_ms IS NULL) ASC, duration_ms ASC, id ASC) WHERE included = 1;
CREATE INDEX asset_included_duration_desc ON asset ((duration_ms IS NULL) ASC, duration_ms DESC, id DESC) WHERE included = 1;
CREATE INDEX asset_included_bpm ON asset (tag_bpm) WHERE included = 1 AND tag_bpm IS NOT NULL;
CREATE INDEX asset_included_year ON asset (tag_year) WHERE included = 1 AND tag_year IS NOT NULL;
CREATE INDEX asset_included_label ON asset (tag_label) WHERE included = 1 AND tag_label IS NOT NULL;
CREATE INDEX asset_included_key ON asset (tag_key) WHERE included = 1 AND tag_key IS NOT NULL;
CREATE INDEX asset_included_format ON asset (format) WHERE included = 1;

CREATE VIRTUAL TABLE asset_fts USING fts5(
    title,
    artist,
    album,
    genre,
    label,
    comment,
    path,
    tokenize = 'unicode61 remove_diacritics 2'
);

INSERT INTO asset_fts (rowid, title, artist, album, genre, label, comment, path)
SELECT
    asset.id,
    asset.sort_title,
    COALESCE(asset.sort_artist, ''),
    COALESCE(asset.tag_album, ''),
    COALESCE(asset.tag_genre, ''),
    COALESCE(asset.tag_label, ''),
    COALESCE((SELECT nullif(trim(value), '') FROM file_tag WHERE asset_id = asset.id AND field = 'comment'), ''),
    asset.path
FROM asset;
