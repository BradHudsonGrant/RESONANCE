# 0004: Identity model

Date: 12 November 2026 (settled 15 November 2026). Status: accepted.

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
- The fingerprint comes from `rusty-chromaprint` (0.3.0, MIT), a pure-Rust port (`Configuration::preset_test2()`, first 120 seconds, compressed to unpadded URL-safe Base64 for AcoustID and stored alongside the raw little-endian `u32` subfingerprints for local similarity matching). Checked against `fpcalc` `1.5.1` across `fixtures-local/` in [benchmarks/identity.md](../benchmarks/identity.md): wire encoding is 49/49 byte-identical on the same `u32` words, median bitwise agreement across full decodes is `0.9924` (minimum `0.9364`), and `OffsetFile` decodes the leading-ID3v2 `.m4a` fixture that `fpcalc` rejects.

## Settled in M2

Measured on 15 November 2026 ([benchmarks/identity.md](../benchmarks/identity.md)):

- **Normalised audio stream.** Symphonia `0.6.1` (`gapless: true`, `verify: true`, with `"alac"` and `"id3v2"` enabled) decodes every audio packet into interleaved full-scale signed `i32` little-endian samples (`i8`/`u8`, `i16`/`u16` and `i24`/`u24` shift cleanly into the high bits with zero dither). Hashing `"resonance-audio-i32le-v1\0" || sample_rate_u32_le || channels_u16_le || samples_i32_le` produces the exact same 32-byte `audio_hash` across WAV (16-bit and zero-padded 24-bit), AIFF, FLAC, ALAC `.m4a` and retagged copies with cover art, proven on both generated signals and a 24-bit FLAC track from `fixtures-local/`.
- **Decoder upgrades on lossy files.** `asset.audio_decoder` records `"symphonia/0.6.1"`. When that string changes in a later build, `pending_identity_assets` re-queues included lossy assets (`mp3`, `m4a`) while leaving lossless assets untouched, and tracks keep their identity through `track_id` and Chromaprint.
- **Hash function.** `blake3` `1.8.7` (used under `Apache-2.0`, [ADR 0005](0005-licence-policy.md)) hashes raw file bytes (`byte_hash`) at `2,234 MB/s` on one thread and hashes the normalised PCM stream (`audio_hash`) in the same decode pass as Chromaprint.
- **First-pass cost over 113 GB.** Over the 50 real tracks in `fixtures-local/` (`1.454 GB`), full identity runs at `110.1 MB/s` on one thread (`17.1 min` for `113 GB`) and `809.1 MB/s` across 12 threads (`2.3 min` for `113 GB`). Background batches use up to half the logical cores (capped at 8) without holding the database lock during decoding, so the first pass completes in a few minutes while the app stays responsive.

## What would change it

A fingerprint that turns out too weak on vinyl rips and underground releases to tell versions apart would need a second signal, such as alignment against the candidate.
