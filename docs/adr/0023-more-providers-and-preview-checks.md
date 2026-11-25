# 0023: More providers and preview checks

Date: 24 November 2026. Status: accepted. Adds to [ADR 0017](0017-identification-providers-over-ureq.md).

## Context

AcoustID, MusicBrainz and Discogs miss a lot of club music: white labels, edits, label exclusives and anything only sold as a download. A name search on those three often finds the right title on the wrong version, a radio edit for an extended mix or the original for a remix, and nothing in the answer says so.

## Decision

- Two more name searches, neither needing a key. Deezer's public search runs at 8 requests a second, and the iTunes Search API runs at 18 a minute, under its published 20.
- Pasted SoundCloud and YouTube links are read through their public oEmbed endpoints only.
- When names are all there is, the top two candidates that offer a 30-second preview are checked by ear. The preview is decoded in memory, fingerprinted with Chromaprint and slid along the file's own fingerprint. A match turns a name match into an audio match. Nothing is written to disk. The check is on by default and can be turned off in Settings.
- AudD is a paid recogniser. It runs only when the user has entered a token and the free steps leave a track uncertain.
- Ranking moves into `crates/core/src/identify/`: `parse` for names, `score` for candidates, `fuse` for merging answers and `url` for pasted links. Up to five ranked candidates are kept per track, so Prep can offer the runner-up.
- Deezer and iTunes covers are never stored or embedded, because their terms do not allow it. Deezer's BPM is not staged either.

## Why

A preview check is the only free way to tell an extended mix from a radio edit when the names agree. Keeping it in memory means no audio is downloaded to the library or the cache.

## What follows from it

- A track can now take up to two preview downloads, about 1 MB each, on its first run. Later runs read the cached answer. Deezer's answers are cached for 10 minutes rather than 30 days, because their preview links are signed and expire after about 20.
- A track settled by an ISRC or a link in its own tags skips the AcoustID lookup. An ISRC is the label's own code for the recording, so a wrong one is rare, and the skip saves a request per track. AcoustID still takes up to 10 fingerprints in one request.
- The covers of the runner-up matches are fetched into the artwork store so Prep can show them. Removing the ones no track uses is [#147](https://github.com/BradHudsonGrant/RESONANCE/issues/147).
- Each new provider is one file under `crates/identify/src/providers/` with its own limiter, as ADR 0017 set out.
