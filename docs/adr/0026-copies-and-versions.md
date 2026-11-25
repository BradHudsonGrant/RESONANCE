# 0026: Copies and versions

Date: 25 November 2026. Status: accepted.

## Context

A library collects the same track more than once: a file copied to a backup folder, a WAV and the FLAC made from it, an MP3 bought before the lossless release. It also holds real versions of one song: a radio edit, an extended mix, a dub, someone's remix. The first kind can go down to one file. The second must never be offered for removal, because to a DJ an extended mix and a radio edit are different tools ([product.md](../product.md)).

## Decision

- `classify_asset_pair` in `crates/core/src/duplicates.rs` sorts two files along the identity ladder of [ADR 0004](0004-identity-model.md):
  - Same file: equal byte hash.
  - Same audio: different bytes, equal decoded-audio hash. A retag, new artwork, or WAV against FLAC.
  - Another encode: Chromaprint agreement of at least 0.88, lengths within 2.5 s, and titles that name the same mix.
  - Different version: shared material or the same artist and title, plus lengths more than 3.5 s apart or titles that name different mixes. Shared material is a run of about 10 s, at any offset, where every 4 s window of the two fingerprints agrees on at least 85% of bits. That finds a radio edit inside the first two minutes of an extended mix.
  - Anything else is unrelated. That includes the same song 2.5 to 3.5 s longer, which is too long for an encode and too short to call an edit.
- A mix name is a bracketed or dashed part of the title holding a version word: edit, extended, dub, instrumental, VIP, remix, mix, rework, bootleg, acapella, version, club, radio or reprise. "Original mix" names no mix, "Extended Mix" and "Extended" are the same, and "feat." parts stay in the title.
- Copies rank unflagged before flagged (damaged or a likely transcode), lossless before lossy, then by bitrate, sample rate and bit depth. A missing file is never the one to keep.
- For one track, candidates are files with the same byte or audio hash, files by the same artist, and fingerprinted files within 2.5 s of its length. Each is read from an index. A file that shares material with the track but has no tags and a different length is only found through its name.
- Keeping a copy is the user's choice, made in Prep and confirmed once more. Each other copy goes to the Recycle Bin and its track folds into the kept one: favourite, collection places and edited tags carry over where the kept track has none. Versions are never touched. The Recycle Bin is the undo until the operation journal takes this over ([#163](https://github.com/BradHudsonGrant/RESONANCE/issues/163)).
- The Recycle Bin, or the desktop trash on macOS and Linux, is reached through `trash` 5.4.0, MIT, from the `ArturKovacs/trash` repository. 5.4.0 was released on 17 November 2026, 12 days after 5.3.0, and the repository is not archived. It brings in `urlencoding`, MIT, and on Linux a second `jiff`, MIT or Unlicense, beside crates the app already has. [ADR 0005](0005-licence-policy.md) listed 5.3.0 as permissive; 5.4.0 was checked again for this.

## Costs

A title that names its mix in a way the list does not know is read as the same mix. Two untagged cuts of different lengths are never linked, because the lookup finds files without tags only by length. The fingerprint covers the first two minutes, so two versions that share only their second halves look unrelated.

## What would change it

DJs telling versions apart by words the list lacks, or untagged libraries where versions go unnoticed, which would need a fingerprint index across the whole library rather than one lookup per track.
