# 0025: Quality checks in the identity decode

Date: 25 November 2026. Status: accepted.

## Context

A DJ needs to know which files are damaged, clipped, broken up or lossy files saved as lossless before playing them out. Every check needs the decoded audio, and the identity pass already decodes every file once to hash it and take its fingerprint. A second decode would double the slowest part of bringing in a library.

## Decision

- The identity decode feeds a `QualityMeter` in `crates/library/src/quality.rs` beside the audio hash and the fingerprint. It measures loudness, loudness range, sample and true peak, low and mid crest factors, full-scale runs, dropouts and, on lossless files at 44.1 kHz or more, the spectrum from 8 to 21.5 kHz.
- The rules that judge those numbers live once in `crates/core/src/quality.rs`. A flag is either confirmed, a fact about the file such as frames that failed to decode, or likely, a strong sign worth checking by ear, and it always says why in plain words.
  - Damaged, confirmed: decode errors, a failed stream checksum, or a stated length more than 500 ms from the decoded one. MP3 headers often guess their length, so MP3 is not held to it.
  - Clipping: 10 or more runs of at least 4 full-scale samples is confirmed. A true peak over +0.5 dBTP on a lossy file is likely.
  - Dropouts, likely: 10 to 500 ms below -75 dBFS, cut into music above -24 dBFS on both sides, with a sample jump over 0.25 at one edge.
  - Fake lossless, likely: in the 90th-percentile spectrum of the loud frames, a drop of at least 28 dB within 600 Hz to below -85 dBFS, at or under 20.6 kHz. A record's roll-off of 6 to 12 dB an octave never comes close to that, and converters filter every recording above 20.6 kHz.
- Results go in `asset_quality` and `quality_flag`, written in the same transaction as the identity they were measured with. An asset with no `asset_quality` row is pending, so a library identified before this decodes each file once more.
- Loudness and true peak come from `ebur128` 0.1.11, MIT, a pure Rust port of libebur128 that implements EBU R128 and ITU-R BS.1770 with 4x oversampled true peak. It was released on 26 October 2026, has had over a million downloads, and pulls in `bitflags`, `dasp_frame`, `dasp_sample` and `smallvec`, all MIT or Apache-2.0. The spectrum uses `rustfft` 6.4.1, MIT or Apache-2.0, which the library already had through `rusty-chromaprint`.

## Costs

The identity pass does more work per sample. On the 50 local fixtures a release build identifies them in 7.1 s with the checks. Files longer than about 18 minutes at 44.1 kHz are judged for a lossy past on their first 18 minutes. The flags are heuristics where they say likely: a master cut at 16 kHz on purpose would be flagged.

## What would change it

A file type the checks misjudge often enough to annoy, or an identity pass that becomes too slow on large libraries, which would move the spectrum to a later, separate pass.
