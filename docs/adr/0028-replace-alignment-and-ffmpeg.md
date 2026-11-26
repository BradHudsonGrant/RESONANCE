# 0028: Replace, alignment and FFmpeg

Date: 25 November 2026. Status: accepted.

## Context

A DJ who finds a better copy of a track should be able to swap the file and carry on, with collections, tags and cues where they were ([product.md](../product.md), Replace, step by step). Two encodes of one master differ by a few milliseconds of encoder padding, and a vinyl rip can play at another speed, so cues need the files lined up to well under the 5 ms the ear notices. Convert and Repair need FFmpeg, which [ADR 0005](0005-licence-policy.md) says runs as a separate program and left the choice of shipping it until now.

## Decision

- Cues are stored in track time in a new `cue` table. Each file maps track time onto its own samples through `asset.offset_samples` and `asset.tempo_ratio`, so a replace moves no cue. It sets the new file's alignment and marks each cue "carried over, check".
- `crates/library/src/align.rs` decodes both files to mono and resamples the first and last 45 s of each to 44.1 kHz with `rubato`, already used by the engine. A smoothed spectral-flux envelope at 1 kHz, cross-correlated over 10 s either way, gives up to five candidate lags; loop-based music makes a lag a bar away score nearly as well as the right one. A high-passed (80 Hz) sample cross-correlation within 256 samples of each candidate, with a parabola through the peak, picks the lag to a fraction of a sample. When the end of the track disagrees with the start by more than 2 samples, both ends are measured again over 2 s and the speed ratio comes from the two lags. Correlation below 0.6 means the files did not line up.
- The verdict lives in `crates/core/src/replace.rs`: the same recording only when the identity ladder calls the pair a copy ([ADR 0026](0026-copies-and-versions.md)) and the alignment is confident, better when the copy ranking puts the new file first. A different version defaults to being added as a track of its own.
- Apply is one journaled operation ([ADR 0027](0027-operation-steps-and-recovery.md)) with three new steps: tag a staged copy, attach the placed file to the track, and add a file as a new track. The old file is hidden and sent to the bin before the new one moves into place, so a templated name equal to the old name works. Tags merge with the user's values first, then the old file's, then the new file's. Undo restores the old file from the bin and takes the new one out.
- FFmpeg is found on the machine, not shipped: on the PATH, then where Homebrew, WinGet, Scoop, Chocolatey and Linux packages put it, because a desktop app on macOS does not see the shell's PATH. Settings shows the version and path, or how to install it. Convert has three named presets, and Repair remuxes without re-encoding and drops ID3v2 bytes in front of an MP4. Every output is decoded and fingerprinted against its source, and must decode cleanly, match at 0.88 or more and keep its length within 2.5 s, before it is offered to the compare sheet. The track's file is never touched by either.
- External DJ libraries are updated through their adapters ([#158](https://github.com/BradHudsonGrant/RESONANCE/issues/158)). Until those exist, each replace records the old and new paths in the operation's `forward_json` for the adapters to read.

## Costs

Comparing decodes both files twice, once for identity and once for alignment, which takes a few seconds for a long track. Apply measures again, so it acts on the files as they are. A machine without FFmpeg gets Replace but not Convert or Repair. Speeds that differ by more than about 0.5% smear even a 2 s window and fall back to the start lag alone.

## What would change it

Shipping an LGPL FFmpeg build in the installer, if finding one on the machine proves too hard for new DJs. A grid or phrase table in M5 rides on the same track time and needs no change here.
