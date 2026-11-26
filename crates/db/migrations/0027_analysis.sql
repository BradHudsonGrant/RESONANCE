-- One row per analyser run on a recording. The key is the audio hash, not the file, so renaming,
-- moving, retagging or converting a file between lossless containers finds the run again. A new
-- analyser version or a changed setting gives a new key and runs again.
CREATE TABLE analysis_run (
    audio_hash BLOB NOT NULL CHECK (length(audio_hash) = 32),
    analyser TEXT NOT NULL,
    version TEXT NOT NULL,
    param_hash BLOB NOT NULL CHECK (length(param_hash) = 32),
    status TEXT NOT NULL CHECK (status IN ('done', 'failed')),
    confidence REAL CHECK (confidence IS NULL OR (confidence >= 0.0 AND confidence <= 1.0)),
    completed_ns INTEGER NOT NULL,
    PRIMARY KEY (audio_hash, analyser, version, param_hash)
) STRICT, WITHOUT ROWID;

-- Automatic results are keyed by audio hash and hold times in that decoded audio. Corrections
-- are keyed by track and hold track time, so they stay with the recording when its file is
-- replaced, and a new automatic result never touches them. A correction wins over a result.
-- Key corrections are the user layer of the key tag (ADR 0018), and loudness is measured per
-- file in `asset_quality`, so neither has a table here.

-- `beats` is little-endian f64 seconds. `first_downbeat` indexes bar 1, beat 1 in it.
CREATE TABLE beat_grid_auto (
    audio_hash BLOB PRIMARY KEY CHECK (length(audio_hash) = 32),
    bpm REAL NOT NULL CHECK (bpm > 0.0),
    beats BLOB NOT NULL CHECK (length(beats) % 8 = 0),
    first_downbeat INTEGER NOT NULL CHECK (first_downbeat >= 0),
    confidence REAL NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0)
) STRICT;

CREATE TABLE beat_grid_user (
    track_id INTEGER PRIMARY KEY REFERENCES track (id) ON DELETE CASCADE,
    bpm REAL NOT NULL CHECK (bpm > 0.0),
    beats BLOB NOT NULL CHECK (length(beats) % 8 = 0),
    first_downbeat INTEGER NOT NULL CHECK (first_downbeat >= 0),
    edited_ns INTEGER NOT NULL
) STRICT;

-- `key` is a Camelot code such as 8A.
CREATE TABLE key_auto (
    audio_hash BLOB PRIMARY KEY CHECK (length(audio_hash) = 32),
    key TEXT NOT NULL,
    tuning_cents REAL NOT NULL CHECK (tuning_cents >= -50.0 AND tuning_cents <= 50.0),
    confidence REAL NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0)
) STRICT;

-- `curve` holds one energy byte per bar.
CREATE TABLE energy_auto (
    audio_hash BLOB PRIMARY KEY CHECK (length(audio_hash) = 32),
    curve BLOB NOT NULL,
    confidence REAL NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0)
) STRICT;

-- `bars` holds one 48-byte bar summary per bar, in bar order.
CREATE TABLE bar_features_auto (
    audio_hash BLOB PRIMARY KEY CHECK (length(audio_hash) = 32),
    bars BLOB NOT NULL CHECK (length(bars) % 48 = 0)
) STRICT;

CREATE TABLE phrase_auto (
    audio_hash BLOB NOT NULL CHECK (length(audio_hash) = 32),
    start_bar INTEGER NOT NULL CHECK (start_bar >= 0),
    bar_count INTEGER NOT NULL CHECK (bar_count > 0),
    role TEXT NOT NULL CHECK (role IN ('intro', 'buildup', 'drop', 'breakdown', 'cooldown', 'outro')),
    confidence REAL NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    PRIMARY KEY (audio_hash, start_bar)
) STRICT, WITHOUT ROWID;

CREATE TABLE phrase_user (
    track_id INTEGER NOT NULL REFERENCES track (id) ON DELETE CASCADE,
    start_bar INTEGER NOT NULL CHECK (start_bar >= 0),
    bar_count INTEGER NOT NULL CHECK (bar_count > 0),
    role TEXT NOT NULL CHECK (role IN ('intro', 'buildup', 'drop', 'breakdown', 'cooldown', 'outro')),
    PRIMARY KEY (track_id, start_bar)
) STRICT, WITHOUT ROWID;

-- Cues analysis proposes. The user's own cues are the `cue` table, in track time.
CREATE TABLE cue_proposed (
    id INTEGER PRIMARY KEY,
    audio_hash BLOB NOT NULL CHECK (length(audio_hash) = 32),
    kind TEXT NOT NULL CHECK (
        kind IN ('mix_in', 'groove_in', 'breakdown', 'buildup', 'drop', 'mix_out', 'loop')
    ),
    position_s REAL NOT NULL CHECK (position_s >= 0.0),
    confidence REAL NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0)
) STRICT;

CREATE INDEX cue_proposed_audio ON cue_proposed (audio_hash, position_s);
