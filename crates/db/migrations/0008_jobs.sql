CREATE TABLE job (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL,
    tier INTEGER NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'completed', 'failed', 'cancelled')),
    label TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    done_items INTEGER NOT NULL DEFAULT 0,
    total_items INTEGER,
    error_message TEXT,
    created_ns INTEGER NOT NULL,
    started_ns INTEGER,
    finished_ns INTEGER
) STRICT;

CREATE INDEX job_status_tier ON job (status, tier, id);
CREATE INDEX job_recent ON job (id DESC);
