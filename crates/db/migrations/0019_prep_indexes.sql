CREATE INDEX proposed_artwork_pending ON proposed_artwork (track_id) WHERE status = 'pending';
CREATE INDEX user_artwork_dirty ON user_artwork (track_id) WHERE dirty_on_disk = 1;
