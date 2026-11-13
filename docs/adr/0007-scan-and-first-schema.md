# 0007: The scan and the first schema

Date: 13 November 2026. Status: accepted as the default.

## Context

M0 reads a folder of music into the database. The choices here carry into every later feature: what a row is, how a file is recognised on the next scan, and what the scan may touch. On the development laptop the repo sits inside the music folder, so the scan also has to keep the app's own files out of the library.

## Decision

- The first migration makes four tables: `source`, `track`, `asset` and `file_tag`. A path seen for the first time becomes a new track with one asset. Columns for identity, time mapping and primary files arrive in the migrations that first use them.
- An asset's path is stored relative to its source folder, with forward slashes on every platform.
- Tags read from a file are rows in `file_tag`: asset, field, value. The fields are a fixed list in `core`.
- A file is read again only when its size or modified time differs from the last scan.
- A file that cannot be read as audio is still listed, with no facts and no tags, and the scan reports it.
- The scan opens each file for reading only and reads its tags through that handle.
- A scan stays out of hidden folders, the old app's four quarantine folders at any depth, and the folder directly under the source that holds the running program or its data.
- The source folder is walked in its canonical form.
- An ID3v2 tag in front of an MP4 file is stepped over.
- The database is `library.db` in the platform's folder for local app data. `--data-folder` puts it somewhere else.

## Why

Relative paths mean that moving a whole library to another drive changes one `source` row.

Three layers of tags are coming: what is in the file, what providers proposed, and what the user set. A row per field keeps each field's origin apart and lets a view pick the effective value. A wide table would need a column per field per layer.

A file that will not parse is still on disk in the user's folder. Listing it gives the integrity checks in M3 a row to report against, and stops the scan reading it again on every run.

Opening files for reading only makes "never writes" a property of the handle, where care in the code could slip. A test also compares every byte and modified time before and after a scan.

The rule for the app's own folder uses where the program runs from and where its data is. It needs no setting and no path compiled in, and it covers both a checkout inside the music folder and a portable install there. Its cost is an app installed two or more folders deep: the top folder it sits in is skipped, along with any music beside it. The editable ignore list in M1 is the way out of that.

On Windows the canonical form of a path is the only one that opens a folder whose name ends in a space or a dot. The real library has one.

One of the five M4A files in the local set of real tracks starts with an ID3v2 tag. Tag readers take such a file for raw AAC and fail. The MP4 behind the tag is whole, so the scan reads it from where it starts.

The dependencies were checked on the day: `rusqlite` 0.42.0 (MIT), `lofty` 0.25.5 and `directories` 6.0.0 (MIT or Apache-2.0), and `dunce` 1.0.5 (used under Apache-2.0), which turns a canonical Windows path back into its ordinary form for storing and showing. `rusqlite` and `lofty` both had a release in the last month. `directories` was last released in January 2025 and `dunce` in August 2024. Each of those two is a small crate that does one job, and crates.io counts 16 million and 139 million downloads of them in the last 90 days.

## What follows from it

- Tags inside a stray ID3 block are not read. Only the MP4's own tags are.
- A decoder needs the same step over the ID3 block, because offsets inside the MP4 count from where the MP4 starts.
- A deleted file is not noticed yet. M1 flags missing files.
- A scan writes everything in one transaction at the end. M1 moves scanning onto the job queue and writes in batches, so a long scan shows progress and survives being stopped.
- A field holds one value. Several artists in one file come through as the first.

## What would change it

Multi-valued tags that matter to search or matching would turn the one value per field into a list.

A user with an install deep inside a music folder, before the ignore list is editable, would mean narrowing the rule to the program's own folder and its data folder.
