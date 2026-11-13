# Library table

The budget in [architecture.md](../architecture.md#performance-budgets): a library table of 100,000 rows, scrolled fast, holds the display's refresh rate with no frame over 16 ms.

## How it is measured

1. `just synthetic-library` writes 100,000 made-up tracks into `target/dev/synthetic`.
2. `just serve target/dev/synthetic` serves them, and `just ui` serves the page.
3. In the page's console, `await measureTableScroll()` runs two passes of 600 frames each and prints the result. The dev build puts the function on `window`; it is in `ui/src/lib/library/measure-scroll.ts`.

The first pass is a hard fling on a touchpad, 10,000 px a second (about 330 rows), from the top down. The second jumps to a random place every frame, the way a dragged scrollbar lands. Each frame's length is the time between two `requestAnimationFrame` callbacks. A frame counts as missed when it lasts more than one and a half display frames.

The table's own work for a frame was also timed on its own, with the page's scroll handler, Svelte's update and a forced style and layout inside one timer, over 1,000 even steps and 1,000 jumps.

## 13 November 2026, M0

Machine: ROG Zephyrus G16, Core Ultra 9 285H, 64 GB RAM, Windows 11 Pro. The page ran in the browser pane of the Claude desktop app (Chromium 156) with the viewport set to 1920 by 1080, 49 rows in view, at a display refresh of 6.2 ms (about 161 Hz). The core was a dev build.

Five runs straight after loading the page. Each pass is 600 frames.

| Run | Pass | Median frame | 99th percentile | Longest frame | Missed frames | Frames with a blank row |
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

Every row in view was filled 24 to 31 ms after the last jump. A jump lands on rows nobody has asked for yet, so a row is blank until its page arrives, four or five frames later.

The table's own work per frame:

| Scroll | Median | 99th percentile | Longest |
| --- | --- | --- | --- |
| Even steps of 62 px | 0.4 ms | 2.3 ms | 4.1 ms |
| Jumps | 0.5 ms | 1.4 ms | 3.6 ms |

A page of 200 rows is about 94 kB and came back from the core in 5.5 to 7.1 ms at the top of the library, 9.3 to 10.2 ms at row 50,000 and 12.9 to 13.7 ms at the end, five reads each, measured from the page over the socket.

## What it shows

The table's own work stays under 4.1 ms a frame, which leaves room inside a 6.2 ms frame. Runs 1, 4 and 5 held the refresh rate with no frame near 16 ms, and an earlier set of five runs the same day had none over 12.4 ms.

The budget is not met in every run here. Runs 2 and 3 had single frames of 40.6 to 99.6 ms. The browser's long-animation-frame record for one such frame showed no script and no style or layout work, and rendering began 52 ms after the frame did, so that time went outside the page. The pane shares its process with the rest of the desktop app, and the viewport was scaled down to fit it.

Measure again in the packaged app, where nothing else shares the window, when the Tauri shell exists ([#20](https://github.com/BradHudsonGrant/RESONANCE/issues/20)). That run decides whether the budget is met.
