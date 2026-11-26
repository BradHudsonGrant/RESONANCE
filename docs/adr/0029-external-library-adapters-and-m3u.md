# 0029: External library adapters and M3U playlists

Date: 25 November 2026. Status: accepted.

## Context

DJs keep playlists in other apps, and a rename or replace in Resonance moves files those apps still name by the old path ([product.md](../product.md), Replace step 4 and Other DJ libraries). Each app needs an adapter that can detect, read, plan, back up, apply and verify. M3U and M3U8 come first: plain text, a format many DJ apps import, and safe to test in a sandbox folder.

## Decision

- `crates/integrations` holds one trait, `ExternalLibraryAdapter`, with `detect`, `read_library`, `preview_changes` and `apply_changes`. `apply_changes` is written once on the trait: it plans, copies each file it will write into a backups folder and checks the copy, refuses if the file changed since it was read, writes beside the file and moves the result over it, reads the bytes back, then reads the library again through the adapter and checks it names every new path. If any check fails, every file written is put back. Adapters only say how to read and plan. The crate depends on `core` only. The API composes it with the database.
- The text rules live in `crates/core/src/playlist.rs`: reading `#EXTM3U` and `#EXTINF`, relative and absolute paths in both slash styles, `file://` addresses, UTF-8 with or without a BOM and older Latin-1 files. A path is compared by a key with forward slashes and no `.` or `..`, lower-cased on Windows and macOS. A rewrite changes only the lines it plans to, keeps line endings and encoding, writes a new path in the style of the old one (relative if it was relative, backslashes if it had them), and refreshes the `#EXTINF` length and title from the library. A Latin-1 file that cannot hold a new character is refused rather than converted.
- `external_library` holds each library by the file it is read from. Its playlist shows as a collection with `external_library_id` set, which makes it read-only: adding, removing and renaming are refused in `crates/db`, and deleting it stops following the file and never touches it. Reading again rebuilds the collection from the file, matching entries to tracks by path inside a source or by a change still waiting.
- `pending_external_change` holds one row per library and track: the path the library names and the path it should name. A rename, a replace and their undo queue a change in the same transaction as the asset's new path, for every library whose mirror holds the track. A second change moves the new path on, and a change back to where the library already is removes the row, so undo before applying leaves nothing to write.
- Nothing is written until the user asks. The sidebar shows how many files moved, and the Update playlists sheet shows each line before and after. Backups go to `backups/playlists` in the data folder.
- Export writes a collection to a new extended M3U8 with full paths in the platform's slashes, and never overwrites a file.

## Costs

A playlist updated through Resonance is not an operation in History, so it is undone from its backup rather than with undo. A mirror only holds tracks the library has scanned. An entry that names a file outside the sources is counted and left out. Every track in a mirror queues a change on every rename, even if the user never updates that playlist.

## What would change it

An adapter whose library is a database, such as Engine DJ, may want to plan statements rather than whole files, which would add a second kind of planned change to the trait. Writes into another app's library while it runs would need a running-app check before `apply_changes`.
