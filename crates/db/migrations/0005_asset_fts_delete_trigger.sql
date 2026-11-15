-- Standalone FTS5 virtual tables do not follow foreign-key cascades from `asset`.
CREATE TRIGGER trg_asset_delete_fts
AFTER DELETE ON asset
BEGIN
    DELETE FROM asset_fts WHERE rowid = old.id;
END;
