-- Tags the table can show and sort by that had no column on `asset` yet. Like the others they
-- hold the value the track shows: the user's where there is one, else the file's.
ALTER TABLE asset ADD COLUMN tag_catno TEXT COLLATE NOCASE;
ALTER TABLE asset ADD COLUMN tag_comment TEXT COLLATE NOCASE;
ALTER TABLE asset ADD COLUMN tag_composer TEXT COLLATE NOCASE;
ALTER TABLE asset ADD COLUMN tag_remixer TEXT COLLATE NOCASE;
ALTER TABLE asset ADD COLUMN tag_mix TEXT COLLATE NOCASE;

-- Tags without a column of their own, one row per tag a file has, so sorting by one walks an
-- index instead of reading `file_tag`. `sort_value` pads a leading number so 9 sorts before 10.
CREATE TABLE asset_tag_value (
    asset_id INTEGER NOT NULL REFERENCES asset (id) ON DELETE CASCADE,
    field TEXT NOT NULL,
    sort_value TEXT NOT NULL COLLATE NOCASE,
    PRIMARY KEY (asset_id, field)
) WITHOUT ROWID;

CREATE INDEX asset_tag_value_sort ON asset_tag_value (field, sort_value, asset_id);

-- Every column that can be empty sorts its empty values last in both directions, which needs an
-- index each way. A column that is never empty reads its one index backwards.
CREATE INDEX asset_included_album_asc ON asset ((tag_album IS NULL) ASC, tag_album ASC, id ASC) WHERE included = 1;
CREATE INDEX asset_included_album_desc ON asset ((tag_album IS NULL) ASC, tag_album DESC, id DESC) WHERE included = 1;
CREATE INDEX asset_included_genre_asc ON asset ((tag_genre IS NULL) ASC, tag_genre ASC, id ASC) WHERE included = 1;
CREATE INDEX asset_included_genre_desc ON asset ((tag_genre IS NULL) ASC, tag_genre DESC, id DESC) WHERE included = 1;
CREATE INDEX asset_included_label_asc ON asset ((tag_label IS NULL) ASC, tag_label ASC, id ASC) WHERE included = 1;
CREATE INDEX asset_included_label_desc ON asset ((tag_label IS NULL) ASC, tag_label DESC, id DESC) WHERE included = 1;
CREATE INDEX asset_included_catno_asc ON asset ((tag_catno IS NULL) ASC, tag_catno ASC, id ASC) WHERE included = 1;
CREATE INDEX asset_included_catno_desc ON asset ((tag_catno IS NULL) ASC, tag_catno DESC, id DESC) WHERE included = 1;
CREATE INDEX asset_included_year_asc ON asset ((tag_year IS NULL) ASC, tag_year ASC, id ASC) WHERE included = 1;
CREATE INDEX asset_included_year_desc ON asset ((tag_year IS NULL) ASC, tag_year DESC, id DESC) WHERE included = 1;
CREATE INDEX asset_included_comment_asc ON asset ((tag_comment IS NULL) ASC, tag_comment ASC, id ASC) WHERE included = 1;
CREATE INDEX asset_included_comment_desc ON asset ((tag_comment IS NULL) ASC, tag_comment DESC, id DESC) WHERE included = 1;
CREATE INDEX asset_included_composer_asc ON asset ((tag_composer IS NULL) ASC, tag_composer ASC, id ASC) WHERE included = 1;
CREATE INDEX asset_included_composer_desc ON asset ((tag_composer IS NULL) ASC, tag_composer DESC, id DESC) WHERE included = 1;
CREATE INDEX asset_included_remixer_asc ON asset ((tag_remixer IS NULL) ASC, tag_remixer ASC, id ASC) WHERE included = 1;
CREATE INDEX asset_included_remixer_desc ON asset ((tag_remixer IS NULL) ASC, tag_remixer DESC, id DESC) WHERE included = 1;
CREATE INDEX asset_included_mix_asc ON asset ((tag_mix IS NULL) ASC, tag_mix ASC, id ASC) WHERE included = 1;
CREATE INDEX asset_included_mix_desc ON asset ((tag_mix IS NULL) ASC, tag_mix DESC, id DESC) WHERE included = 1;
CREATE INDEX asset_included_bitrate_asc ON asset ((bitrate_kbps IS NULL) ASC, bitrate_kbps ASC, id ASC) WHERE included = 1;
CREATE INDEX asset_included_bitrate_desc ON asset ((bitrate_kbps IS NULL) ASC, bitrate_kbps DESC, id DESC) WHERE included = 1;
CREATE INDEX asset_included_sample_rate_asc ON asset ((sample_rate IS NULL) ASC, sample_rate ASC, id ASC) WHERE included = 1;
CREATE INDEX asset_included_sample_rate_desc ON asset ((sample_rate IS NULL) ASC, sample_rate DESC, id DESC) WHERE included = 1;
CREATE INDEX asset_included_bit_depth_asc ON asset ((bit_depth IS NULL) ASC, bit_depth ASC, id ASC) WHERE included = 1;
CREATE INDEX asset_included_bit_depth_desc ON asset ((bit_depth IS NULL) ASC, bit_depth DESC, id DESC) WHERE included = 1;
CREATE INDEX asset_included_channels_asc ON asset ((channels IS NULL) ASC, channels ASC, id ASC) WHERE included = 1;
CREATE INDEX asset_included_channels_desc ON asset ((channels IS NULL) ASC, channels DESC, id DESC) WHERE included = 1;
CREATE INDEX asset_included_format_sort ON asset (format, id) WHERE included = 1;
CREATE INDEX asset_included_size ON asset (size_bytes, id) WHERE included = 1;
CREATE INDEX asset_included_modified ON asset (modified_ns, id) WHERE included = 1;

-- The library table as the user last left it. No rows means the default columns.
CREATE TABLE table_column (
    position INTEGER PRIMARY KEY,
    column_id TEXT NOT NULL UNIQUE,
    width_px INTEGER CHECK (width_px IS NULL OR width_px > 0),
    chosen INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE TABLE table_sort (
    position INTEGER PRIMARY KEY,
    column_id TEXT NOT NULL,
    direction TEXT NOT NULL CHECK (direction IN ('asc', 'desc'))
) STRICT;

CREATE TABLE table_setting (
    name TEXT PRIMARY KEY,
    value INTEGER NOT NULL
) STRICT, WITHOUT ROWID;
