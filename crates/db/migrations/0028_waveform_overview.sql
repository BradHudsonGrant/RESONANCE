-- The waveform a table row, the dock and the inspector draw: 140 columns of four bytes, a peak
-- and the low, mid and high band levels. It is keyed by audio hash, like analysis results, so a
-- renamed, moved, retagged or losslessly converted file keeps it. Deck waveforms are drawn at full
-- detail when a track loads, so only this small overview is kept.
CREATE TABLE waveform_overview (
    audio_hash BLOB PRIMARY KEY CHECK (length(audio_hash) = 32),
    columns BLOB NOT NULL CHECK (length(columns) = 560)
) STRICT;
