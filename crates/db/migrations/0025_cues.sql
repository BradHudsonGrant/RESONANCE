-- Points the DJ marked on a track. `position_s` is track time, which each file maps onto its
-- own samples through `asset.offset_samples` and `asset.tempo_ratio`, so a cue stays on its
-- beat when the file is replaced. `carried_over_check` is set when a replace moved the cue onto
-- a new file and the DJ has not looked at it since.
CREATE TABLE cue (
    id INTEGER PRIMARY KEY,
    track_id INTEGER NOT NULL REFERENCES track (id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('hot', 'memory')),
    position_s REAL NOT NULL CHECK (position_s >= 0.0),
    carried_over_check INTEGER NOT NULL DEFAULT 0 CHECK (carried_over_check IN (0, 1))
) STRICT;

CREATE INDEX cue_track ON cue (track_id, position_s);
