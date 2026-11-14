# Budgets at v0.1.0

Every budget in [architecture.md](../architecture.md#performance-budgets), measured on 13 November 2026 by `just bench`. It runs `ui/dev/bench.ts` for everything that needs a window, then `resonance bench` for the audio and this record. [ADR 0011](../adr/0011-bench-in-the-page-and-the-cli.md) says how each figure is taken.

| Budget | Result | Measured |
| --- | --- | --- |
| Cold start to a usable library, under 1.5 s | Met | 0.41 s at the longest of 5 launches, 100,000 tracks |
| Library table, 100,000 rows, fast scroll: holds the refresh rate, no frame over 16 ms | Missed | 1 missed frame in 6,000, longest 13.6 ms |
| Mix view: holds the refresh rate, 99th percentile under 8 ms at 120 Hz | Missed | 99th percentile 20.7 ms, 285 missed frames in 6,000, 1708 by 789 canvas |
| Text search, under 50 ms | Not measurable yet | Search is built in M2 |
| UI thread never blocked for more than 16 ms | Missed | Longest task 22.0 ms, while the desktop app starts |
| Audio stable at a 256-sample buffer or smaller, no dropouts in an hour | Missed | 480-frame period, 0 dropouts in 180 s |
| Idle memory with the library open, under 400 MB | Met | 101 MB private working set, 397 MB working set |
| Wave component, under 1 ms of UI-thread time per frame | Met | Longest 0.2 ms in 3,000 frames |

## Machine and versions

| | |
| --- | --- |
| Processor | Intel(R) Core(TM) Ultra 9 285H, 16 logical cores |
| Memory | 64 GB |
| System | Windows 11 Pro, build 26300 |
| Graphics in the desktop app | ANGLE (NVIDIA, NVIDIA GeForce RTX 5090 Laptop GPU (0x00002C58) Direct3D11 vs_5_0 ps_5_0, D3D11) |
| Graphics in Edge | ANGLE (Intel, Intel(R) Arc(TM) 140T GPU (32GB) (0x00007D51) Direct3D11 vs_5_0 ps_5_0, D3D11) |
| Display | 6.8 ms a frame, about 147 Hz |
| Audio device | Speakers (3- ADAM Audio D3V ), 48 kHz |
| Resonance | 0.1.0, commit 33d3068 |
| Rust | rustc 1.100.0 (0ef1bc386 2026-11-09) |
| WebView2 | 157.0.4341.46 |
| Edge | 157.0.4341.46 |
| Node | v24.13.1 |

## Cold start

From starting `resonance-app.exe` to the first filled rows of a 100,000-track library, with its files already in the disk cache. The table marks the moment its first rows fill, and the bench reads the mark three seconds later, so attaching to the page adds nothing. The marked rows are painted in the frame that follows.

| Launch | To the page starting | To filled rows |
| --- | --- | --- |
| First, with a new webview profile | 0.62 s | 0.69 s |
| 1 | 0.30 s | 0.38 s |
| 2 | 0.32 s | 0.40 s |
| 3 | 0.33 s | 0.40 s |
| 4 | 0.33 s | 0.41 s |
| 5 | 0.30 s | 0.38 s |

The budget is judged on the numbered launches. The first launch after an install also makes the webview's profile.

## Library table

Five runs of `measureTableScroll()` in the desktop app at 1440 by 900 CSS pixels and a device pixel ratio of 1.25, as in [library-table.md](library-table.md). Each pass is a fling or a run of jumps. A frame is missed when it lasts more than one and a half display frames.

| Run | Pass | Median frame | 99th percentile | Longest frame | Missed frames | Frames with a loading row |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | Fling | 6.8 ms | 6.9 ms | 7.8 ms | 0 | 0 |
| 1 | Jumps | 6.8 ms | 6.9 ms | 7.0 ms | 0 | 525 |
| 2 | Fling | 6.8 ms | 6.9 ms | 7.9 ms | 0 | 0 |
| 2 | Jumps | 6.8 ms | 6.9 ms | 6.9 ms | 0 | 532 |
| 3 | Fling | 6.8 ms | 6.9 ms | 7.0 ms | 0 | 0 |
| 3 | Jumps | 6.8 ms | 6.9 ms | 7.0 ms | 0 | 531 |
| 4 | Fling | 6.8 ms | 6.9 ms | 7.0 ms | 0 | 0 |
| 4 | Jumps | 6.8 ms | 6.9 ms | 13.6 ms | 1 | 532 |
| 5 | Fling | 6.8 ms | 6.9 ms | 7.1 ms | 0 | 0 |
| 5 | Jumps | 6.8 ms | 7.0 ms | 7.1 ms | 0 | 541 |

After the last jump, every rendered row was filled in 0.0, 0.0, 26.5, 26.0, 13.0 ms.

## Mix view

Five runs of `measureMix()` in a headed Edge window with a page of 1418 by 810 CSS pixels at a device pixel ratio of 1.25 and a canvas of 1708 by 789, while both decks played two MP3s from `fixtures-local`. The method is in [mix-view.md](mix-view.md). The view runs against `resonance serve` for now, so it is measured in the browser and not in the desktop app.

| Run | Refresh | 99th percentile | Longest | Missed | View, longest | Callbacks | Underruns | Longest callback |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 6.8 ms | 20.7 ms | 41.2 ms | 207 | 0.2 ms | 1,070 | 0 | 0.0 ms |
| 2 | 6.8 ms | 20.6 ms | 20.8 ms | 52 | 0.3 ms | 872 | 0 | 0.0 ms |
| 3 | 6.8 ms | 6.9 ms | 13.6 ms | 2 | 0.2 ms | 812 | 0 | 0.1 ms |
| 4 | 6.8 ms | 13.5 ms | 13.7 ms | 16 | 0.2 ms | 824 | 0 | 0.1 ms |
| 5 | 6.8 ms | 7.0 ms | 13.7 ms | 8 | 0.2 ms | 817 | 0 | 0.0 ms |

Late callbacks, starved callbacks and stream errors across the runs: 0.

## UI thread

The longest task on the page's main thread in a Chromium trace. Tracing adds a little to every task, so these are upper bounds.

| While | Longest task |
| --- | --- |
| The desktop app starts, traced for its first 5 s | 22.0 ms |
| The desktop app scrolls the table once and draws the wave for 600 frames | 11.6 ms |
| The Mix view in Edge draws 1,200 frames with both decks playing | 1.1 ms |

## Audio

Both decks played the same two MP3s through the default output device for 180 s, with no page open. The counts come from the audio callback, as in [mix-view.md](mix-view.md).

| | |
| --- | --- |
| Period | 480 frames, 256 asked for |
| Device buffer | 1,056 frames |
| Callbacks | 18,007 |
| Frames asked for in a callback | 480 to 480 |
| Underruns | 0 (0 frames) |
| Late callbacks | 0 |
| Starved callbacks | 0 |
| Stream errors | 0 |
| Longest callback | 0.1 ms |

## Memory

The desktop app and every process under it: the webview's browser, GPU, renderer and helper processes. The private working set counts each process's own pages and leaves out pages it shares. The working set adds shared pages once for every process that maps them.

| When | Processes | Private working set | Working set |
| --- | --- | --- | --- |
| Idle with the library open, 10 s after launch | 7 | 101 MB | 397 MB |
| After the scroll and wave runs | 8 | 383 MB | 750 MB |

## Wave

Five runs of `measureWave()` in the desktop app, as in [wave.md](wave.md). The page's timer reads in steps of 0.1 ms, so 0.0 means under 0.1 ms.

| Run | Median | 99th percentile | Longest |
| --- | --- | --- | --- |
| 1 | 0.0 ms | 0.2 ms | 0.2 ms |
| 2 | 0.0 ms | 0.1 ms | 0.2 ms |
| 3 | 0.0 ms | 0.1 ms | 0.2 ms |
| 4 | 0.0 ms | 0.1 ms | 0.2 ms |
| 5 | 0.0 ms | 0.1 ms | 0.2 ms |

## Where this leaves the budgets

Cold start, idle memory and the wave meet their budgets. Text search is built in M2. The other four are missed:

- Library table: one missed frame in 6,000, and no frame near 16 ms. The budget stays open in [#40](https://github.com/BradHudsonGrant/RESONANCE/issues/40).
- Mix view: frames missed in every run, while the view's own work took at most 0.3 ms a frame. Edge drew on the Intel Arc 140T and the desktop app on the RTX 5090. The budget stays open in [#46](https://github.com/BradHudsonGrant/RESONANCE/issues/46).
- UI thread: the longest task, 22.0 ms, is while the desktop app starts. In this run, scrolling, the wave and the Mix view stay under 16 ms. This is new, and open in [#55](https://github.com/BradHudsonGrant/RESONANCE/issues/55).
- Audio: no dropouts in three minutes at the device's 480-frame period. A 256-frame buffer needs ASIO or WASAPI exclusive mode, open in [#47](https://github.com/BradHudsonGrant/RESONANCE/issues/47).

### Other runs the same day

This record is the third full run of the day, with the same app code. The first ran the Mix view with Edge emulating a device pixel ratio of 1, so its Mix figures are left out. In the second, the idle refresh read 10.0 and 7.5 ms on the first two table runs instead of 6.7 ms, and two Mix runs took 40 to 60% longer than the rest, so the machine was busy with something else for part of it.

| Run | Table: missed frames in 6,000, longest | Mix: missed frames in 6,000, 99th percentile | Longest task: starting, scrolling, Mix |
| --- | --- | --- | --- |
| First | 19, 13.5 ms | not comparable | 22.8, 15.0, 1.0 ms |
| Second | 836, 31.4 ms | 750, 32.8 ms | 21.8, 42.3, 1.7 ms |
| This record | 1, 13.6 ms | 285, 20.7 ms | 22.0, 11.6, 1.1 ms |

### UI thread at startup

A detailed startup trace showed two long tasks. In the first, 22.6 ms, the module script ran for 10.2 ms and Blink then forced a style and layout of the whole page for 9.5 ms as parsing finished. In the second, 18.4 ms, the first rows rendered when their page arrived.

One change was tried on a `perf/` branch: sizing the table with a ResizeObserver in place of `bind:clientHeight` and `bind:offsetHeight`, which read the size while the table mounts. The second task fell to 12.2 ms, but the forced layout in the first stayed at 8.8 ms and the longest task was 22.2 ms. It was not kept. What to try next is in [#55](https://github.com/BradHudsonGrant/RESONANCE/issues/55).
