# Library table

The budget in [architecture.md](../architecture.md#performance-budgets): a library table of 100,000 rows, scrolled fast, holds the display's refresh rate with no frame over 16 ms.

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

| Run | Pass | Median frame | 99th percentile | Longest frame | Missed frames | Frames with a loading row |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | Fling | 6.2 ms | 8.3 ms | 10.5 ms | 2 | 0 |
| 1 | Jumps | 6.2 ms | 7.6 ms | 8.5 ms | 0 | 536 |
| 2 | Fling | 6.2 ms | 7.2 ms | 9.0 ms | 0 | 0 |
| 2 | Jumps | 6.2 ms | 6.5 ms | 62.1 ms | 3 | 528 |
| 3 | Fling | 6.2 ms | 12.4 ms | 99.6 ms | 6 | 0 |
| 3 | Jumps | 6.2 ms | 12.5 ms | 40.6 ms | 10 | 539 |
| 4 | Fling | 6.2 ms | 6.4 ms | 6.4 ms | 0 | 0 |
| 4 | Jumps | 6.2 ms | 6.4 ms | 6.4 ms | 0 | 544 |
| 5 | Fling | 6.2 ms | 6.4 ms | 6.5 ms | 0 | 0 |
| 5 | Jumps | 6.2 ms | 6.4 ms | 6.4 ms | 0 | 522 |

Every rendered row was filled 24 to 31 ms after the last jump. A jump lands on rows nobody has asked for yet, so a row is blank until its page arrives, four or five frames later.

The table's own work per frame:

| Scroll | Median | 99th percentile | Longest |
| --- | --- | --- | --- |
| Even steps of 62 px | 0.4 ms | 2.3 ms | 4.1 ms |
| Jumps | 0.5 ms | 1.4 ms | 3.6 ms |

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

| Run | Pass | Median frame | 99th percentile | Longest frame | Missed frames | Frames with a loading row |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | Fling | 6.3 ms | 6.4 ms | 18.7 ms | 2 | 0 |
| 1 | Jumps | 6.3 ms | 6.4 ms | 6.5 ms | 0 | 535 |
| 2 | Fling | 6.3 ms | 6.4 ms | 6.5 ms | 0 | 0 |
| 2 | Jumps | 6.3 ms | 12.5 ms | 12.6 ms | 10 | 538 |
| 3 | Fling | 6.3 ms | 6.4 ms | 6.5 ms | 0 | 0 |
| 3 | Jumps | 6.3 ms | 6.4 ms | 6.4 ms | 0 | 529 |
| 4 | Fling | 6.3 ms | 12.6 ms | 12.6 ms | 13 | 0 |
| 4 | Jumps | 6.3 ms | 12.5 ms | 12.6 ms | 18 | 533 |
| 5 | Fling | 6.3 ms | 12.5 ms | 12.6 ms | 6 | 0 |
| 5 | Jumps | 6.3 ms | 6.5 ms | 12.6 ms | 3 | 536 |

After the last jump, every rendered row was filled in 49.1, 36.9, 37.0, 0.0 and 30.5 ms respectively.

The budget is not met. Across 6,000 scrolling frames, 52 counted as missed frames and one exceeded 16 ms. A trace is needed to establish the cause before choosing a change.

### Memory after the native run

With the 100,000-track library still open after the five runs, a Windows process snapshot covered the native app and its descendants: seven processes in all.

| Counter | Sum |
| --- | --- |
| Working set | 455.03 MiB |
| Private committed memory | 298.22 MiB |

Summed working sets count shared pages more than once. Private committed memory measures a different thing from resident memory. These figures leave the idle-memory budget open; it needs an idle measurement with shared resident pages counted once.
