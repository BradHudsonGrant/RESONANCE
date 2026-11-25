-- Caches fetched external cover URLs to stored artwork hashes (or NULL when the remote
-- address returned 404 or an unsupported format), so candidate previews fetch each URL once
-- without touching per-track cover proposals.
CREATE TABLE artwork_source (
    source_url TEXT PRIMARY KEY,
    artwork_hash TEXT REFERENCES artwork (hash),
    fetched_ns INTEGER NOT NULL
) STRICT, WITHOUT ROWID;
