-- Files an older reader could not open have no length. Clearing their stamp makes the next
-- scan read them once more with the current reader. Only files still in the library are
-- touched.
UPDATE asset SET modified_ns = -1
WHERE duration_ms IS NULL AND included = 1 AND missing_since IS NULL;
