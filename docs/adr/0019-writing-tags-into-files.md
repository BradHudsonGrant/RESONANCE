# 0019: Writing tags into files

Date: 18 November 2026. Status: accepted.

## Context

[product.md](../product.md) says the files belong to the user: any change to a file is previewed, written to a journal, and can be undone. Tags go into the format the file expects, ID3v2.3 by default because DJ hardware reads it most reliably, and frames the app does not understand are kept. [ADR 0018](0018-tag-layers-and-precedence.md) left the user's values in `user_tag` with `dirty_on_disk` set until this step writes them.

A DJ's files carry more than the fields the library reads. Serato keeps its beat grid and cues in `GEOB` frames, other software leaves `PRIV` frames and custom `TXXX` text, and many files hold artwork and ReplayGain. Losing any of those is worse than not writing at all.

## Decision

- Writing happens only when the user applies it. `PreviewTagWrites` lists each file, the fields that would change from what to what, and why a file cannot be written. It changes nothing. `ApplyTagWrites` writes the same list.
- MP3, WAV and AIFF get an ID3v2.3 tag. FLAC gets Vorbis comments. M4A gets MP4 `ilst` atoms. The writer opens the file as its own format in `lofty` (`MpegFile`, `WavFile`, `AiffFile`, `FlacFile`, `Mp4File`), splits the format's tag into the part `lofty` can map to generic items and the remainder, changes only the items for the fields being written, and merges the remainder back. Frames it does not map never pass through the generic tag, so they come out as they went in. Pictures do pass through it, as pictures, and come back with the same type, format and data. Tests write and undo all five formats with a `PRIV` frame, a custom `TXXX` (or Vorbis field, or freeform MP4 atom) and a cover, and check each survives.
- A field is written under every key its format has a frame for, so any reader finds the same value. An empty user value removes the field. Mix and subgenre have no common frame, so they stay in the library.
- The file is never edited in place. It is copied to a `.resonance-tmp` sibling on the same volume, the copy is written, and the copy's decoded audio hash must equal the original's before the copy replaces the file with a rename. If the hash differs, the copy is deleted and the file is left alone.
- A file is refused if its size or modified time differs from the last scan, or if it is an M4A with an ID3v2 block in front of the MP4 container. Those need a rescan or a container repair first.
- Each apply is one row in a new `operation` table: kind, state (`pending`, `applied`, `failed`, `undone`), a summary, the forward steps and the inverse steps as JSON, and when it was created, applied and undone. The inverse for each file is the exact items each written key held before, plus the ID3v2 version the file had, and is saved as soon as that file is written, so a crash part way still leaves the files that changed undoable.
- `UndoOperation` restores those items through the same writer and the same checks. It refuses to start if any file has changed since the write, so undo never throws away a later edit. Undo puts the file back; the user's values stay in `user_tag` and show as unwritten again.

## Why ID3v2.3

ID3v2.4 allows UTF-8 text and new frames such as `TDRC`, and some older players and software only understand v2.3. The CDJ-2000NXS2 manual lists ID3 v1, v1.1, v2.2, v2.3 and v2.4 as supported, so current Pioneer players are not the problem, but v2.3 is the version every player and DJ program reads. It has not been checked on hardware here yet. When it is, the result goes in this record, and v2.4 becomes an option if a player is found that needs it.

Writing v2.3 converts v2.4-only frames in the same tag to their v2.3 equivalents, as any v2.3 writer does. Undo writes the tag back in the version it had.

## Why a journal row per apply

M4 Replace and M11 Organise move and delete files. They need the same promise: preview, record, undo. One table with forward and inverse steps per operation serves all three, and the Activity panel can list recent operations from it.

## What would change it

A tag format whose remainder `lofty` cannot carry through a split and merge, or a player that reads ID3v2.4 but not v2.3.
