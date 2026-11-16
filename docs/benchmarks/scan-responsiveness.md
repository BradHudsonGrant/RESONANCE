# 3,000-file scan and Activity queue responsiveness

Measurements for the M2 acceptance check in [roadmap.md](../roadmap.md#m2-library-v030) ("A scan of 3,000 files stays responsive throughout") against the `< 16 ms` scroll frame ceiling and `< 50 ms` search budget in [architecture.md](../architecture.md#performance-budgets).

Machine: ROG Zephyrus G16, Core Ultra 9 285H (16 logical cores), 64 GB RAM, Windows 11 Pro, `rustc 1.100.0`, headless Microsoft Edge `157.0.4341.60` at `1440 × 900` CSS pixels (`6.1 ms` display refresh, about `164 Hz`). Real-time antivirus scanning is on and the repo has no exclusion.

## Method

Three runs against a fresh SQLite database, a release `resonance serve` process and the `benchmark`-mode UI build. The fixture folder holds `3,000` generated stereo 16-bit `44.1 kHz` WAV files (`2.0 s` and `352,844` bytes each, `1,058,532,000` bytes / `1.059 GB` across `15` subfolders of `200` files) with distinct frequencies per file so every track produces a unique `byte_hash`, `audio_hash` and `rusty-chromaprint` (`preset_test2`) fingerprint.

`scan` runs as a Tier 0 `scan_source` job in `crates/jobs`. Directory walking, tag and container header reads, and `.exists()` checks run outside the `Database` mutex; SQLite WAL commits happen in chunks of `200` files while `activity_changed` and `library_changed` pushes are throttled to `<= 10 Hz`. Once `scan_source` finishes, Tier 1 `identify_library` hashes and fingerprints all `3,000` files across `8` threads (half the logical cores) and commits `16` files per WAL transaction. Incoming WebSocket requests on `/core` run concurrently so an in-flight `scan` call never blocks reads on the same connection.

While `scan_source` and `identify_library` run, the page loops `tracks` (`limit: 200` across rotating offsets) and `FTS5` text searches over `/core`. As soon as the live table reaches `400` rows, `measureTableScroll(148)` runs `148` fling frames and `148` random-jump frames concurrently with those IPC calls, followed by a second `measureTableScroll(148)` pass once all `3,000` tracks and identities are committed.

## Queue duration and concurrent IPC latency

Run 1 opened the `3,000` newly written WAV files cold with Windows real-time antivirus inspecting every first open, stretching `scan_source` to `24.56 s`; Runs 2 and 3 hit warm filesystem cache in `0.52 s` and `0.35 s`. Across all three runs, `identify_library` processed the `1.059 GB` set in `2.11 s` median (`1.80 s`, `2.11 s`, `2.78 s`).

| Run | `scan_source` | `identify_library` | Total queue |
| --- | ------------- | ------------------ | ----------- |
| 1   | 24,556 ms     | 1,797 ms           | 26,358 ms   |
| 2   | 524 ms        | 2,114 ms           | 2,651 ms    |
| 3   | 352 ms        | 2,777 ms           | 3,147 ms    |

Full WebSocket round-trip times from the browser page while `scan_source` and `identify_library` were writing batches:

| Run | Phase              | Calls | `tracks` median | `tracks` p95 | `tracks` p99 | `tracks` max | `FTS5` median | `FTS5` p95 | `FTS5` p99 | `FTS5` max |
| --- | ------------------ | ----- | --------------- | ------------ | ------------ | ------------ | ------------- | ---------- | ---------- | ---------- |
| 1   | `scan_source`      | 1,708 | 1.3 ms          | 2.0 ms       | 5.3 ms       | 24.0 ms      | 1.0 ms        | 2.2 ms     | 4.7 ms     | 6.3 ms     |
| 1   | `identify_library` | 111   | 1.6 ms          | 3.5 ms       | 3.8 ms       | 6.0 ms       | 2.0 ms        | 3.5 ms     | 4.0 ms     | 4.5 ms     |
| 2   | `scan_source`      | 21    | 1.5 ms          | 3.3 ms       | 10.9 ms      | 10.9 ms      | 1.5 ms        | 33.4 ms    | 78.4 ms    | 78.4 ms    |
| 2   | `identify_library` | 104   | 2.3 ms          | 6.6 ms       | 7.2 ms       | 9.4 ms       | 3.1 ms        | 6.5 ms     | 11.6 ms    | 66.5 ms    |
| 3   | `scan_source`      | 21    | 1.4 ms          | 6.9 ms       | 20.0 ms      | 20.0 ms      | 1.0 ms        | 5.1 ms     | 5.4 ms     | 5.4 ms     |
| 3   | `identify_library` | 141   | 1.6 ms          | 6.1 ms       | 12.3 ms      | 27.5 ms      | 2.5 ms        | 11.2 ms    | 12.3 ms    | 12.4 ms    |

Combining both phases per run (`1,819`, `125` and `162` paired calls), `tracks` stayed at `1.4`–`1.9 ms` median and `2.2`–`6.6 ms` 95th percentile (`27.5 ms` worst single call), and `FTS5` search stayed at `1.0`–`2.4 ms` median and `2.5`–`11.1 ms` 95th percentile, inside the `< 50 ms` search budget. Even during Run 1's `24.56 s` cold antivirus scan, `1,708` page reads and `1,708` searches returned in `1.3 ms` and `1.0 ms` median instead of waiting behind one scan-wide lock.

## Table scroll during active queue

`measureTableScroll(148)` launched while `scan_source` was running and scrolled concurrently with the IPC loop (`888` active scrolling frames across the three runs at `6.1 ms` refresh):

| Run | Pass  | Active jobs (start -> end)          | Median frame | 99th percentile | Longest frame | Missed frames | Frames with a loading row |
| --- | ----- | ----------------------------------- | ------------ | --------------- | ------------- | ------------- | ------------------------- |
| 1   | Fling | `scan_source` -> `scan_source`      | 6.1 ms       | 6.3 ms          | 6.3 ms        | 0             | 0                         |
| 1   | Jumps | `scan_source` -> `scan_source`      | 6.1 ms       | 12.2 ms         | 12.4 ms       | 5             | 0                         |
| 2   | Fling | `scan_source` -> `identify_library` | 6.1 ms       | 6.3 ms          | 6.3 ms        | 0             | 0                         |
| 2   | Jumps | `scan_source` -> `identify_library` | 6.1 ms       | 6.3 ms          | 6.4 ms        | 0             | 9                         |
| 3   | Fling | `scan_source` -> `identify_library` | 6.1 ms       | 6.3 ms          | 12.1 ms       | 1             | 0                         |
| 3   | Jumps | `scan_source` -> `identify_library` | 6.1 ms       | 6.3 ms          | 12.2 ms       | 1             | 6                         |

Every rendered row after the jump pass was already filled (`0.0 ms` wait). Steady-state `measureTableScroll(148)` on all `3,000` tracks immediately afterwards held the same `6.1 ms` median (`6.2`–`12.0 ms` 99th percentile, `12.2 ms` longest frame).

## Conclusion

Moving directory traversal and tag probing outside the database lock, committing scans in `200`-file WAL batches and identity results in `16`-file transactions, and serving WebSocket calls concurrently keeps the library responsive throughout a `3,000`-file (`1.059 GB`) import. Concurrent `tracks` and `FTS5` queries held `1.0`–`2.4 ms` medians (`2.2`–`11.1 ms` 95th percentile per run), and `888` scrolling frames during the active `scan_source` and `identify_library` jobs held a `6.1 ms` median with no frame over `12.4 ms`, inside the `16 ms` ceiling.
