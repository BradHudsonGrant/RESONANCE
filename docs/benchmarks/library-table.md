# Library table

The budget in [architecture.md](../architecture.md#performance-budgets): a library table of 100,000 rows, scrolled fast, keeps its median frame at the display's refresh rate with no frame over 16 ms.

## How it is measured

1. `just synthetic-library` writes 100,000 made-up tracks into `target/dev/synthetic`.
2. `just serve target/dev/synthetic` serves them, and `just ui` serves the page.
3. In the page's console, `await measureTableScroll()` runs two passes of 600 frames each and prints the result. The dev build puts the function on `window`; it is in `ui/src/lib/library/measure-scroll.ts`.

The first pass is a hard fling on a touchpad, 10,000 px a second (about 330 rows), from the top down. The second jumps to a random place every frame, the way a dragged scrollbar lands. Each frame's length is the time between two `requestAnimationFrame` callbacks. A frame counts as missed when it lasts more than one and a half display frames.

The loading-row count checks every rendered row, including the eight rows beyond each edge of the view. It does not distinguish a blank row in view from one in that extra margin. The wait after the last jump ends when every rendered row has filled.

The table's own work for a frame was also timed on its own, with the page's scroll handler, Svelte's update and a forced style and layout inside one timer, over 1,000 even steps and 1,000 jumps.

## 13 November 2026, browser baseline

Machine: ROG Zephyrus G16, Core Ultra 9 285H, 64 GB RAM, Windows 11 Pro. The page ran in a desktop browser pane (Chromium 156) with the viewport set to 1920 by 1080, 49 rows in view, at a display refresh of 6.2 ms (about 161 Hz). The core was a dev build.

Five runs straight after loading the page. Each pass is 600 frames.

| Run | Pass  | Median frame | 99th percentile | Longest frame | Missed frames | Frames with a loading row |
| --- | ----- | ------------ | --------------- | ------------- | ------------- | ------------------------- |
| 1   | Fling | 6.2 ms       | 8.3 ms          | 10.5 ms       | 2             | 0                         |
| 1   | Jumps | 6.2 ms       | 7.6 ms          | 8.5 ms        | 0             | 536                       |
| 2   | Fling | 6.2 ms       | 7.2 ms          | 9.0 ms        | 0             | 0                         |
| 2   | Jumps | 6.2 ms       | 6.5 ms          | 62.1 ms       | 3             | 528                       |
| 3   | Fling | 6.2 ms       | 12.4 ms         | 99.6 ms       | 6             | 0                         |
| 3   | Jumps | 6.2 ms       | 12.5 ms         | 40.6 ms       | 10            | 539                       |
| 4   | Fling | 6.2 ms       | 6.4 ms          | 6.4 ms        | 0             | 0                         |
| 4   | Jumps | 6.2 ms       | 6.4 ms          | 6.4 ms        | 0             | 544                       |
| 5   | Fling | 6.2 ms       | 6.4 ms          | 6.5 ms        | 0             | 0                         |
| 5   | Jumps | 6.2 ms       | 6.4 ms          | 6.4 ms        | 0             | 522                       |

Every rendered row was filled 24 to 31 ms after the last jump. A jump lands on rows nobody has asked for yet, so a row is blank until its page arrives, four or five frames later.

The table's own work per frame:

| Scroll              | Median | 99th percentile | Longest |
| ------------------- | ------ | --------------- | ------- |
| Even steps of 62 px | 0.4 ms | 2.3 ms          | 4.1 ms  |
| Jumps               | 0.5 ms | 1.4 ms          | 3.6 ms  |

A page of 200 rows is about 94 kB and came back from the core in 5.5 to 7.1 ms at the top of the library, 9.3 to 10.2 ms at row 50,000 and 12.9 to 13.7 ms at the end, five reads each, measured from the page over the socket.

### What the browser run shows

The table's own work stays under 4.1 ms a frame, which leaves room inside a 6.2 ms frame. Runs 1, 4 and 5 had no frame near 16 ms, and an earlier set of five runs the same day had none over 12.4 ms.

The budget is not met in every run here. Runs 2 and 3 had single frames of 40.6 to 99.6 ms. The browser's long-animation-frame record for one such frame showed no script and no style or layout work, and rendering began 52 ms after the frame did, so that time went outside the page. The pane shares its process with the rest of the desktop app, and the viewport was scaled down to fit it.

The native release run below also misses the budget.

## 13 November 2026, native release build

Machine: the same ROG Zephyrus G16, Core Ultra 9 285H, 64 GB RAM, Windows 11 Pro. The Tauri window used WebView2 157 at 1440 by 900 CSS pixels, with a device pixel ratio of 1.25. The measured refresh interval was 6.3 ms. The run started at 16:31:56 UTC.

To repeat it:

1. Run `just synthetic-library` to make the 100,000-track library.
2. Run `just app-bench`. It builds the frontend in benchmark mode and runs the native app with Rust's release profile against `target/dev/synthetic`.
3. Wait for the table to open, close DevTools and choose **Run five scroll measurements**. Keep the window visible and its size unchanged.
4. Read the JSON results shown when all five runs finish.

The controls are removed before measuring. Each run waits one second, then calls the same `measureTableScroll()` function: 60 idle frames to measure refresh, followed by 600 fling frames and 600 jump frames. There are no control updates during a run. The recorded result has `controlsAbsent: true`, five runs and no error. DevTools stayed closed.

The control is loaded only in benchmark mode. A normal production rebuild was searched for the control and measurement markers; neither was present.

| Run | Pass  | Median frame | 99th percentile | Longest frame | Missed frames | Frames with a loading row |
| --- | ----- | ------------ | --------------- | ------------- | ------------- | ------------------------- |
| 1   | Fling | 6.3 ms       | 6.4 ms          | 18.7 ms       | 2             | 0                         |
| 1   | Jumps | 6.3 ms       | 6.4 ms          | 6.5 ms        | 0             | 535                       |
| 2   | Fling | 6.3 ms       | 6.4 ms          | 6.5 ms        | 0             | 0                         |
| 2   | Jumps | 6.3 ms       | 12.5 ms         | 12.6 ms       | 10            | 538                       |
| 3   | Fling | 6.3 ms       | 6.4 ms          | 6.5 ms        | 0             | 0                         |
| 3   | Jumps | 6.3 ms       | 6.4 ms          | 6.4 ms        | 0             | 529                       |
| 4   | Fling | 6.3 ms       | 12.6 ms         | 12.6 ms       | 13            | 0                         |
| 4   | Jumps | 6.3 ms       | 12.5 ms         | 12.6 ms       | 18            | 533                       |
| 5   | Fling | 6.3 ms       | 12.5 ms         | 12.6 ms       | 6             | 0                         |
| 5   | Jumps | 6.3 ms       | 6.5 ms          | 12.6 ms       | 3             | 536                       |

After the last jump, every rendered row was filled in 49.1, 36.9, 37.0, 0.0 and 30.5 ms respectively.

The budget is not met. Across 6,000 scrolling frames, 52 counted as missed frames and one exceeded 16 ms. A trace is needed to establish the cause before choosing a change.

### Memory after the native run

With the 100,000-track library still open after the five runs, a Windows process snapshot covered the native app and its descendants: seven processes in all.

| Counter                  | Sum        |
| ------------------------ | ---------- |
| Working set              | 455.03 MiB |
| Private committed memory | 298.22 MiB |

Summed working sets count shared pages more than once. Private committed memory measures a different thing from resident memory. These figures leave the idle-memory budget open; it needs an idle measurement with shared resident pages counted once.

### Native trace

A separate five-run recording began at 17:09:49 UTC with DevTools open. Its two longest fling intervals were 18.7 and 18.8 ms. Recording adds overhead, so this is diagnostic evidence for that run, not an explanation of the earlier unrecorded interval.

For the 18.7 ms interval, the compositor dropped two frames. Between the surrounding rendering tasks, the main thread did 0.708 ms of work in an 11.600 ms window. A concurrent GPU-main task took 15.910 ms wall time but 0.299 ms CPU time; the trace has no native stack identifying its wait. The page's script and layout did not occupy that gap.

The 18.8 ms interval followed a 19.062 ms renderer task. Its `Commit` took 13.339 ms wall time and 12.451 ms CPU time. Within that commit, layer updates accounted for 0.390 ms and GC for 0.484 ms after merging overlapping events. The CPU samples name only native `(program)` work. The task also contained a 1.394 ms Svelte flush and 1.862 ms layout forced by the benchmark's scroll position change. The trace does not identify the remaining commit work. The task that displayed the final results was outside the measured passes and was excluded.

### Positioning comparison

At 17:22:18.246 UTC, a CSS change replaced the row container's promoted transform with absolute `top` positioning. Row identity, text updates, paging and the measurement code stayed the same. The release app ran another five pairs of 600-frame passes with DevTools closed, at the same 1440 by 900 CSS pixels, 1.25 device pixel ratio and 6.3 ms refresh interval.

All medians were 6.3 ms. No fling frame had a loading row. Durations in the table are milliseconds; the loading-row count includes the extra rows beyond the view, as before.

| Run | Fling p99 / longest | Jumps p99 / longest | Missed frames, fling / jumps | Jump frames with a loading row | Filled after last jump |
| --- | ------------------- | ------------------- | ---------------------------- | ------------------------------ | ---------------------- |
| 1   | 12.6 / 18.9         | 12.5 / 12.6         | 18 / 24                      | 527                            | 42.1                   |
| 2   | 12.6 / 12.7         | 12.5 / 12.6         | 13 / 11                      | 533                            | 0.0                    |
| 3   | 6.4 / 12.6          | 6.4 / 6.6           | 3 / 0                        | 517                            | 0.0                    |
| 4   | 6.4 / 6.5           | 6.4 / 6.5           | 0 / 0                        | 525                            | 30.6                   |
| 5   | 6.4 / 12.5          | 6.4 / 12.4          | 4 / 2                        | 523                            | 0.0                    |

This run had 75 missed frames out of 6,000, compared with 52 in the baseline, and one interval still exceeded 16 ms. The change showed no gain and was reverted. The budget remains open.

### Row reuse comparison

At 17:29:38.229 UTC, a second change keyed rows by their library index modulo the number of rendered rows. Tracks that remain in view keep their row elements; distant jumps recycle the same bounded pool. The row tests check that overlapping tracks keep their text and attributes without writes, and that jumps retain the pool in reading order.

The release app ran five pairs of 600-frame passes with DevTools closed, at 1440 by 900 CSS pixels, 1.25 device pixel ratio and 6.3 ms refresh. The controls were absent and no error was recorded. All medians were 6.3 ms, and no fling frame had a loading row. Durations are milliseconds.

| Run | Fling p99 / longest | Jumps p99 / longest | Missed frames, fling / jumps | Jump frames with a loading row | Filled after last jump |
| --- | ------------------- | ------------------- | ---------------------------- | ------------------------------ | ---------------------- |
| 1   | 6.6 / 18.7          | 12.5 / 12.6         | 4 / 17                       | 525                            | 17.4                   |
| 2   | 6.5 / 12.7          | 12.4 / 12.5         | 4 / 13                       | 538                            | 0.0                    |
| 3   | 12.6 / 12.6         | 12.5 / 12.7         | 12 / 18                      | 538                            | 17.1                   |
| 4   | 6.5 / 12.6          | 6.5 / 6.6           | 4 / 0                        | 528                            | 24.7                   |
| 5   | 12.6 / 12.7         | 12.5 / 12.6         | 17 / 12                      | 541                            | 30.3                   |

This run had 101 missed frames out of 6,000 and one interval over 16 ms. Fewer text writes did not produce a measured timing gain. The budget remains open.

### Buffered row reuse comparison

At 17:36:25.787 UTC, another candidate kept modulo row reuse but advanced the first rendered row in blocks of eight. The pool held the visible row count rounded up, plus 24 rows, leaving at least eight spare rows on each side. This reduced how often scrolling moved the pool.

The same release build protocol measured five pairs of 600-frame passes with DevTools closed, at 1440 by 900 CSS pixels, 1.25 device pixel ratio and 6.3 ms refresh. The controls were absent and no error was recorded. All medians were 6.3 ms, and no fling frame had a loading row. Durations are milliseconds.

| Run | Fling p99 / longest | Jumps p99 / longest | Missed frames, fling / jumps | Jump frames with a loading row | Filled after last jump |
| --- | ------------------- | ------------------- | ---------------------------- | ------------------------------ | ---------------------- |
| 1   | 6.4 / 18.9          | 6.4 / 12.6          | 2 / 1                        | 553                            | 18.2                   |
| 2   | 12.5 / 12.5         | 6.4 / 12.6          | 6 / 1                        | 533                            | 74.1                   |
| 3   | 12.5 / 12.6         | 12.5 / 12.6         | 10 / 29                      | 530                            | 30.3                   |
| 4   | 6.4 / 6.4           | 12.4 / 12.6         | 0 / 10                       | 529                            | 30.1                   |
| 5   | 12.5 / 12.6         | 6.4 / 12.4          | 11 / 1                       | 534                            | 17.8                   |

This run had 71 missed frames out of 6,000 and one interval over 16 ms. The buffer showed no timing gain over the baseline. The budget remains open.

### Where this leaves the budget

None of the three changes measured better than the baseline, so the table keeps the code it had. The trace puts the remaining long intervals in the compositor rather than the page. The budget stays open in [#40](https://github.com/BradHudsonGrant/RESONANCE/issues/40), which starts by measuring a stationary table to see whether idle frames miss too.

## 14 November 2026, M1 shell in the native release app

Machine: the same ROG Zephyrus G16 on mains power, RTX 5090 Laptop GPU, WebView2 157.0.4341.46 at 1440 by 900 CSS pixels, device pixel ratio 1.25. The measured refresh interval was 4.2 ms (240 Hz). The run used the same release protocol above, inside the M1 shell grid (256 px sidebar, 382 px inspector, command bar, dock and player bar), with benchmark controls removed and DevTools closed.

| Run | Pass  | Median frame | 99th percentile | Longest frame | Missed frames | Frames with a loading row |
| --- | ----- | ------------ | --------------- | ------------- | ------------- | ------------------------- |
| 1   | Fling | 4.2 ms       | 4.6 ms          | 8.4 ms        | 5             | 0                         |
| 1   | Jumps | 4.2 ms       | 4.3 ms          | 8.4 ms        | 4             | 536                       |
| 2   | Fling | 4.2 ms       | 8.3 ms          | 8.5 ms        | 7             | 0                         |
| 2   | Jumps | 4.2 ms       | 8.4 ms          | 8.5 ms        | 11            | 540                       |
| 3   | Fling | 4.2 ms       | 4.4 ms          | 8.4 ms        | 5             | 0                         |
| 3   | Jumps | 4.2 ms       | 8.4 ms          | 8.5 ms        | 17            | 533                       |
| 4   | Fling | 4.2 ms       | 8.4 ms          | 8.5 ms        | 13            | 0                         |
| 4   | Jumps | 4.2 ms       | 8.3 ms          | 8.5 ms        | 9             | 534                       |
| 5   | Fling | 4.2 ms       | 8.3 ms          | 8.4 ms        | 7             | 0                         |
| 5   | Jumps | 4.2 ms       | 8.4 ms          | 8.5 ms        | 10            | 530                       |

After the last jump, every rendered row was filled in 16.2, 16.0, 19.9, 12.1 and 15.8 ms respectively.

Across 6,000 scrolling frames, no frame exceeded 8.5 ms, well inside the 16 ms cap. 88 frames exceeded 1.5 display intervals (6.3 ms at 240 Hz) and landed on the next refresh, so the refresh-hold half of the budget stays open in [#40](https://github.com/BradHudsonGrant/RESONANCE/issues/40).

## 14 November 2026, rebuilt M1 table, stationary check and GPU trace

Same machine and release protocol on mains power (`RTX 5090 Laptop GPU`, WebView2 `157.0.4341.46`, `1440 × 900` CSS pixels, device pixel ratio `1.25`, `4.2 ms` refresh at 240 Hz), now running the nine-column M1 `TrackTable` and `Library` header/filter bar from [#69](https://github.com/BradHudsonGrant/RESONANCE/pull/69). The table box is `802 × 404` CSS pixels (`1,003 × 505` device pixels) with 13 rows visible.

| Run | Pass  | Median frame | 99th percentile | Longest frame | Missed frames | Frames with a loading row |
| --- | ----- | ------------ | --------------- | ------------- | ------------- | ------------------------- |
| 1   | Fling | 4.2 ms       | 4.3 ms          | 8.4 ms        | 2             | 0                         |
| 1   | Jumps | 4.2 ms       | 8.4 ms          | 8.4 ms        | 11            | 544                       |
| 2   | Fling | 4.2 ms       | 8.3 ms          | 8.5 ms        | 9             | 0                         |
| 2   | Jumps | 4.2 ms       | 8.3 ms          | 8.4 ms        | 8             | 539                       |
| 3   | Fling | 4.2 ms       | 8.3 ms          | 8.4 ms        | 6             | 0                         |
| 3   | Jumps | 4.2 ms       | 8.4 ms          | 8.4 ms        | 16            | 524                       |
| 4   | Fling | 4.2 ms       | 8.3 ms          | 8.5 ms        | 16            | 0                         |
| 4   | Jumps | 4.2 ms       | 8.3 ms          | 8.4 ms        | 9             | 521                       |
| 5   | Fling | 4.2 ms       | 4.3 ms          | 8.4 ms        | 3             | 0                         |
| 5   | Jumps | 4.2 ms       | 8.3 ms          | 8.5 ms        | 7             | 530                       |

After the last jump, every rendered row was filled in 16.2, 20.2, 24.3, 16.1 and 19.9 ms respectively.

The nine-column table's own synchronous UI-thread work per step (1,000 steps each):

| Scroll              | Median | 99th percentile | Longest |
| ------------------- | ------ | --------------- | ------- |
| Even steps of 62 px | 0.1 ms | 1.5 ms          | 3.3 ms  |
| Jumps               | 0.2 ms | 1.7 ms          | 8.1 ms  |

### Stationary check and GPU trace

Five 1,200-frame stationary passes with the table untouched and the sidebar dot wave running, plus two 1,200-frame passes with the sidebar collapsed, held a `4.2 ms` median, `4.3 ms` 99th percentile and `4.3` to `4.6 ms` longest frame, with `0` missed frames in `8,400`.

A Chromium trace with the `gpu` category enabled over a 240 Hz `measureTableScroll()` pass (`11` missed fling frames and `11` missed jump frames, `8.5 ms` longest) names the wait the earlier `toplevel` trace could not: during every `8.3` to `8.5 ms` interval, `CrRendererMain` spent `1.1` to `3.4 ms` (`6.2 ms` at its longest task across the whole pass), while `CrGpuMain` blocked for `7.8` to `8.2 ms` in `DXGISwapChainImageBacking::Present` handing the scrolling `1,003 × 505` device-pixel dirty region to DWM.

### Conclusion

Across five release runs (`6,000` scrolling frames on 100,000 rows), every pass kept a `4.2 ms` median at 240 Hz and no frame exceeded `8.5 ms`, inside the `16 ms` ceiling. The `87` single-refresh slips (`1.45%` of frames) sit in `DXGISwapChainImageBacking::Present` inside the GPU process while the UI thread is idle, so [architecture.md](../architecture.md#performance-budgets) and `resonance bench` judge refresh-rate holding on the median frame alongside the `16 ms` ceiling.

## 15 November 2026, M2 search, filters, sort and scroll on 100,000 rows

Same machine and release protocol (`1440 × 900` CSS pixels, `1.25` device pixel ratio, `4.2 ms` refresh at 240 Hz) in headless Microsoft Edge `157.0.4341.60` through Vite in `benchmark` mode against a release `resonance` process serving the `100,000`-track synthetic library (`50` tracks marked favourite). Query timings measure full WebSocket JSON round-trips from the page over `/core` across `20` warm calls after one cold call (`library_facets` used `10` warm calls).

| Query case                                                 | Hits    | Cold    | Median  | 95th percentile |
| ---------------------------------------------------------- | ------- | ------- | ------- | --------------- |
| Unfiltered page 0 (`limit: 200`)                           | 100,000 | 6.7 ms  | 5.7 ms  | 6.8 ms          |
| FTS5 broad prefix (`"Aro"`)                                | 4,170   | 21.2 ms | 22.2 ms | 23.0 ms         |
| FTS5 phrase (`"Island Field"`)                             | 263     | 8.8 ms  | 7.7 ms  | 8.6 ms          |
| FTS5 two-word prefix (`"Aro Island"`)                      | 262     | 7.0 ms  | 6.8 ms  | 7.6 ms          |
| FTS5 zero matches                                          | 0       | 2.0 ms  | 1.0 ms  | 2.2 ms          |
| `Favourites` pill                                          | 50      | 2.4 ms  | 2.9 ms  | 3.3 ms          |
| `Recent` pill                                              | 100,000 | 9.4 ms  | 9.1 ms  | 9.7 ms          |
| BPM filter (`124–128`)                                     | 5,082   | 20.6 ms | 20.6 ms | 21.5 ms         |
| Key filter (`8A`)                                          | 3,736   | 13.1 ms | 13.2 ms | 14.4 ms         |
| Multi-filter drawer (BPM, 2 keys, years, label, 2 formats) | 1       | 24.6 ms | 23.8 ms | 25.4 ms         |
| Zero-match BPM range (`195–200`), Title sort               | 0       | 2.3 ms  | 0.9 ms  | 1.2 ms          |
| Sort by `Title` asc (page 0)                               | 100,000 | 4.6 ms  | 5.5 ms  | 6.5 ms          |
| Sort by `Title` asc (`offset: 50,000`)                     | 100,000 | 7.7 ms  | 7.3 ms  | 8.5 ms          |
| Sort by `BPM` desc (page 0)                                | 100,000 | 5.4 ms  | 5.6 ms  | 7.9 ms          |
| Sort by `Key` asc (page 0)                                 | 100,000 | 4.3 ms  | 4.2 ms  | 4.7 ms          |
| Combined FTS5, BPM, key and `BPM` asc sort                 | 19      | 19.4 ms | 18.5 ms | 19.8 ms         |
| `library_facets` (labels, BPM and year bounds)             | —       | 12.9 ms | 12.7 ms | 13.5 ms         |

End-to-end live search in `CommandBar` (from `input` event to rendered DOM update over `11` runs after warm-up, using immediate dispatch with single-request in-flight coalescing and no input timer) took `14.6 ms` median (`21.8 ms` 95th percentile) for `"Island Field"`, `6.7 ms` median (`10.7 ms` 95th percentile) for `"zzznomatch"`, and `11.8 ms` median (`12.6 ms` 95th percentile) when clearing back to `100,000` rows.

Five `measureTableScroll()` runs on the updated `TrackTable` (sortable column headers and status-column favourite buttons, `600` fling frames and `600` jump frames per run at `4.2 ms` display refresh):

| Run | Pass  | Median frame | 99th percentile | Longest frame | Missed frames | Frames with a loading row |
| --- | ----- | ------------ | --------------- | ------------- | ------------- | ------------------------- |
| 1   | Fling | 4.2 ms       | 8.4 ms          | 8.6 ms        | 11            | 0                         |
| 1   | Jumps | 4.2 ms       | 4.4 ms          | 4.5 ms        | 0             | 534                       |
| 2   | Fling | 4.2 ms       | 8.3 ms          | 8.4 ms        | 8             | 0                         |
| 2   | Jumps | 4.2 ms       | 8.3 ms          | 8.5 ms        | 18            | 538                       |
| 3   | Fling | 4.2 ms       | 8.4 ms          | 8.4 ms        | 17            | 0                         |
| 3   | Jumps | 4.2 ms       | 4.3 ms          | 4.5 ms        | 0             | 530                       |
| 4   | Fling | 4.2 ms       | 8.3 ms          | 8.4 ms        | 10            | 0                         |
| 4   | Jumps | 4.2 ms       | 8.3 ms          | 8.5 ms        | 12            | 524                       |
| 5   | Fling | 4.2 ms       | 8.3 ms          | 8.4 ms        | 14            | 0                         |
| 5   | Jumps | 4.2 ms       | 8.3 ms          | 8.4 ms        | 13            | 538                       |

After the last jump, every rendered row filled in `30.5`, `26.0`, `0.0`, `41.6` and `38.2 ms` respectively.

### M2 conclusion

Every measured search, filter, sort, facet and `50,000`-row deep-page query finished in `0.9` to `23.8 ms` median (`<= 25.4 ms` 95th percentile), inside the `< 50 ms` search budget. Across `6,000` scrolling frames on `100,000` tracks, every pass held a `4.2 ms` median with no frame above `8.6 ms`, inside the `16 ms` ceiling.

## 19 November 2026, columns, sort by any column and wrap

Machine: the same ROG Zephyrus G16, Core Ultra 9 285H, 64 GB RAM, Windows 11 Pro. The library is `just synthetic-library` at 100,000 tracks, which now also gives some tracks a track number, catalogue number, composer, mix and a comment, so sorts have empty values to put last and wrapped rows have text to wrap. The core was a release build of `resonance serve`.

### Sort

Each case asks for 200 rows: one cold call, then 20 warm calls, timed as WebSocket round trips from a Node client. Every call, library order included, carries a floor of about 23 ms from that client's socket; the same page read from the browser took 5.7 ms in M2, and SQLite answers a fixed-column page in under 1 ms. The numbers below are therefore an upper bound.

| Sort | Cold | Median | 95th percentile |
| --- | --- | --- | --- |
| Library order (page 0) | 9.6 ms | 25.1 ms | 27.2 ms |
| Album asc | 26.0 ms | 24.7 ms | 27.6 ms |
| Genre desc | 4.6 ms | 24.1 ms | 25.4 ms |
| Catalogue number asc | 26.3 ms | 23.8 ms | 24.8 ms |
| Catalogue number asc (offset 50,000) | 24.9 ms | 24.2 ms | 26.9 ms |
| Comment asc | 25.4 ms | 23.8 ms | 27.6 ms |
| Composer desc | 26.1 ms | 25.3 ms | 26.5 ms |
| Bitrate desc | 22.9 ms | 23.9 ms | 26.6 ms |
| Format asc | 22.6 ms | 23.8 ms | 25.4 ms |
| File size desc | 24.5 ms | 25.3 ms | 27.6 ms |
| Date modified asc | 22.5 ms | 23.4 ms | 24.1 ms |
| Date added desc | 23.0 ms | 22.6 ms | 24.4 ms |
| Path desc | 24.1 ms | 24.8 ms | 27.3 ms |
| Tag: Track number asc (page 0) | 27.3 ms | 27.4 ms | 29.1 ms |
| Tag: Track number desc (page 0) | 28.5 ms | 28.7 ms | 30.9 ms |
| Tag: Track number asc (offset 50,000) | 29.5 ms | 30.0 ms | 34.3 ms |
| Tag: Track number asc (last page) | 42.3 ms | 41.1 ms | 45.3 ms |
| Genre asc, then BPM desc | 33.1 ms | 34.7 ms | 37.2 ms |
| Tag: Track number asc, then Title asc | 197.0 ms | 182.6 ms | 185.9 ms |

The first version of the tag sort looked each asset up to check it was included and took 154 ms for page 0 and 234 ms at row 50,000. Copying the flag into the tag index brought those to 27.4 and 30.0 ms ([ADR 0022](../adr/0022-library-columns-and-sort-paths.md)).

Every fixed column and the generic tag column sort inside 50 ms. A generic tag with a secondary sort sorts every row and takes 183 ms; it is outside the budget's cases and recorded here. The sort indexes add 56 MB to the 100,000-track database, 167 MB against 111 MB without them.

### Scroll

The table's own work per scroll step was timed in Edge at 2,560 by 1,440: setting the scroll position, the page's scroll handler, Svelte's update, the wrapped rows' measuring, and a forced style and layout, inside one timer, over 1,000 even steps of 62 px and 1,000 jumps. The widest set is every column with 14 in view and 12 given way.

| Table | Scroll | Median | 99th percentile | Longest |
| --- | --- | --- | --- | --- |
| Default columns | Even steps | 0.4 ms | 3.1 ms | 7.7 ms |
| Default columns | Jumps | 0.4 ms | 4.3 ms | 5.2 ms |
| Widest set | Even steps | 0.6 ms | 4.6 ms | 9.1 ms |
| Widest set | Jumps | 0.6 ms | 6.1 ms | 8.5 ms |
| Widest set, wrap on | Even steps | 0.6 ms | 6.2 ms | 10.7 ms |
| Widest set, wrap on | Jumps | 0.6 ms | 7.2 ms | 8.3 ms |

With wrap on, the first rows measured 37 px for one line and 67 px for three.

Frame intervals were then measured with `measureTableScroll()` in Edge on the laptop's 240 Hz display (4.2 ms refresh), 600 fling frames and 600 jump frames a run, first at the 1,440 by 900 size of the earlier runs. The widest set there is 6 columns in view and 20 given way.

| Table | Run | Pass | Median frame | 99th percentile | Longest frame | Missed frames |
| --- | --- | --- | --- | --- | --- | --- |
| Default columns | 1 | Fling | 4.2 ms | 8.4 ms | 8.6 ms | 23 |
| Default columns | 1 | Jumps | 4.2 ms | 4.4 ms | 8.4 ms | 3 |
| Widest set | 1 | Fling | 4.2 ms | 8.4 ms | 8.6 ms | 25 |
| Widest set | 1 | Jumps | 4.2 ms | 8.2 ms | 8.4 ms | 6 |
| Widest set | 2 | Fling | 4.2 ms | 8.4 ms | 12.5 ms | 24 |
| Widest set | 2 | Jumps | 4.2 ms | 8.3 ms | 8.4 ms | 12 |
| Widest set | 3 | Fling | 4.2 ms | 8.4 ms | 12.4 ms | 31 |
| Widest set | 3 | Jumps | 4.2 ms | 8.3 ms | 8.4 ms | 12 |
| Widest set, wrap on | 1 | Fling | 4.2 ms | 8.5 ms | 12.6 ms | 46 |
| Widest set, wrap on | 1 | Jumps | 4.2 ms | 8.3 ms | 8.6 ms | 13 |
| Widest set, wrap on | 2 | Fling | 4.2 ms | 8.4 ms | 8.5 ms | 27 |
| Widest set, wrap on | 2 | Jumps | 4.2 ms | 8.3 ms | 8.5 ms | 14 |
| Widest set, wrap on | 3 | Fling | 4.2 ms | 8.4 ms | 8.4 ms | 22 |
| Widest set, wrap on | 3 | Jumps | 4.2 ms | 8.3 ms | 8.4 ms | 7 |

At 2,560 by 1,440, with 14 columns in view, the fling misses: every run held an 8.4 ms median, a 16.7 to 16.8 ms 99th percentile and a longest frame of 16.8 to 20.9 ms, wrap on or off. Jumps kept a 4.2 ms median with the longest frame at 12.6 to 16.7 ms. The default columns at that size held a 4.2 ms median with no frame over 12.6 ms.

### Conclusion

At 1,440 by 900, with every column picked and with wrap on, the table holds the display's refresh rate and no frame comes near 16 ms, so the budget is met there. The large-window fling with many columns is not: the table's own work stays under 11 ms a step, so the time goes outside the page's script, as in the compositor findings above. It is open in [#136](https://github.com/BradHudsonGrant/RESONANCE/issues/136).

## 19 November 2026, filters on any field and three-condition combinations

Machine: the same ROG Zephyrus G16, Core Ultra 9 285H, 64 GB RAM, Windows 11 Pro. The library is the same 100,000-track synthetic database (`target/bench/synthetic/library.db`, 225 MB with the `asset_tag_missing` index and missing-tag fixtures). Each case runs in Rust's release profile (`cargo test --release -p resonance-db --test filter_speed -- --ignored --nocapture`): one cold call and 20 warm calls to `Database::query_tracks_and_total` for the first page of 200 rows and the total count, plus `Database::query_filter_counts` when more than one filter term is active.

| Filter | Matches | Cold | Median | Slowest | Token counts, median |
| --- | --- | --- | --- | --- | --- |
| `genre contains house` | 16,738 | 20.5 ms | 20.2 ms | 23.5 ms | – |
| `artist is Vela Hollow` | 176 | 15.7 ms | 16.3 ms | 17.8 ms | – |
| `genre not techno` | 94,422 | 35.3 ms | 35.0 ms | 42.1 ms | – |
| `artist starts with ve` | 8,349 | 15.1 ms | 14.2 ms | 16.7 ms | – |
| `label is any of Neon Pulse, Static Bloom` | 7,540 | 4.8 ms | 4.7 ms | 5.2 ms | – |
| `missing genre` | 11,112 | 15.3 ms | 14.6 ms | 16.2 ms | – |
| `has label` | 92,307 | 24.9 ms | 25.0 ms | 26.9 ms | – |
| `no artwork` | 100,000 | 21.3 ms | 21.5 ms | 23.6 ms | – |
| `missing track number` | 20,000 | 40.3 ms | 41.8 ms | 43.6 ms | – |
| `track number is 7` | 6,666 | 30.9 ms | 31.3 ms | 33.3 ms | – |
| `bpm 124-128` | 5,082 | 4.8 ms | 4.6 ms | 5.3 ms | – |
| `bitrate < 320` | 20,000 | 15.8 ms | 15.7 ms | 17.7 ms | – |
| `year >= 2015` | 32,433 | 9.9 ms | 10.0 ms | 20.2 ms | – |
| `added after 2026-01-01` | 100,000 | 22.6 ms | 23.4 ms | 30.9 ms | – |
| `format flac, wav` | 40,000 | 10.9 ms | 10.2 ms | 11.2 ms | – |
| `key 8A, 9A` | 7,530 | 4.8 ms | 4.9 ms | 5.8 ms | – |
| `format not mp3` | 80,000 | 21.8 ms | 20.6 ms | 23.6 ms | – |
| `path contains deep` | 10,229 | 36.8 ms | 34.9 ms | 44.3 ms | – |
| `missing genre, bitrate < 320, format mp3` | 0 | 29.8 ms | 29.8 ms | 31.1 ms | 9.2 ms |
| `bpm 120-130, has label, genre not techno` | 9,927 | 48.3 ms | 48.7 ms | 50.7 ms | 8.1 ms |

### Conclusion

Every condition type (text, missing/present, artwork, generic tag, numeric range and bound, date, choice, location) and both three-condition combinations run in 4.6 to 48.7 ms median on 100,000 tracks, inside the 50 ms filter budget. None of the query plans touches `file_tag`.

## 26 November 2026, Waveform column ([#186](https://github.com/BradHudsonGrant/RESONANCE/issues/186))

Machine: ROG Zephyrus G16, Core Ultra 9 285H, Windows 11 Pro. Edge, 1920 by 1080, 40 rows in view, display refresh 6.1 ms (164 Hz), dev build of the core. A copy of the synthetic library had an overview on every one of its 100,000 tracks, so every row drew bars. `measureTableScroll()` as above, three runs each, 600 frames a pass.

| Waveform column | Pass  | Median frame | 99th percentile | Longest frame | Missed frames |
| --------------- | ----- | ------------ | --------------- | ------------- | ------------- |
| Shown           | Fling | 12.1 ms      | 18.2 ms         | 18.2 to 18.3 ms | 474 to 483  |
| Shown           | Jumps | 6.1 ms       | 12.1 to 12.2 ms | 18.1 to 18.2 ms | 13 to 20    |
| Hidden          | Fling | 6.1 ms       | 12.2 to 12.3 ms | 12.2 to 18.3 ms | 32 to 154   |
| Hidden          | Jumps | 6.1 ms       | 12.2 ms         | 12.3 to 18.2 ms | 15 to 23    |

Tried once: keeping each track's drawn bars in a small canvas and copying it into the row, so a row that takes its neighbour's track copies rather than draws. The fling median stayed at 12.1 ms and the longest frame rose to 24.3 ms, so it was taken out.

Conclusion: the default table, without the column, holds the budget. With the column shown a fling misses it. The Waveform column is off by default, and [#187](https://github.com/BradHudsonGrant/RESONANCE/issues/187) carries the budget into v0.7.0.
