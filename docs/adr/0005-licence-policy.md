# 0005: Licence policy

Date: 12 November 2026. Status: accepted.

## Context

The repo is private and has no licence yet. The source may be opened later, or the app sold, and that is undecided. The dependency tree must not make the choice.

## Decision

- Only permissive licences in the dependency tree: MIT, Apache-2.0, BSD, ISC, Zlib and MPL-2.0.
- No GPL or AGPL linking.
- `cargo-deny` enforces this in CI and in `just verify`.
- Model weights with non-commercial licences are opt-in downloads with the licence shown. The default set must be usable commercially.
- The installer carries a generated third-party notices file.
- No LICENSE file while the repo is private. The README says the code is not yet licensed for reuse.

## Why

A permissive tree can go into an open-source release or a closed commercial one. A copyleft dependency would close the second door.

## What follows from it

- GPL projects such as Mixxx can be read to learn how a controller or a file format behaves. Their code and mapping files are not copied.
- The six licences above are the starting allow list. Other permissive licences turn up in any Rust tree. Unicode-3.0 comes with `unicode-ident`, and `notify`, the file watcher, is CC0-1.0. Each one is added to `deny.toml` on its own line with a comment naming the crate that needs it, after checking that it is permissive.
- Where a crate offers a choice, take the permissive option. `blake3` is CC0-1.0 or Apache-2.0, so it is used under Apache-2.0.
- FFmpeg is run as a separate program and never linked. The FFmpeg on the development laptop is a GPL build (`--enable-gpl --enable-version3`). Running it is fine. Shipping it inside the installer would bring GPL obligations for that binary, so an installer bundles an LGPL build or finds FFmpeg on the machine. Decide when Convert lands in M4.
- The Chromaprint C library has carried LGPL code. Check before linking it. The default is the pure-Rust port, `rusty-chromaprint`, which is MIT.
- Signalsmith Stretch is MIT. Rubber Band is GPL and is not used.
- Checked today and permissive: `tauri` 2.12.5, `ort` 2.0.0-rc.15, `rusqlite` 0.42.0, `lofty` 0.25.5, `cpal` 0.18.2, `trash` 5.3.0, `midir` 0.11.1, `gilrs` 0.11.2, `hidapi` 2.6.7, `beat-this` 1.1.0, `signalsmith-stretch` 0.1.3 and `ebur128` 0.1.11 are MIT or Apache-2.0. `symphonia` 0.6.1 is MPL-2.0. Each is checked again, along with its maintenance, in the ADR that adds it.
- AcoustID is free for non-commercial applications once registered. A commercial release needs its commercial plan.
- Discogs-MAEST is CC BY-NC-SA 4.0 and MERT is CC BY-NC 4.0. Both are opt-in only.

## What would change it

A licence being chosen for the project. A copyleft choice would widen what can be linked. A commercial release would need the AcoustID plan and a second look at every model licence.
