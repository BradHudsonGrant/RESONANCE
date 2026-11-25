# Budgets at v0.4.0

Every budget in [architecture.md](../architecture.md#performance-budgets), measured on 25 November 2026 by the steps of `just bench`. They run `ui/dev/bench.ts` for everything that needs a window, then `resonance bench` for the audio and this record. [ADR 0011](../adr/0011-bench-in-the-page-and-the-cli.md) says how each figure is taken.

| Budget                                                                                          | Result                            | Measured                                                                                                                                                          |
| ----------------------------------------------------------------------------------------------- | --------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Cold start to a usable library, under 1.5 s                                                     | Met                               | 1.40 s at the longest of 5 launches, 100,000 tracks                                                                                                               |
| Library table, 100,000 rows, fast scroll: median at the refresh rate, no frame over 16 ms       | Met                               | 37 missed frames in 6,000, longest 12.0 ms                                                                                                                        |
| Mix view: median at the refresh rate, 99th percentile within two intervals, no frame over 16 ms | Met                               | 99th percentile 6.7 ms, longest 7.0 ms, 0 missed frames in 6,000, 1401 by 276 canvas                                                                              |
| Text search, under 50 ms                                                                        | Met at v0.3.0, not measured again | 25.4 ms at the worst 95th percentile across search, filter and sort queries on 100,000 tracks in [library-table.md](library-table.md); M3 changed no search query |
| UI thread never blocked for more than 16 ms                                                     | Missed                            | Longest task 35.5 ms, while the desktop app starts                                                                                                                |
| Audio stable at a 256-sample buffer or smaller, no dropouts in an hour                          | Met                               | 256-frame period, 0 dropouts in 180 s here, and 0 underruns in a full hour on 18 November in [mix-view.md](mix-view.md)                                           |
| Idle memory with the library open, under 400 MB                                                 | Met                               | 139 MB private working set, 450 MB working set                                                                                                                    |
| Wave component, under 1 ms of UI-thread time per frame                                          | Met                               | Longest 0.6 ms in 3,000 frames                                                                                                                                    |

One budget is missed. The startup task measured 35.5 ms, up from 11.2 to 12.5 ms in the traces taken for [#141](https://github.com/BradHudsonGrant/RESONANCE/pull/141), and carries over to `v0.5.0` as [#154](https://github.com/BradHudsonGrant/RESONANCE/issues/154).

## Machine and versions

|                             |                                                                                                 |
| --------------------------- | ----------------------------------------------------------------------------------------------- |
| Processor                   | Intel(R) Core(TM) Ultra 9 285H, 16 logical cores                                                |
| Memory                      | 63 GB                                                                                           |
| System                      | Windows 11 Pro, build 26300                                                                     |
| Graphics in the desktop app | ANGLE (NVIDIA, NVIDIA GeForce RTX 5090 Laptop GPU (0x00002C58) Direct3D11 vs_5_0 ps_5_0, D3D11) |
| Graphics in Edge            | ANGLE (NVIDIA, NVIDIA GeForce RTX 5090 Laptop GPU (0x00002C58) Direct3D11 vs_5_0 ps_5_0, D3D11) |
| Display                     | 5.6 ms a frame, about 179 Hz                                                                    |
| Audio device                | CABLE Input (VB-Audio Virtual Cable), 48 kHz                                                    |
| Resonance                   | 0.4.0, built from bf6c89d with the version set                                                  |
| Rust                        | rustc 1.100.0 (0ef1bc386 2026-11-09)                                                            |
| WebView2                    | 158.0.4370.33                                                                                   |
| Edge                        | 158.0.4370.33                                                                                   |
| Node                        | v24.13.1                                                                                        |

## Cold start

From starting `resonance-app.exe` to the first filled rows of a 100,000-track library, with its files already in the disk cache. The table marks the moment its first rows fill, and the bench reads the mark three seconds later, so attaching to the page adds nothing. The marked rows are painted in the frame that follows.

| Launch                            | To the page starting | To filled rows |
| --------------------------------- | -------------------- | -------------- |
| First, with a new webview profile | 1.07 s               | 1.69 s         |
| 1                                 | 1.01 s               | 1.33 s         |
| 2                                 | 1.03 s               | 1.37 s         |
| 3                                 | 1.01 s               | 1.36 s         |
| 4                                 | 1.05 s               | 1.28 s         |
| 5                                 | 1.06 s               | 1.40 s         |

The budget is judged on the numbered launches. The first launch after an install also makes the webview's profile.

## Library table

Five runs of `measureTableScroll()` in the desktop app at 1440 by 900 CSS pixels and a device pixel ratio of 1.25, as in [library-table.md](library-table.md). Each pass is a fling or a run of jumps. A frame is missed when it lasts more than one and a half display frames.

| Run | Pass  | Median frame | 99th percentile | Longest frame | Missed frames | Frames with a loading row |
| --- | ----- | ------------ | --------------- | ------------- | ------------- | ------------------------- |
| 1   | Fling | 5.6 ms       | 6.6 ms          | 11.9 ms       | 3             | 0                         |
| 1   | Jumps | 5.7 ms       | 6.5 ms          | 8.1 ms        | 0             | 540                       |
| 2   | Fling | 5.7 ms       | 10.9 ms         | 11.7 ms       | 7             | 0                         |
| 2   | Jumps | 5.7 ms       | 6.5 ms          | 11.0 ms       | 1             | 532                       |
| 3   | Fling | 5.7 ms       | 11.1 ms         | 12.0 ms       | 7             | 0                         |
| 3   | Jumps | 5.7 ms       | 6.5 ms          | 6.7 ms        | 0             | 537                       |
| 4   | Fling | 5.6 ms       | 11.2 ms         | 11.8 ms       | 6             | 0                         |
| 4   | Jumps | 5.7 ms       | 6.5 ms          | 11.2 ms       | 1             | 532                       |
| 5   | Fling | 5.7 ms       | 11.3 ms         | 12.0 ms       | 10            | 0                         |
| 5   | Jumps | 5.6 ms       | 6.5 ms          | 11.6 ms       | 2             | 533                       |

After the last jump, every rendered row was filled in 22.0, 37.2, 28.3, 27.4, 27.4 ms.

## Mix view

Five runs of `measureMix()` in a headed Edge window with a page of 1418 by 810 CSS pixels at a device pixel ratio of 1.25 and a canvas of 1401 by 276, while both decks played two MP3s from `fixtures-local`. The method is in [mix-view.md](mix-view.md). The view runs against `resonance serve` for now, so it is measured in the browser and not in the desktop app.

| Run | Refresh | 99th percentile | Longest | Missed | View, longest | Callbacks | Underruns | Longest callback |
| --- | ------- | --------------- | ------- | ------ | ------------- | --------- | --------- | ---------------- |
| 1   | 5.3 ms  | 6.6 ms          | 6.9 ms  | 0      | 0.2 ms        | 1,212     | 0         | 0.1 ms           |
| 2   | 5.3 ms  | 6.6 ms          | 7.0 ms  | 0      | 0.2 ms        | 1,215     | 0         | 0.1 ms           |
| 3   | 5.3 ms  | 6.6 ms          | 6.7 ms  | 0      | 0.2 ms        | 1,219     | 0         | 0.0 ms           |
| 4   | 5.3 ms  | 6.6 ms          | 7.0 ms  | 0      | 0.2 ms        | 1,215     | 0         | 0.0 ms           |
| 5   | 5.3 ms  | 6.7 ms          | 6.8 ms  | 0      | 0.2 ms        | 1,219     | 0         | 0.0 ms           |

Late callbacks, starved callbacks and stream errors across the runs: 0.

## UI thread

The longest task on the page's main thread in a Chromium trace. Tracing adds a little to every task, so these are upper bounds.

| While                                                                    | Longest task |
| ------------------------------------------------------------------------ | ------------ |
| The desktop app starts, traced for its first 5 s                         | 35.5 ms      |
| The desktop app scrolls the table once and draws the wave for 600 frames | 12.8 ms      |
| The Mix view in Edge draws 1,200 frames with both decks playing          | 1.9 ms       |

## Audio

Both decks played the same two MP3s through the default output device for 180 s, with no page open. The counts come from the audio callback, as in [mix-view.md](mix-view.md).

|                                |                           |
| ------------------------------ | ------------------------- |
| Mode                           | Exclusive                 |
| Period                         | 256 frames, 256 asked for |
| Device buffer                  | 256 frames                |
| Callbacks                      | 33,764                    |
| Frames asked for in a callback | 256 to 256                |
| Underruns                      | 0 (0 frames)              |
| Late callbacks                 | 0                         |
| Starved callbacks              | 0                         |
| Stream errors                  | 0                         |
| Longest callback               | 0.2 ms                    |

## Memory

The desktop app and every process under it: the webview's browser, GPU, renderer and helper processes. The private working set counts each process's own pages and leaves out pages it shares. The working set adds shared pages once for every process that maps them.

| When                                          | Processes | Private working set | Working set |
| --------------------------------------------- | --------- | ------------------- | ----------- |
| Idle with the library open, 10 s after launch | 7         | 139 MB              | 450 MB      |
| After the scroll and wave runs                | 8         | 442 MB              | 843 MB      |

## Wave

Five runs of `measureWave()` in the desktop app, as in [wave.md](wave.md). The page's timer reads in steps of 0.1 ms, so 0.0 means under 0.1 ms.

| Run | Median | 99th percentile | Longest |
| --- | ------ | --------------- | ------- |
| 1   | 0.0 ms | 0.2 ms          | 0.3 ms  |
| 2   | 0.0 ms | 0.1 ms          | 0.2 ms  |
| 3   | 0.0 ms | 0.1 ms          | 0.6 ms  |
| 4   | 0.0 ms | 0.1 ms          | 0.2 ms  |
| 5   | 0.0 ms | 0.1 ms          | 0.2 ms  |
