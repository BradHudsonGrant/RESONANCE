# 0020: Renaming files from templates

Date: 18 November 2026. Status: accepted.

## Context

[product.md](../product.md) asks for filename templates such as `{artist} - {title} ({mix})` with a live preview on the user's own tracks, names every OS accepts, and renames that stay in the file's folder. Like a tag write, a rename is previewed, journaled and undoable ([ADR 0019](0019-writing-tags-into-files.md)). A DJ may rename one track or a library of 100,000, and the library moves between Windows, macOS, Linux and USB sticks formatted as exFAT.

## Decision

- `crates/core/src/template.rs` parses templates and turns tags into names. The tokens are `{artist}`, `{title}`, `{mix}`, `{bpm}`, `{key}`, `{label}`, `{catno}`, `{year}` and `{track}`. BPM is rounded to a whole number, the track number has two digits, and the year is the first four digits of the date. Values come from the tags the track shows: the user's over the file's.
- An empty token takes one separator with it, and brackets whose tokens are all empty go, so an empty mix never leaves `()` behind. A template that uses `{artist}` or `{title}` leaves the file alone when that tag is empty.
- Names drop or replace `<>:"/\|?*` and control characters, lose trailing dots and spaces, and get a `_` in front of Windows device names (`CON`, `NUL`, `COM1` and so on). The extension never changes. The whole path stays within 259 UTF-16 units, because DJ software on Windows does not use long paths. A name is cut to fit, and a folder too deep for any name is reported.
- Names are compared without case. NTFS, APFS and exFAT ignore case, so a name that differs only in case from another file in the folder is taken. A taken name, or one another file in the same batch claims first, is a collision and that file is not renamed. Names the batch frees stay taken until it ends. `fs::rename` replaces an existing file on every platform, so the target is checked again just before each rename.
- A case-only rename goes through a third name (`<new name>.resonance-rename`), because on a case-blind file system a direct rename is no change.
- Each apply is one `operation` row of kind `rename_files`. Each file it moves is a row in `operation_rename` (operation, asset, old path, new path), written in the same transaction as the asset's new path. The library lock is held while the file is renamed and its path recorded, so a scan the watcher starts never sees the new name before the library knows it, and there is no churn after a rename.
- Undo renames the files back, newest first. It refuses to start if any file has moved since, so it never undoes a later change.
- Up to 200 files rename while the sheet waits. A larger batch, such as the whole library, runs as one Activity job with progress and cancel. It pauses while Mix plays, like the other background jobs. A cancelled batch keeps what it renamed as one operation that can be undone.
- Nothing is renamed until the user opens the Rename sheet and confirms. Settings holds the template the sheet starts from.

## Why a table rather than JSON for the inverse

The tag writer rewrites the operation's inverse JSON after every file. For 100,000 renames that is quadratic. One small insert per file costs the same at any size, and keeping it in the transaction that moves the path means the journal and the library cannot disagree after a crash.

## What would change it

A file system that cannot take the case-only step, or a request to free names inside a batch so two files can swap names.
