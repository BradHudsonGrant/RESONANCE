CREATE TABLE operation_tag_decision (
    operation_id INTEGER NOT NULL REFERENCES operation(id) ON DELETE CASCADE,
    track_id INTEGER NOT NULL REFERENCES track(id) ON DELETE CASCADE,
    field TEXT NOT NULL,
    prior_value TEXT,
    prior_dirty INTEGER NOT NULL DEFAULT 0,
    accepted_value TEXT NOT NULL,
    PRIMARY KEY (operation_id, track_id, field)
) STRICT, WITHOUT ROWID;
