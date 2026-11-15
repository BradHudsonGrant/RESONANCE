ALTER TABLE asset ADD COLUMN byte_hash BLOB CHECK (byte_hash IS NULL OR length(byte_hash) = 32);
ALTER TABLE asset ADD COLUMN audio_hash BLOB CHECK (audio_hash IS NULL OR length(audio_hash) = 32);
ALTER TABLE asset ADD COLUMN audio_decoder TEXT;
ALTER TABLE asset ADD COLUMN fingerprint TEXT;
ALTER TABLE asset ADD COLUMN fingerprint_raw BLOB;
ALTER TABLE asset ADD COLUMN fingerprint_duration_s INTEGER;
ALTER TABLE asset ADD COLUMN identity_size_bytes INTEGER;
ALTER TABLE asset ADD COLUMN identity_modified_ns INTEGER;
ALTER TABLE asset ADD COLUMN is_primary INTEGER NOT NULL DEFAULT 1 CHECK (is_primary IN (0, 1));
ALTER TABLE asset ADD COLUMN offset_samples INTEGER NOT NULL DEFAULT 0;
ALTER TABLE asset ADD COLUMN tempo_ratio REAL NOT NULL DEFAULT 1.0 CHECK (tempo_ratio > 0.0);

CREATE INDEX asset_byte_hash ON asset (byte_hash) WHERE byte_hash IS NOT NULL;
CREATE INDEX asset_audio_hash ON asset (audio_hash) WHERE audio_hash IS NOT NULL;
