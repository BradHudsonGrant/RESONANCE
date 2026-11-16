# Identity hashing and fingerprinting

Measurements for [ADR 0004](../adr/0004-identity-model.md) over the 50 real tracks in `fixtures-local/` (20 MP3, 10 FLAC, 8 AIFF, 7 WAV, 5 M4A; `1,453,998,676` bytes, `1.454 GB` / `1.354 GiB`). The harness is `measure_local_fixtures` in `crates/library/tests/identity.rs`, built with `--release`.

Machine: ROG Zephyrus G16, Core Ultra 9 285H (16 logical cores), 64 GB RAM, Windows 11 Pro, `rustc 1.100.0`. Real-time antivirus scanning is on and the repo has no exclusion.

## Method

`compute_identity` hashes the raw file bytes with `blake3` `1.8.7`, then decodes the default audio stream once through `symphonia` `0.6.1` (`gapless: true`, `verify: true`). Every decoded packet is converted to interleaved full-scale signed `i32` little-endian samples behind `"resonance-audio-i32le-v1\0"`, sample rate (`u32` LE) and channel count (`u16` LE) for `audio_hash`, while the first 120 seconds of `i16` samples feed `rusty-chromaprint` `0.3.0` (`Configuration::preset_test2()`). Each throughput figure below is the median of three runs, with the individual runs in brackets.

## Lossless equivalence

A 24-bit FLAC track from `fixtures-local/` was transcoded with `ffmpeg` `8.1` to 24-bit WAV (`pcm_s24le`), 24-bit AIFF (`pcm_s24be`) and ALAC `.m4a` (`alac`). All four files had different `byte_hash` values and produced the same `audio_hash` (`647b4d38bda09bfc7c2c2955301ea24233640379abc318b68d8c329120d6078f`). Automated tests in `crates/library/tests/identity.rs` also check 16-bit WAV, 24-bit zero-padded WAV, AIFF, FLAC and a FLAC copy retagged with cover art.

## Parity with `fpcalc`

`compute_identity` decoded and fingerprinted all 50 tracks. Official `fpcalc` `1.5.1` decoded 49 and failed on one `.m4a` track in `fixtures-local/` that carries a leading ID3v2 block ahead of its MP4 `ftyp` atom; `open_audio_reader` steps past that header before probing.

Across the 49 tracks both tools decoded:

| Check | Result |
| --- | --- |
| Wire compression and unpadded URL-safe Base64 from raw `u32` words | 49 / 49 byte-identical to `fpcalc` |
| Bitwise subfingerprint agreement, median | 0.9924 |
| Bitwise agreement, 10th percentile / minimum / maximum | 0.9688 / 0.9364 / 0.9996 |
| Tracks at or above 0.95 / 0.90 bitwise agreement | 45 / 49 and 49 / 49 |

The small differences in raw subfingerprints come from Symphonia and FFmpeg rounding decoded lossy frames slightly differently before Chromaprint's resampler and FFT. Every track sits well clear of the 0.85 to 0.90 match band, and the AcoustID wire encoding is exact.

## Throughput and 113 GB projection

| Pass | 50 tracks (1.454 GB) | Throughput | Tracks / s | Projected 113 GB | Projected 113 GiB |
| --- | --- | --- | --- | --- | --- |
| `byte_hash` only, 1 thread | 0.65 s (0.66, 0.65, 0.65) | 2,234.0 MB/s | 76.8 | 0.8 min (51 s) | 0.9 min (54 s) |
| Full identity, 1 thread | 13.21 s (13.21, 13.33, 13.18) | 110.1 MB/s | 3.8 | 17.1 min (1,026 s) | 18.4 min (1,102 s) |
| Full identity, 12 threads | 1.80 s (1.78, 1.80, 1.80) | 809.1 MB/s | 27.8 | 2.3 min (140 s) | 2.5 min (150 s) |

## Conclusion

`blake3` `1.8.7`, full-scale `i32` little-endian audio hashing and `rusty-chromaprint` `0.3.0` meet every requirement in [ADR 0004](../adr/0004-identity-model.md). `scan` stays instant on metadata and file stamps, move reconciliation checks `byte_hash` at over 2 GB/s before falling back to `audio_hash`, and `compute_batch` caps background decoding at half the logical cores (8 threads on this machine) so a first pass over 113 GB finishes in a few minutes while leaving the UI and audio threads free.
