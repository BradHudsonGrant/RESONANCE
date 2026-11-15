# 0013: Filesystem watching with `notify`

Date: 15 November 2026. Status: accepted.

## Context

Each library source has a "watch for changes" toggle that is on by default ([product.md](../product.md#sources-and-library)). When a new download lands in a watched music folder or a tag is edited outside Resonance, the library should pick it up without a manual rescan. Watching must obey the same folder rules as scanning: `include_subfolders`, per-source subfolder toggles, the editable ignored-folder list, hidden folders, `.resonance-ignore` and Resonance's own folder.

## Decision

- Use [`notify`](https://crates.io/crates/notify) `8.2.2` in `crates/library` (`RecommendedWatcher`: `ReadDirectoryChangesW` on Windows, `FSEvents` on macOS, `inotify` on Linux).
- Allow `CC0-1.0` in `deny.toml` for `notify` and `notify-types`, as noted in [ADR 0005](0005-licence-policy.md).
- Watch each enabled source (`RecursiveMode::Recursive` when `include_subfolders` is on, `RecursiveMode::NonRecursive` when it is off), drop events inside excluded or ignored folders, coalesce bursts over a short quiet window, and run the existing read-only `scan` to apply additions and updates.
- Never delete database rows when a file disappears on disk. Missing-file flagging and relinking arrive in their own step so an external move or rename never drops a track's identity.

## Why

`notify` `8.2.2` is the current stable release of `notify-rs/notify`, with `9.0.0-rc` in active development, 55 million crate downloads in the last 90 days, and no copyleft dependencies. Its licences in our Windows and Linux trees are `CC0-1.0`, `MIT` and `Apache-2.0`.

Feeding coalesced watch events into `scan` keeps one set of exclusion rules, read-only file handles and `(size_bytes, modified_ns)` stamp checks instead of a second ingestion path. Copying a large WAV or FLAC into a folder fires several write events before the writer finishes; waiting for a quiet window and letting `scan` skip unreadable or unchanged files keeps partial writes harmless.

## What follows from it

- Adding a source, toggling `watch`, toggling `include_subfolders`, changing excluded subfolders or editing the ignored-folder list updates the active watcher set immediately.
- When a background watcher updates the database, `api` bumps a watch revision and notifies connected WebSocket and Tauri clients so open tables refresh in place without losing scroll position or selection.
- `app/THIRD-PARTY-NOTICES.txt` includes `notify`, `notify-types` and their platform dependencies.

## What would change it

On very large network shares where `ReadDirectoryChangesW` drops events or is unsupported, a periodic background poll fallback per source would supplement `notify`.
