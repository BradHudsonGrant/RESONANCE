-- Set when the user pauses a queued or running job. A paused job is not started, and a running
-- one waits at its next safe point. It stays paused across a restart.
ALTER TABLE job ADD COLUMN paused INTEGER NOT NULL DEFAULT 0 CHECK (paused IN (0, 1));
