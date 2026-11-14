# 0003: Storage

Date: 12 November 2026. Status: accepted as the default.

## Context

The library is 7,268 audio files on the development laptop today and has to work at 100,000. It needs instant text search, filters on any field, a journal that survives a crash, and embedding search for "similar to this". It is one user on one machine.

## Decision

- SQLite in WAL mode through `rusqlite` (0.42.0, MIT), with SQLite bundled so every install runs the same version.
- FTS5 for text search over all tags.
- Embeddings stored as BLOBs and searched in memory by brute force.
- The database lives in the app data folder, found through a platform-folders library. It is never inside a music folder.
- Migrations are numbered SQL files that only go forward. A test opens a database at every earlier schema version and migrates it.
- One connection writes. Readers have their own connections.
- Secrets are not stored here. API tokens go in the OS credential store.

## Why

SQLite needs no server, handles this size comfortably, and WAL lets the UI read while a scan writes. FTS5 is built in.

A vector database is unnecessary at this scale. Brute-force search over 100,000 embeddings in memory takes tens of milliseconds, which is inside the 50 ms search budget once filters have cut the set down.

The rules about migrations come from the old app. It described its schema as a target shape, patched older databases with "add this column if it is missing", and dropped and rebuilt tables when their shape changed. It stamped a schema version and never used it. It also kept API tokens in a plain-text table. See [v3-notes.md](../v3-notes.md).

## What follows from it

- Analysis results are keyed by audio hash and analyser version, so a rename or retag never triggers re-analysis, and a new analyser version does.
- Tags are kept in three layers: what is in the file, what providers proposed, and what the user set. A view gives the effective value.
- The journal for file operations is its own table with steps, inverse steps and state. It is written before the file is touched.
- Waveform peaks and other large binary results are BLOBs in the database until measurement says they should be files.
- Text search has a budget of 50 ms and idle memory a budget of 400 MB. Both are measured on a synthetic 100,000-row library in M2.

## What would change it

Embedding search that misses its budget at 100,000 tracks would bring in an index, still inside the process.

A second writer, such as sync between machines, would reopen the whole question.
