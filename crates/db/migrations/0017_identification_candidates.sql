ALTER TABLE track_identification ADD COLUMN candidates_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE track_identification ADD COLUMN selected_candidate INTEGER NOT NULL DEFAULT 0;
ALTER TABLE asset ADD COLUMN file_isrc TEXT;
ALTER TABLE asset ADD COLUMN isrc_read INTEGER NOT NULL DEFAULT 0;
