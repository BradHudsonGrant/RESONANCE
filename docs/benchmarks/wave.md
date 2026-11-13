# Wave component

The budget in [architecture.md](../architecture.md#performance-budgets): under 1 ms of UI-thread time per frame at 60 fps.

## How it is measured

In the dev page's console, `await measureWave()` draws the wave on a 960 by 240 CSS-pixel canvas for 600 frames. It uses the Lively preset with music input and a beat every 30 frames, so every reactive path runs. Each frame's cost is the time to move the surface and issue the draw, which is all the work the wave does on the UI thread. The code is in `ui/src/lib/wave/measure-wave.ts`.

## 13 November 2026

Machine: ROG Zephyrus G16, Core Ultra 9 285H, 64 GB RAM, Windows 11 Pro. Chromium 156 in a desktop browser pane, device pixel ratio 1.25, medium density (27,040 points).

| Run | Median | 99th percentile | Longest |
| --- | --- | --- | --- |
| 1 | 0.0 ms | 0.3 ms | 0.3 ms |
| 2 | 0.0 ms | 0.3 ms | 0.4 ms |
| 3 | 0.0 ms | 0.3 ms | 0.4 ms |
| 4 | 0.0 ms | 0.3 ms | 0.3 ms |
| 5 | 0.0 ms | 0.2 ms | 0.3 ms |

The page's timer reads in steps of 0.1 ms, so a median of 0.0 means under 0.1 ms. The longest frame in 3,000 was 0.4 ms, inside the budget.

The GPU's own time is not in these numbers. The budget is about the UI thread, which the wave leaves free.
