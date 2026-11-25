-- What the identity decode measured about a file's quality. A row means the file was checked,
-- so files identified before this existed are decoded once more to fill it.
CREATE TABLE asset_quality (
    asset_id INTEGER PRIMARY KEY REFERENCES asset (id) ON DELETE CASCADE,
    integrated_lufs REAL,
    loudness_range_lu REAL,
    sample_peak_dbfs REAL,
    true_peak_dbtp REAL,
    crest_low_db REAL,
    crest_mid_db REAL,
    clipped_runs INTEGER NOT NULL,
    dropout_count INTEGER NOT NULL,
    spectral_cutoff_hz REAL,
    checked_ns INTEGER NOT NULL
) STRICT;

-- Each problem the check found, with how sure it is and why, in the words the app shows.
CREATE TABLE quality_flag (
    asset_id INTEGER NOT NULL REFERENCES asset (id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('damaged', 'clipping', 'dropout', 'transcode')),
    certainty TEXT NOT NULL CHECK (certainty IN ('confirmed', 'likely')),
    reason TEXT NOT NULL,
    PRIMARY KEY (asset_id, kind, reason)
) STRICT, WITHOUT ROWID;

CREATE INDEX quality_flag_kind ON quality_flag (kind, asset_id);
