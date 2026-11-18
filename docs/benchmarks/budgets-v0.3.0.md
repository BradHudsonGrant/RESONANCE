# Budgets at v0.3.0

Every budget in [architecture.md](../architecture.md#performance-budgets), measured on 18 November 2026 by `just bench`. It runs `ui/dev/bench.ts` for everything that needs a window, then `resonance bench` for the audio and this record. [ADR 0011](../adr/0011-bench-in-the-page-and-the-cli.md) says how each figure is taken.

| Budget | Result | Measured |
| --- | --- | --- |
| Cold start to a usable library, under 1.5 s | Met | 0.48 s at the longest of 5 launches, 100,000 tracks |
| Library table, 100,000 rows, fast scroll: median at the refresh rate, no frame over 16 ms | Met | 61 missed frames in 6,000, longest 12.2 ms |
| Mix view: median at the refresh rate, 99th percentile within two intervals, no frame over 16 ms | Met | 99th percentile 12.1 ms, longest 12.3 ms, 62 missed frames in 6,000, 1401 by 276 canvas |
| Text search, under 50 ms | Met | 25.4 ms at the worst 95th percentile across search, filter and sort queries on 100,000 tracks, measured apart from `just bench` in [library-table.md](library-table.md) |
| UI thread never blocked for more than 16 ms | Missed | Longest task 20.2 ms, while the desktop app starts |
| Audio stable at a 256-sample buffer or smaller, no dropouts in an hour | Not yet shown | 256-frame period, 0 dropouts in 180 s |
| Idle memory with the library open, under 400 MB | Met | 136 MB private working set, 424 MB working set |
| Wave component, under 1 ms of UI-thread time per frame | Met | Longest 0.5 ms in 3,000 frames |

Two budgets are not met yet and carry over to `v0.4.0`. The startup task is [#55](https://github.com/BradHudsonGrant/RESONANCE/issues/55) and the hour at a 256-sample buffer is [#47](https://github.com/BradHudsonGrant/RESONANCE/issues/47).

## Machine and versions

| | |
| --- | --- |
| Processor | Intel(R) Core(TM) Ultra 9 285H, 16 logical cores |
| Memory | 63 GB |
| System | Windows 11 Pro, build 26300 |
| Graphics in the desktop app | ANGLE (NVIDIA, NVIDIA GeForce RTX 5090 Laptop GPU (0x00002C58) Direct3D11 vs_5_0 ps_5_0, D3D11) |
| Graphics in Edge | ANGLE (NVIDIA, NVIDIA GeForce RTX 5090 Laptop GPU (0x00002C58) Direct3D11 vs_5_0 ps_5_0, D3D11) |
| Display | 6.1 ms a frame, about 164 Hz |
| Audio device | Speakers (3- ADAM Audio D3V ), 48 kHz |
| Resonance | 0.3.0, commit 6384bbc with uncommitted changes |
| Rust | rustc 1.100.0 (0ef1bc386 2026-11-09) |
| WebView2 | 157.0.4341.60 |
| Edge | 157.0.4341.60 |
| Node | v24.13.1 |

## Cold start

From starting `resonance-app.exe` to the first filled rows of a 100,000-track library, with its files already in the disk cache. The table marks the moment its first rows fill, and the bench reads the mark three seconds later, so attaching to the page adds nothing. The marked rows are painted in the frame that follows.

| Launch | To the page starting | To filled rows |
| --- | --- | --- |
| First, with a new webview profile | 0.37 s | 0.52 s |
| 1 | 0.32 s | 0.47 s |
| 2 | 0.33 s | 0.47 s |
| 3 | 0.35 s | 0.48 s |
| 4 | 0.30 s | 0.47 s |
| 5 | 0.30 s | 0.48 s |

The budget is judged on the numbered launches. The first launch after an install also makes the webview's profile.

## Library table

Five runs of `measureTableScroll()` in the desktop app at 1440 by 900 CSS pixels and a device pixel ratio of 1.25, as in [library-table.md](library-table.md). Each pass is a fling or a run of jumps. A frame is missed when it lasts more than one and a half display frames.

| Run | Pass | Median frame | 99th percentile | Longest frame | Missed frames | Frames with a loading row |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | Fling | 6.1 ms | 6.2 ms | 12.1 ms | 1 | 0 |
| 1 | Jumps | 6.1 ms | 12.1 ms | 12.2 ms | 11 | 549 |
| 2 | Fling | 6.1 ms | 12.0 ms | 12.2 ms | 6 | 0 |
| 2 | Jumps | 6.1 ms | 12.1 ms | 12.2 ms | 14 | 535 |
| 3 | Fling | 6.1 ms | 6.3 ms | 12.1 ms | 1 | 0 |
| 3 | Jumps | 6.1 ms | 6.2 ms | 12.2 ms | 5 | 528 |
| 4 | Fling | 6.1 ms | 6.2 ms | 12.2 ms | 1 | 0 |
| 4 | Jumps | 6.1 ms | 12.0 ms | 12.2 ms | 8 | 528 |
| 5 | Fling | 6.1 ms | 12.1 ms | 12.2 ms | 11 | 0 |
| 5 | Jumps | 6.1 ms | 6.3 ms | 12.2 ms | 3 | 526 |

After the last jump, every rendered row was filled in 41.4, 53.7, 60.1, 51.4, 23.1 ms.

## Mix view

Five runs of `measureMix()` in a headed Edge window with a page of 1418 by 810 CSS pixels at a device pixel ratio of 1.25 and a canvas of 1401 by 276, while both decks played two MP3s from `fixtures-local`. The method is in [mix-view.md](mix-view.md). The view runs against `resonance serve` for now, so it is measured in the browser and not in the desktop app.

| Run | Refresh | 99th percentile | Longest | Missed | View, longest | Callbacks | Underruns | Longest callback |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 6.1 ms | 12.1 ms | 12.2 ms | 17 | 0.2 ms | 736 | 0 | 0.2 ms |
| 2 | 6.1 ms | 6.3 ms | 12.2 ms | 5 | 0.3 ms | 728 | 0 | 0.1 ms |
| 3 | 6.1 ms | 6.2 ms | 12.2 ms | 6 | 0.2 ms | 728 | 0 | 0.2 ms |
| 4 | 6.1 ms | 12.0 ms | 12.2 ms | 12 | 0.2 ms | 734 | 0 | 0.0 ms |
| 5 | 6.1 ms | 12.1 ms | 12.3 ms | 22 | 0.2 ms | 740 | 0 | 0.0 ms |

Late callbacks, starved callbacks and stream errors across the runs: 0.

## UI thread

The longest task on the page's main thread in a Chromium trace. Tracing adds a little to every task, so these are upper bounds.

| While | Longest task |
| --- | --- |
| The desktop app starts, traced for its first 5 s | 20.2 ms |
| The desktop app scrolls the table once and draws the wave for 600 frames | 14.0 ms |
| The Mix view in Edge draws 1,200 frames with both decks playing | 1.1 ms |

## Audio

Both decks played the same two MP3s through the default output device for 180 s, with no page open. The counts come from the audio callback, as in [mix-view.md](mix-view.md).

| | |
| --- | --- |
| Mode | Exclusive |
| Period | 256 frames, 256 asked for |
| Device buffer | 256 frames |
| Callbacks | 33,764 |
| Frames asked for in a callback | 256 to 256 |
| Underruns | 0 (0 frames) |
| Late callbacks | 0 |
| Starved callbacks | 0 |
| Stream errors | 0 |
| Longest callback | 0.3 ms |

## Memory

The desktop app and every process under it: the webview's browser, GPU, renderer and helper processes. The private working set counts each process's own pages and leaves out pages it shares. The working set adds shared pages once for every process that maps them.

| When | Processes | Private working set | Working set |
| --- | --- | --- | --- |
| Idle with the library open, 10 s after launch | 7 | 136 MB | 424 MB |
| After the scroll and wave runs | 8 | 372 MB | 730 MB |

## Wave

Five runs of `measureWave()` in the desktop app, as in [wave.md](wave.md). The page's timer reads in steps of 0.1 ms, so 0.0 means under 0.1 ms.

| Run | Median | 99th percentile | Longest |
| --- | --- | --- | --- |
| 1 | 0.0 ms | 0.1 ms | 0.2 ms |
| 2 | 0.0 ms | 0.1 ms | 0.2 ms |
| 3 | 0.0 ms | 0.1 ms | 0.2 ms |
| 4 | 0.0 ms | 0.1 ms | 0.1 ms |
| 5 | 0.0 ms | 0.1 ms | 0.5 ms |
