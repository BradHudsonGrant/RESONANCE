# 0004: Identity model

Date: 12 November 2026. Status: accepted. The details marked "to settle" are decided with measurements in M1.

## Context

Identity has to survive rename, move, retag and replacement. If it doesn't, cues, grids, collections and sets come loose every time a file changes, and Replace cannot work at all.

The old app used the path. Every rename had to repoint a row, a file renamed outside the app became one missing track and one new track, and duplicate detection had to be limited to one folder to stay sane. See [v3-notes.md](../v3-notes.md).

## Decision

A track is a recording. An asset is a file. An asset belongs to a track.

Three things identify audio, from exact to fuzzy:

| What | Answers | Survives |
| --- | --- | --- |
| Hash of the file's bytes | Is this a literal copy? | Move and rename |
| Hash of the decoded audio | Is this the same audio? | Move, rename, retag, new artwork, a different lossless container |
| Chromaprint fingerprint | Is this the same recording? | All of that, plus a different encode or bitrate |

An asset's identity is its decoded-audio hash and its fingerprint. Size and mtime are only for cheap change detection. A path is never identity.

Cues, grids and phrases are stored against track time. An asset maps to track time through `offset_samples` and `tempo_ratio`, and that mapping function lives in `core`. This is what lets Replace keep cues when the new file starts a few milliseconds later or runs at a slightly different speed.

## Why

Tags and artwork change the file's bytes without changing the audio, so a byte hash cannot be the identity. The decoded audio is what the user means by "this track".

A fingerprint is needed as well because two encodes of the same master decode to different samples. It is also what AcoustID looks up.

## What follows from it

- A scan shows a file within seconds from its tags and file facts. The decoded-audio hash and the fingerprint arrive later, from background analysis. Anything that depends on identity waits for them.
- A missing file is flagged. If the same audio shows up at another path, the app offers to relink.
- The analysis cache key is the decoded-audio hash plus the analyser version.
- Duplicates sort into byte-identical, audio-identical, same recording in a different encode, and different versions. Radio edits, extended mixes, dubs and remixes are versions and are never called duplicates.
- The fingerprint comes from `rusty-chromaprint` (0.3.0, MIT), a pure-Rust port. Check its output against `fpcalc` on the fixtures before trusting AcoustID lookups made with it.

## To settle in M1

- The exact form that gets hashed. Lossless files decode to the same integers everywhere, so FLAC, WAV and AIFF copies of one master should hash the same once sample format and bit depth are normalised. That needs proving on real files.
- Lossy decoders can differ by a rounding step between versions. The hash record carries the decoder and its version. When the decoder is upgraded, lossy assets are hashed again in the background and keep their identity through the fingerprint.
- The hash function. BLAKE3 is the candidate (1.8.7, Apache-2.0).
- The cost of a first pass over 113 GB, measured on the laptop.

## What would change it

A fingerprint that turns out too weak on vinyl rips and underground releases to tell versions apart would need a second signal, such as alignment against the candidate.
