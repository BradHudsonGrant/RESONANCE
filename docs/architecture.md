# Architecture

How Resonance is built. What it does is in [product.md](product.md), how it looks and behaves is in [design/](design/), and the reasons behind the choices below are in [adr/](adr/).

In the stack table, a row marked Fixed is settled. A row marked Default is a recommendation: it can change when there is evidence, and the reason is written down as an ADR.

## Stack

| Layer | Choice | Status |
| --- | --- | --- |
| Desktop shell | Tauri 2 | Fixed |
| UI | Svelte 5, TypeScript in strict mode, Vite | Fixed |
| Core | Rust stable, one Cargo workspace | Fixed |
| Storage | SQLite in WAL mode (rusqlite), FTS5 for text; embeddings stored as BLOBs and searched in memory | Default |
| Audio output | cpal on WASAPI, ASIO optional | Default |
| Decoding | Symphonia, with an FFmpeg binary as fallback and for conversion | Default |
| Time-stretch | Signalsmith Stretch when keylock is on, plain resampling when it is off | Default |
| Tags | lofty | Default |
| Fingerprint | Chromaprint and AcoustID | Default |
| Model runtime | ONNX Runtime in-process through the `ort` crate | Default, with the fallback under Model runtime |
| Trash | the `trash` crate | Default |
| Input | midir, hidapi, gilrs | Default |
| IPC types | Generated from Rust (tauri-specta or ts-rs) | Default |
| Rendering | WebGL2 with a small hand-written renderer | Default |

For every default: check the current version, how well it is maintained and its licence before adding it, and write a short ADR.

Things to know now. `ort` 2.0 was still a release candidate when this was written, so pin the exact version. On Windows the Tauri webview is WebView2, which is Chromium. On macOS and Linux it is the system webview, and WebKitGTK on Linux is the weakest of the three, so plan to ship those platforms on a bundled Chromium runtime (see [What else was considered](#what-else-was-considered)) and stay on WebGL2 until then. A vector database is unnecessary at this scale: brute-force search over 100,000 embeddings in memory takes tens of milliseconds.
## Repository layout

```
RESONANCE/
  crates/
    core/          domain types, time and key maths, provenance; no I/O
    db/            schema, migrations, queries
    library/       sources, scan, watch, identity, tags, file operations, journal
    identify/      fingerprint, providers, matching
    analysis/      DSP analysers, ONNX runner, model registry, cache
    engine/        real-time audio: decks, mixer, clock, metering tap, recording
    metrics/       mix measurements, used live by coach and offline by review
    coach/         guidance rules on top of metrics
    integrations/  rekordbox, Engine DJ, Serato, Traktor, M3U
    control/       actions, keyboard, MIDI, HID and gamepad input, mappings
    jobs/          persistent queue, scheduling, GPU arbitration
    api/           the command layer: typed handlers served over Tauri IPC and a local socket
    cli/           the `resonance` binary: scan, analyse, bench, verify, serve
    fixtures/      test audio made from maths, and the copy of real tracks for local tests; never shipped
  app/             Tauri shell: commands, events, window
  ui/              Svelte app
    src/lib/design/        tokens and primitives
    src/lib/components/    TrackTable, Waveform, Inspector and other domain parts
    src/lib/spaces/        library, prep, sets, mix, replay
    src/lib/ipc/           generated bindings and a thin client
  tools/models/    Python scripts that export and verify ONNX models (dev only)
  mappings/        controller mapping files
  docs/            architecture.md, adr/, design/, roadmap.md
  .github/         workflows, PR and issue templates
```

Create a crate when a milestone needs it. No empty placeholders.

The CLI matters. It runs the same crates as the app, so scanning, analysis, benchmarks and acceptance checks can run without the UI. Use it to prove things work.

## Rules

- Dependencies point inward. Everything may use `core`. `engine` knows nothing about `db`. The UI knows only the IPC layer.
- Each rule has one implementation, in `core`: BPM compatibility, key relations, time mapping, set duration. The UI asks for answers and never works them out again.
- Commands live in `api` as plain typed functions. Two transports call them: Tauri IPC in the packaged app and a localhost WebSocket in development. The handlers and types are the same in both, and the Tauri shell holds no logic.
- The audio callback never allocates, locks, does I/O or logs. Data goes in and out through lock-free queues. Decoding and stretching run on worker threads that feed ring buffers. The audio clock is the only clock. The UI extrapolates between updates.
- Meter and playhead data reach the UI over a binary channel at 60 Hz or less. Waveform data is binary too. Neither goes through JSON events.
- Analysers share one trait. The job queue schedules them by tier and resource. One heavy GPU job runs at a time, it yields to anything the user is waiting on, and it backs off while Mix is live.
- The model registry records, for each model: id, version, source URL, checksum, licence, whether commercial use is allowed, and the input spec. Models download on first use into the app data folder. They are never in git.
- Each crate has typed errors. No `unwrap` outside tests. No catch-all that hides a failure. User-facing messages are produced once, at the IPC boundary.
- No hard-coded paths. Platform folders come from a library. Windows-specific code sits behind `cfg` in one module per crate.

## Data model

The tables that matter:

- `track` and `asset`. An asset belongs to a track and holds path, volume, size, mtime, audio hash, fingerprint, format facts, whether it is the primary file, and `offset_samples` and `tempo_ratio` relative to track time.
- `source` for library folders and their options.
- Tags in three layers: what is in the file, what providers proposed (with provider and confidence), and what the user set. A view gives the effective value.
- `analysis_run` (audio hash, analyser, version, parameter hash, status) and one result table per kind: beat grid, key, loudness, energy curve, phrases, embedding. Each has a confidence. Parallel tables hold user corrections.
- `cue` and `loop`, stored in track time.
- `collection`, `collection_item`, `smart_collection`, `set`, `set_item` (with play window and transition).
- `job` for the queue. `operation` for the journal: steps, inverse steps, state.
- `external_library` and `pending_external_change`.
- `session`, `session_event` (the control log) and `transition_score`.

Cues and grids are stored against track time. An asset maps to track time through its offset and ratio, and that mapping function lives in `core`. This is what lets Replace keep cues.

Migrations only go forward. Test them from every earlier schema version.

## Model runtime

Models run in-process on ONNX Runtime. Use CUDA or TensorRT on NVIDIA, CPU everywhere as fallback, CoreML when macOS arrives. Check whether DirectML is still supported before using it for other Windows GPUs.

Write STFT and mel front ends in Rust where a model needs one, so the exported graph stays simple.

On the RTX card, go for the fastest path that stays correct: TensorRT with cached engines where a model supports it, half precision where a parity test shows no loss, I/O binding so tensors stay on the GPU between steps, and batched windows. Benchmark each against plain CUDA and keep what measurably wins.

Python lives only in `tools/models/`: an export script and a parity test (ONNX output against the reference implementation, within tolerance, on fixed inputs). Nobody needs Python to run the app.

Fallback: if a model a milestone depends on can't be exported after a two-day spike, run it in an optional Python worker behind the same analyser trait, write an ADR, and keep the default install free of Python.

Where to start, with what was checked on 12 November 2026:

| Task | Start with | Licence | Notes |
| --- | --- | --- | --- |
| Beats, downbeats | Beat This! | MIT, code and weights | ONNX exports and a Rust port exist. Test drift handling on vinyl rips. |
| Stems | HTDemucs, 4 or 6 stem | MIT | ONNX exports exist that run on CPU, CUDA, CoreML and DirectML. Try RoFormer variants later if they export. |
| Key | Chroma with dance-music key profiles | n/a | Compare with a learned model. Score against the keys rekordbox gives the same tracks and a public key dataset. |
| Phrases | Bar-level novelty on top of downbeats, with 8, 16 and 32-bar priors | n/a | Probably better for 4/4 dance music than a structure model trained on pop. Benchmark against allin1 offline before deciding. |
| Text-audio embedding | A CLAP checkpoint | Check per checkpoint | Needed for "similar" and natural-language search. |
| Genre and style | Discogs-MAEST | CC BY-NC-SA 4.0 | Non-commercial, so opt-in only. |
| Music embedding | MERT | CC BY-NC 4.0 | Same. |
| Fingerprint lookup | AcoustID | Free for non-commercial apps, registration needed | A commercial plan exists. |

Evaluate on the real library and on public datasets. Don't claim accuracy without numbers, and record the hardware with every benchmark.

Bar-synchronous feature storage, continuous tuning maths, phrase compatibility and pairwise preference learning are set out in [transition-analysis.md](transition-analysis.md).

## Dependencies and licences

Only permissive licences in the dependency tree: MIT, Apache-2.0, BSD, ISC, Zlib, MPL-2.0. No GPL or AGPL linking. That keeps both doors open: the source can be opened later, or the app sold. Enforce it with `cargo-deny` in CI.

Read GPL projects such as Mixxx to learn how a controller or a file format behaves. Don't copy their code or mapping files.

Model weights with non-commercial licences are opt-in downloads with the licence shown. The default set must be usable commercially.

Generate a third-party notices file for the installer.

Keep secrets (Discogs token, AcoustID key, paid API keys) in the OS credential store. They never go in the repo, logs or the database.

## Safety while developing

On the development laptop the repo sits inside the music library. These limits apply to anyone and anything working there:

- Don't write, rename, move or delete anything under `D:\Music` outside `D:\Music\RESONANCE`. The library has about 2,700 tracks in rekordbox and it is not test data.
- For real-music tests, `just fixtures-local <music folder>` copies around 50 tracks across styles and formats into `fixtures-local/` (ignored by git). Work on the copies.
- `D:\Music\ZMusic Manager` and `%APPDATA%\com.bradd.v3` are read-only. Open any database there in read-only mode.
- Integration work uses copies of rekordbox and Engine DJ data in a sandbox folder.
- Don't install or upgrade global tools without saying what and why first.
- Don't download multi-gigabyte models without stating the size first.

## Performance budgets

These are requirements. Measure them on the development laptop in M0, record the numbers in `docs/benchmarks/`, and measure again at the end of every milestone. `just bench` takes every measurement and writes the record; [ADR 0011](adr/0011-bench-in-the-page-and-the-cli.md) says how.

| What | Budget |
| --- | --- |
| Cold start to a usable library | Under 1.5 s |
| Library table, 100,000 rows, fast scroll | Holds the display's refresh rate, no frame over 16 ms |
| Mix view: two scrolling waveforms, meters, coach strip | Holds the display's refresh rate; at 120 Hz the 99th-percentile frame is under 8 ms |
| Text search | Under 50 ms |
| UI thread | Never blocked for more than 16 ms |
| Audio | Stable at a 256-sample buffer or smaller on WASAPI exclusive or ASIO, with no dropouts in an hour |
| Idle memory with the library open | Under 400 MB |
| Wave component | Under 1 ms of UI-thread time per frame |

How the UI meets them: every real-time view draws on the GPU with WebGL2 and makes no DOM updates per frame. Meter and playhead data arrive as binary. Waveform rendering moves to a worker on an OffscreenCanvas if that measures better. Lists are virtualised. Nothing allocates per frame.

The Mix view is the risk, so M0 builds it first as a spike: two decks playing real files through the audio device, scrolling waveforms locked to the audio clock, meters over the binary channel. If it can't meet its budget after a fair attempt at tuning, stop. The fallback is a Flutter UI on the same Rust core, which is one more reason the core must not depend on the shell.

## Development loop

Day-to-day work must not rebuild the whole stack.

- UI work runs in an ordinary browser. `just ui` starts Vite, and the page talks to `resonance serve`, which is the core running as a local process on a localhost WebSocket with a random token. A UI change hot-reloads in place and no Rust is compiled.
- Core work happens in the crate being changed, with `cargo check`, its tests and the CLI. `just serve` runs the core for the page, and rebuilds and restarts it when a crate changes. A build that fails leaves the running core in place.
- `just serve` serves the library in `target/dev/data`, and `just scan <folder>` reads a music folder into it. `just synthetic-library` writes 100,000 made-up tracks into `target/dev/synthetic`, and `just serve target/dev/synthetic` serves those instead.
- The desktop shell is thin and rarely changes. Build it for smoke tests and releases.
- `just app` builds the bundled UI and opens the native shell against `target/dev/data`. Give it another folder to isolate a smoke test. The override also keeps WebView2 storage there. `just installer` builds the Windows NSIS installer; [ADR 0009](adr/0009-native-shell-and-installer.md) covers the native transport and packaging.
- Playwright drives the same browser page against the real core.

Keep Rust builds short. Split crates so a change rebuilds one of them. Build dependencies optimised and without debug info, and keep only line-table debug info for the project's own crates. The build folder and its caches stay inside the repo folder: no Dev Drive and no antivirus exclusion. `just build-times` measures a clean build and an incremental one, and the numbers go in [benchmarks/build-times.md](benchmarks/build-times.md) so a regression shows.

`sccache` and `rust-lld` as the Windows linker were both measured in M0 and neither made the incremental build faster, so neither is used. The numbers are in the same file. Measure both again when the Tauri shell joins the workspace, because its dependency tree is several times larger.

Two things were looked at and left out for now. The Cranelift backend still needs nightly, and its measured gain on a large project was in single digits. Dioxus's Subsecond hot-patching is experimental and patches only the top crate.

## What else was considered

- A native Rust UI on GPUI. It has the fastest drawing path. Upstream builds it for the Zed editor and has turned down custom-shader support, there is no hot reload for UI code, and every UI change would be a Rust rebuild.
- Qt Quick. The established native toolkit, in-process, with the best runtime numbers. Its Rust bridges are not stable yet: cxx-qt calls itself not production-ready, and Qt's own Rust bridge reached Technology Preview on 27 October 2026. Writing the core in C++ instead would give up the compiler checks that keep real-time code safe. Look again when the Qt Bridge for Rust is stable.
- Flutter with a Rust core. In-process calls, hot reload, and its own renderer (Impeller became the desktop default in Flutter 3.47, August 2026). This is the fallback named under Performance budgets. Its costs are a young desktop renderer, harder custom vertex-shader visuals and thinner tooling for UI testing.
- Electron. On Windows it draws with the same Chromium engine as WebView2, so nothing gets faster, and it uses more memory. Its advantage is one engine on every OS. Tauri 3, in alpha in late 2026, adds a bundled-Chromium (CEF) runtime that offers the same. Choose between the two when macOS and Linux work starts. The UI code is the same either way.

## Facts checked on 12 November 2026

- Tauri 2 is current and actively released (2.11.x in mid-2026). v2.tauri.app/release
- Tauri 3.0.0-alpha.0 makes the runtime selectable between wry (system webview) and CEF (bundled Chromium). newreleases.io/project/github/tauri-apps/tauri/release/tauri-v3.0.0-alpha.0
- In a published comparison, Tauri and Electron started in about the same time and Tauri used less than half the memory. dev.to/gethopp/tauri-vs-electron-performance-bundle-size-and-the-real-trade-offs-1el4
- Upstream GPUI is developed for the Zed editor, and custom-shader pull requests have been turned down. vanuan.github.io/gpui-book/chapter_9.html
- cxx-qt describes itself as not production-ready (kdab.com/cxx-qt). Qt's own Rust bridge reached Technology Preview on 27 October 2026 (qt.io/blog/qt-bridges-technology-preview-rust-csharp).
- Flutter 3.47 (12 August 2026) made Impeller the default renderer on Windows, Linux and macOS. docs.flutter.dev/perf/impeller
- Subsecond hot-patching is experimental and patches only the top crate. docs.rs/subsecond
- The Cranelift backend still needs nightly; on Zed's incremental build it measured 5 to 12 percent faster than LLVM in February 2026. rust-lang.github.io/rust-project-goals/2026/improve-cg_clif-performance.html
- `ort` on docs.rs is at 2.0.0-rc.12. docs.rs/crate/ort
- HTDemucs ONNX exports run on CPU, CoreML, CUDA and DirectML without PyTorch (pypi.org/project/demucs-onnx). Mixxx ran a 2025 project on the same conversion (mixxx.org/news/2025-10-27-gsoc2025-demucs-to-onnx-dhunstack).
- Beat This! is MIT, with ONNX exports and a Rust port. github.com/CPJKU/beat_this, docs.rs/crate/beat-this
- MERT-v1-330M is CC BY-NC 4.0 (huggingface.co/m-a-p/MERT-v1-330M). Discogs-MAEST is CC BY-NC-SA 4.0 (huggingface.co/mtg-upf/discogs-maest-30s-pw-73e-ts).
- AcoustID is free for non-commercial applications once registered. acoustid.org
- DDJ-FLX4 is USB audio and MIDI class compliant. Mixxx's mapping for it is GPL-2.0. manual.mixxx.org/2.6/en/hardware/controllers/pioneer_ddj_flx4
- Rane System One runs Engine DJ standalone and also works as a controller for Serato DJ Pro from 4.0.3, appearing as a MIDI device. support.rane.com
- rekordbox 6 and 7 keep the library in a SQLCipher-encrypted `master.db`. The `rbox` crate and pyrekordbox read and write it. docs.rs/crate/rbox, pyrekordbox.readthedocs.io
- GitHub Free has no protected branches or rulesets on private repositories (docs.github.com, "About protected branches"). It includes 2,000 Actions minutes a month for private repos, with Windows at twice the rate (docs.github.com/en/billing/concepts/product-billing/github-actions).
