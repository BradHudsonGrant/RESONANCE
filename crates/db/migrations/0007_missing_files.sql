ALTER TABLE asset ADD COLUMN missing_since INTEGER;

CREATE INDEX asset_included_missing ON asset (id) WHERE included = 1 AND missing_since IS NOT NULL;
