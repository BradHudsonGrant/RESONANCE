# Budgets at v0.5.0

Every budget in [architecture.md](../architecture.md#performance-budgets), measured on 26 November 2026 by `just bench`. It runs `ui/dev/bench.ts` for everything that needs a window, then `resonance bench` for the audio and this record. [ADR 0011](../adr/0011-bench-in-the-page-and-the-cli.md) says how each figure is taken.

| Budget | Result | Measured |
| --- | --- | --- |
| Cold start to a usable library, under 1.5 s | Met | 0.53 s at the longest of 5 launches, 100,000 tracks |
| Library table, 100,000 rows, fast scroll: median at the refresh rate, no frame over 16 ms | Met | 302 missed frames in 6,000, longest 12.5 ms |
| Mix view: median at the refresh rate, 99th percentile within two intervals, no frame over 16 ms | Met | 99th percentile 4.3 ms, longest 4.6 ms, 0 missed frames in 6,000, 1401 by 276 canvas |
| Text search, under 50 ms | Met | 29.5 ms at the worst 95th percentile, a one-letter search matching 23,363 of 100,000 tracks |
| UI thread never blocked for more than 16 ms | Met | Longest task 11.4 ms, while the desktop app starts |
| Audio stable at a 256-sample buffer or smaller, no dropouts in an hour | Met | 256-frame period, 0 dropouts in 180 s here, and 0 underruns in a full hour on 18 November in [mix-view.md](mix-view.md) |
| Idle memory with the library open, under 400 MB | Met | 138 MB private working set, 450 MB working set |
| Wave component, under 1 ms of UI-thread time per frame | Met | Longest 0.3 ms in 3,000 frames |

Every budget is met. The startup task measured 11.4 ms, down from 35.5 ms at v0.4.0, which meets [#154](https://github.com/BradHudsonGrant/RESONANCE/issues/154).

## Machine and versions

| | |
| --- | --- |
| Processor | Intel(R) Core(TM) Ultra 9 285H, 16 logical cores |
| Memory | 63 GB |
| System | Windows 11 Pro, build 26300 |
| Graphics in the desktop app | ANGLE (NVIDIA, NVIDIA GeForce RTX 5090 Laptop GPU (0x00002C58) Direct3D11 vs_5_0 ps_5_0, D3D11) |
| Graphics in Edge | ANGLE (NVIDIA, NVIDIA GeForce RTX 5090 Laptop GPU (0x00002C58) Direct3D11 vs_5_0 ps_5_0, D3D11) |
| Display | 4.2 ms a frame, about 238 Hz |
| Audio device | Laptop Speakers (Realtek(R) Audio), 48 kHz |
| Resonance | 0.5.0, built from ca12e77 with the version set |
| Rust | rustc 1.100.0 (0ef1bc386 2026-11-09) |
| WebView2 | 158.0.4370.33 |
| Edge | 158.0.4370.33 |
| Node | v24.13.1 |

## Cold start

From starting `resonance-app.exe` to the first filled rows of a 100,000-track library, with its files already in the disk cache. The table marks the moment its first rows fill, and the bench reads the mark three seconds later, so attaching to the page adds nothing. The marked rows are painted in the frame that follows.

| Launch | To the page starting | To filled rows |
| --- | --- | --- |
| First, with a new webview profile | 15.64 s | 15.79 s |
| 1 | 0.36 s | 0.51 s |
| 2 | 0.38 s | 0.53 s |
| 3 | 0.36 s | 0.50 s |
| 4 | 0.36 s | 0.52 s |
| 5 | 0.36 s | 0.50 s |

The budget is judged on the numbered launches. The first launch after an install also makes the webview's profile.

## Library table

Five runs of `measureTableScroll()` in the desktop app at 1440 by 900 CSS pixels and a device pixel ratio of 1.25, as in [library-table.md](library-table.md). Each pass is a fling or a run of jumps. A frame is missed when it lasts more than one and a half display frames.

| Run | Pass | Median frame | 99th percentile | Longest frame | Missed frames | Frames with a loading row |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | Fling | 4.2 ms | 8.4 ms | 8.5 ms | 47 | 0 |
| 1 | Jumps | 4.2 ms | 8.3 ms | 8.4 ms | 9 | 539 |
| 2 | Fling | 4.2 ms | 8.4 ms | 12.5 ms | 56 | 0 |
| 2 | Jumps | 4.2 ms | 8.2 ms | 8.5 ms | 8 | 532 |
| 3 | Fling | 4.2 ms | 8.4 ms | 8.5 ms | 48 | 0 |
| 3 | Jumps | 4.2 ms | 8.3 ms | 8.4 ms | 9 | 521 |
| 4 | Fling | 4.2 ms | 8.4 ms | 12.5 ms | 52 | 0 |
| 4 | Jumps | 4.2 ms | 8.3 ms | 8.4 ms | 9 | 530 |
| 5 | Fling | 4.2 ms | 8.4 ms | 12.5 ms | 56 | 0 |
| 5 | Jumps | 4.2 ms | 8.3 ms | 8.4 ms | 8 | 526 |

The display ran at 238 Hz, where it ran at 179 Hz for v0.4.0, so a frame counts as missed past 6.3 ms rather than 8.4 ms. That is why there are more missed frames. The median held the refresh rate and the longest frame was 12.5 ms.

After the last jump, every rendered row was filled in 48.7, 18.9, 24.6, 0.0, 18.0 ms.

## Text search

Each query went 20 times over the WebSocket to a release `resonance serve` on the 100,000-track library, after one cold call. Times are full round trips for the first 200 rows, which now include each row's worst quality flag.

| Query | Hits | Cold | Median | 95th percentile |
| --- | --- | --- | --- | --- |
| Text `island field` | 263 | 10.4 ms | 7.4 ms | 7.9 ms |
| Text `a` | 23,363 | 26.1 ms | 27.0 ms | 29.5 ms |
| Text `zzznomatch` | 0 | 2.4 ms | 1.4 ms | 1.8 ms |
| Favourites | 0 | 1.4 ms | 1.3 ms | 1.4 ms |
| No query | 100,000 | 6.2 ms | 5.2 ms | 5.4 ms |

## Mix view

Five runs of `measureMix()` in a headed Edge window with a page of 1418 by 810 CSS pixels at a device pixel ratio of 1.25 and a canvas of 1401 by 276, while both decks played two MP3s from `fixtures-local`. The method is in [mix-view.md](mix-view.md). The view runs against `resonance serve` for now, so it is measured in the browser and not in the desktop app.

| Run | Refresh | 99th percentile | Longest | Missed | View, longest | Callbacks | Underruns | Longest callback |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 4.2 ms | 4.3 ms | 4.4 ms | 0 | 0.2 ms | 501 | 0 | 0.0 ms |
| 2 | 4.2 ms | 4.3 ms | 4.6 ms | 0 | 0.2 ms | 498 | 0 | 0.0 ms |
| 3 | 4.2 ms | 4.3 ms | 4.4 ms | 0 | 0.2 ms | 498 | 0 | 0.1 ms |
| 4 | 4.2 ms | 4.3 ms | 4.4 ms | 0 | 0.3 ms | 498 | 0 | 0.0 ms |
| 5 | 4.2 ms | 4.3 ms | 4.5 ms | 0 | 0.3 ms | 498 | 0 | 0.0 ms |

Late callbacks, starved callbacks and stream errors across the runs: 0.

## UI thread

The longest task on the page's main thread in a Chromium trace. Tracing adds a little to every task, so these are upper bounds.

| While | Longest task |
| --- | --- |
| The desktop app starts, traced for its first 5 s | 11.4 ms |
| The desktop app scrolls the table once and draws the wave for 600 frames | 11.1 ms |
| The Mix view in Edge draws 1,200 frames with both decks playing | 1.7 ms |

## Audio

Both decks played the same two MP3s through the default output device for 180 s, with no page open. The counts come from the audio callback, as in [mix-view.md](mix-view.md).

| | |
| --- | --- |
| Mode | Exclusive |
| Period | 256 frames, 256 asked for |
| Device buffer | 256 frames |
| Callbacks | 33,751 |
| Frames asked for in a callback | 256 to 256 |
| Underruns | 0 (0 frames) |
| Late callbacks | 0 |
| Starved callbacks | 0 |
| Stream errors | 0 |
| Longest callback | 0.2 ms |

## Memory

The desktop app and every process under it: the webview's browser, GPU, renderer and helper processes. The private working set counts each process's own pages and leaves out pages it shares. The working set adds shared pages once for every process that maps them.

| When | Processes | Private working set | Working set |
| --- | --- | --- | --- |
| Idle with the library open, 10 s after launch | 7 | 138 MB | 450 MB |
| After the scroll and wave runs | 8 | 432 MB | 839 MB |

## Wave

Five runs of `measureWave()` in the desktop app, as in [wave.md](wave.md). The page's timer reads in steps of 0.1 ms, so 0.0 means under 0.1 ms.

| Run | Median | 99th percentile | Longest |
| --- | --- | --- | --- |
| 1 | 0.0 ms | 0.2 ms | 0.3 ms |
| 2 | 0.0 ms | 0.1 ms | 0.2 ms |
| 3 | 0.0 ms | 0.1 ms | 0.2 ms |
| 4 | 0.0 ms | 0.1 ms | 0.1 ms |
| 5 | 0.0 ms | 0.1 ms | 0.2 ms |
