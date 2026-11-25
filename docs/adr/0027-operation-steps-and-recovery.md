# 0027: Operation steps, recovery and the bin

Date: 25 November 2026. Status: accepted.

## Context

Every change to files is journaled and can be undone ([product.md](../product.md), rule 1). Tag writes and renames already write an `operation` row, but a crash in the middle of one could leave a working copy beside a file or a file under a name the library does not know. Replace, Organise and deleting tracks are made of several file steps: copy, move, update the library, move to the bin. If the app stops between two of them, the library and the disk must not stay half changed.

## Decision

- An operation made of file steps writes its whole plan to `operation_step` before the first file is touched: one row per step with what it does and what puts it back, in one transaction. Steps are copy a file (checked against a BLAKE3 hash), move a file, hide an asset, move a file to the bin, and delete a file for good. The rules for states and recovery live in `crates/core/src/journal.rs`, and the steps run in `crates/library/src/journal.rs`.
- An operation goes pending, then executing with the step it is on in `current_step`, then applied. A step that fails puts back the ones done before it and the operation becomes rolled back. Undo runs the inverses newest first and skips any already run, so an undo that stopped part way can run again.
- A change to the library is made in the same transaction that marks its step done. A change on disk is marked done straight after it happens, so the only unknown after a crash is the step that was running. Recovery asks the disk about that one: a copy exists, a moved file is at its new name, a binned file is gone from its folder.
- When the library opens, before the watcher or any job starts, every pending or executing operation is brought to rest. If every step took effect it is finished, after checking the hash of any copy the journal had not marked done. Otherwise the steps that took effect are put back, newest first. The outcome is written to `operation.recovery`. A problem, such as a file no longer in the bin, is noted there and the operation stays unfinished, so the next start tries again.
- Tag writes and renames keep their own per-file tables, `operation_tag_decision` and `operation_rename`, which already record each file in the same transaction as its change. Recovery removes a tag write's leftover working copy and gives a renamed file that was never recorded its old name back. A rename now journals its planned file names before it starts.
- Deleting a track moves its files to the Recycle Bin, or the desktop's trash, through `trash` ([ADR 0026](0026-copies-and-versions.md)). The asset stays in the database with `trashed_by` set and is left out of every view, so collections, favourites and tags come back on undo. Undo finds the file in the bin by its old path, size and the time it went, and restores it. Restoring needs `trash::os_limited`, which covers Windows and Linux. On macOS undo says the file is in the Trash to put back by hand.
- When the bin refuses a file, nothing changes and the app asks before deleting for good. That operation is journaled but cannot be undone.
- SQLite cannot add states to a CHECK in place, so migration 24 rebuilds `operation`. Foreign keys are now off while migrations run, so the rebuild keeps the rows that point at the table, and a migration that leaves more broken references than it found is refused.
- Keeping one copy of a recording still uses the bin as its undo. Folding tracks deletes rows that need an inverse first ([#167](https://github.com/BradHudsonGrant/RESONANCE/issues/167)).

## Costs

A file deleted from the bin by the user cannot come back, and undo says so. A tag write that stopped after its file was swapped but before the library recorded it keeps the new tags without an undo for that file; the next scan reads them. Undo on macOS needs a hand restore until `trash` can restore there.

## What would change it

`trash` restoring on macOS, or steps that cannot be checked on disk after a crash, which would need a marker file per step.
