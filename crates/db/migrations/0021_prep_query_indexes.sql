CREATE INDEX IF NOT EXISTS asset_included_primary_track
    ON asset (track_id, is_primary DESC, id)
    WHERE included = 1;

CREATE INDEX IF NOT EXISTS asset_included_non_primary
    ON asset (id)
    WHERE included = 1 AND is_primary = 0;
