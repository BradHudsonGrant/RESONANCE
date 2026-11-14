# 0011: The bench runs in the page and the CLI

Date: 13 November 2026. Status: accepted.

## Context

The budgets in [architecture.md](../architecture.md#performance-budgets) are measured again at the end of every milestone, on the same laptop and in the same way. Until now each one was measured by hand: `measureTableScroll()`, `measureWave()` and `measureMix()` from the page's console, and a button in a benchmark build of the desktop app.

Most budgets are about frames, the UI thread and the window's memory. Only the page and the operating system can see those. The audio budget is about the audio callback, which only the engine sees, and it needs no page at all.

## Decision

`just bench` builds the release CLI and the release desktop app with the benchmark frontend, then runs two steps.

`ui/dev/bench.ts` takes every measurement that needs a window and writes them to `target/bench/page.json`:

- It starts the release desktop app on a 100,000-track library with the WebView2 debugging port open, and attaches Playwright to it. The page's measurement functions run as they are; the benchmark build puts them on `window`, as the dev build does.
- Cold start runs from starting the process to a `tracks-filled` performance mark, which the table sets when its first rows fill. The bench reads the mark three seconds after launch, so attaching does not slow the start. Five launches count. A first launch before them makes a new webview profile, as the first launch after an install does.
- The UI thread is the page's main thread. Its longest task comes from Chromium traces: one from the moment the process starts, using WebView2's `--trace-startup`, one over a scroll pass and a wave run, and one over a Mix run.
- Memory is the private working set of the app and every process under it, from Windows performance counters, with the summed working set beside it.
- The Mix view only runs against `resonance serve` for now, so it is measured in a headed Edge window against its own core and Vite on port 5181, with two MP3s from `fixtures-local` playing.

`resonance bench` does the rest and writes the record:

- It plays the same two tracks on both decks through the engine for three minutes, with no page open, and reads the callback's counters.
- It reads the processor, memory and system with `sysinfo`, and the commit and compiler with `git` and `rustc`.
- It writes `docs/benchmarks/budgets-v<version>.md`: a result for every budget, then the figures behind each one.
- Text search is listed as not measurable until M2 builds it.

## Why

Attaching over the debugging port measures the release build in the real webview, with the same measurement code as the hand runs and nothing added to the app but one performance mark.

Playing audio from the CLI keeps the browser, the page and the GPU out of the audio figures, and the run can be as long as asked. The Mix runs still report the callback's counters, so audio is also seen with the view drawing.

The CLI writes the record because it sees both halves: its own audio run and the page's file.

Summed working sets count every shared page once for each process that maps it, which is why the 455 MiB in [native-shell.md](../benchmarks/native-shell.md) could not settle the memory budget. The private working set counts each page once and is the figure Task Manager shows. Both are recorded.

## Dependencies

Checked on 13 November 2026:

- `sysinfo` 0.40.3, MIT, with only its `system` feature. Released 11 November 2026; the repository had commits on 12 November 2026.
- `ntapi` 0.4.3, MIT or Apache-2.0, which `sysinfo` uses on Windows. Released 10 February 2026.
- `objc2-io-kit` 0.3.2, Zlib, MIT or Apache-2.0, which `sysinfo` uses on macOS only.

## What follows from it

- The bench runs on Windows only. It uses WebView2's debugging port, `taskkill` and Windows performance counters. macOS and Linux need their own ways to attach and to read memory when that work starts.
- It needs `fixtures-local` with two MP3s of at least four minutes.
- A run opens windows on screen and plays audio for about four minutes, so it is not part of `just verify` or CI.
- Cold start is measured with the files in the disk cache. A disk-cold start needs the cache emptied, which needs administrator tools.
- The audio run is three minutes, not the hour the budget names, so audio is recorded as not yet shown until a run of an hour is clean at 256 frames.
- Tracing adds a little to every task, so the UI-thread figures are upper bounds.

## What would change it

The Mix view running in the desktop app, once the native transport has a binary channel in M7, moves its measurement there. A second platform needs its own launch and memory steps in `ui/dev/bench.ts`.
