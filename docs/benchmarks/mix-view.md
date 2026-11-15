# Mix view

The budgets in [architecture.md](../architecture.md#performance-budgets): the Mix view holds the display's refresh rate and, at 120 Hz, its 99th-percentile frame is under 8 ms. Audio is stable at a 256-sample buffer.

## How it is measured

1. `just serve` and `just ui`, then open Mix, put a track on each deck and play both.
2. In the page's console, `await measureMix(frames)` records that many frames and returns the result. The dev build puts it on `window`; it is in `ui/src/lib/mix/measure-mix.ts`.

A frame's length is the time between two `requestAnimationFrame` callbacks, as for the library table, and a frame counts as missed when it lasts more than one and a half display frames. The view's cost is its own UI-thread time per frame: moving the playheads and issuing the draw. The audio figures are the engine's counters over the run: callbacks, underruns (a deck's ring buffer had less than the callback needed), late callbacks (the device asked for more than one and a half periods) and starved callbacks (the device's buffer had run empty). The playhead error is how far each reading from the core was from where the page's playhead had got to.

Short runs are 1,200 frames, about 8 seconds. Stability runs are 28,000 frames, about three minutes.

## 13 November 2026

Machine: ROG Zephyrus G16, Core Ultra 9 285H, 64 GB RAM, Windows 11 Pro. Audio out to ADAM Audio D3V speakers over USB, WASAPI shared mode at 48 kHz. The core was a dev build. Device pixel ratio 1.25. The decks played two MP3s, except in the untouched pane stability run, which played a WAV and an MP3.

The device ran its callback every 480 frames (10 ms) inside a 1,056-frame buffer, although the stream asked for 256. Every callback in every run was asked for exactly 480 frames.

### Browser pane, Chromium 156

| Run                                    | Canvas                        | Refresh | 99th percentile | Longest       | Missed | View, longest | Callbacks       | Underruns | Longest callback | Playhead error, 99th / longest |
| -------------------------------------- | ----------------------------- | ------- | --------------- | ------------- | ------ | ------------- | --------------- | --------- | ---------------- | ------------------------------ |
| 5 short runs                           | 1178 × 589                    | 6.4 ms  | 6.6 ms          | 6.7 to 6.8 ms | 0      | 0.3 to 0.4 ms | 768 to 770 each | 0         | 0.19 ms          | 0.35 / 0.61 ms                 |
| 5 short runs                           | 2335 × 1128                   | 6.4 ms  | 6.6 ms          | 6.6 to 6.8 ms | 0      | 0.3 to 0.4 ms | 768 to 770 each | 0         | 0.17 ms          | 0.29 / 0.50 ms                 |
| Stability, pane resized during the run | 2335 × 1128, then 2709 × 1309 | 6.4 ms  | 6.6 ms          | 91.9 ms       | 80     | 0.6 ms        | 18,093          | 0         | 0.25 ms          | 0.36 / 23.6 ms                 |
| Stability, untouched                   | 2709 × 1309                   | 6.4 ms  | 10.4 ms         | 21.5 ms       | 396    | 0.4 ms        | 18,762          | 0         | 0.38 ms          | 0.29 / 1.37 ms                 |

No run had a late callback, a starved callback or a stream error.

### Edge 157, headed, maximised

Driven by Playwright in a window of 4088 by 1643 CSS pixels, so the canvas was 5045 by 1831 device pixels.

| Run           | Refresh | 99th percentile | Longest | Missed | View, longest | Callbacks | Underruns | Longest callback | Playhead error, 99th / longest |
| ------------- | ------- | --------------- | ------- | ------ | ------------- | --------- | --------- | ---------------- | ------------------------------ |
| Short         | 6.4 ms  | 12.9 ms         | 19.3 ms | 25     | 0.2 ms        | 789       | 0         | 0.14 ms          | 0.35 / 0.47 ms                 |
| Short, traced | 6.4 ms  | 12.9 ms         | 13.0 ms | 33     | 0.2 ms        | 793       | 0         | 0.21 ms          | 0.37 / 0.52 ms                 |
| Stability     | 6.4 ms  | 12.9 ms         | 19.4 ms | 463    | 0.3 ms        | 18,327    | 0         | 0.22 ms          | 0.32 / 3.81 ms                 |

### Trace

The traced short run in Edge had 33 intervals over 8 ms, each about 13 ms: two display frames. In every one, the page's main thread and compositor were busy for under 1.5 ms. The GPU process's main thread spent about 12.8 ms in `DXGISwapChainImageBacking::Present`, waiting to hand the frame to the swap chain. The time goes in presenting the frame, not in the view.

One change followed from it: a WebGL2 context created with `desynchronized: true`, which lets Chromium present the canvas without the full-window compositor pass. It was switched on from the test script, so the view's code was otherwise the same, and measured once in the same Edge window. The short run missed 39 frames and the stability run 830, against 25 and 463 without it, with audio clean in both. It is not used.

## 14 November 2026 — Rebuilt Mix view (#52)

Measured on the same machine on mains power (`RTX 5090 Laptop GPU`, Edge `157.0.4341.60`, release `resonance.exe`, Realtek Audio at 48 kHz, 480-frame period inside a 1,056-frame buffer, device pixel ratio 1.25). The rebuilt view sets the two scrolling lanes to 110 CSS pixels each plus a 1 px divider (`1121 × 221` CSS pixels, `1401 × 276` device pixels in a `1418 × 810` viewport) and pauses the sidebar dot wave while a deck plays. On mains power the panel runs at 240 Hz (`4.2 ms` refresh).

| Run                              | Canvas     | Refresh | 99th percentile | Longest       | Missed (> 6.3 ms) | View, longest | Longest UI task | Callbacks       | Underruns | Longest callback | Playhead error, 99th / longest |
| -------------------------------- | ---------- | ------- | --------------- | ------------- | ----------------- | ------------- | --------------- | --------------- | --------- | ---------------- | ------------------------------ |
| 5 short runs (1,200 frames each) | 1401 × 276 | 4.2 ms  | 4.3 to 8.4 ms   | 8.4 to 8.5 ms | 11 to 33          | 0.2 to 0.4 ms | —               | 502 to 512 each | 0         | 0.12 ms          | 0.18–0.36 / 0.21–4.22 ms       |
| Short, traced (1,200 frames)     | 1401 × 276 | 4.2 ms  | 4.3 ms          | 4.4 ms        | 0                 | 0.2 ms        | 2.03 ms         | 499             | 0         | 0.01 ms          | 0.17 / 0.20 ms                 |

No run had an underrun, a late callback, a starved callback or a stream error. Every frame above one 240 Hz period was a single doubled interval (`8.4` to `8.5 ms`), and no frame in 7,200 exceeded `8.5 ms`. Maximised full-window stability runs stay with [#46](https://github.com/BradHudsonGrant/RESONANCE/issues/46).

## Conclusion

The view's own work is not the limit. It takes at most 0.6 ms of UI-thread time a frame at any size (`0.4 ms` in the rebuilt view, with a `2.03 ms` longest main-thread task), and each reading from the core lands within 0.4 ms of where the playhead has got to at the 99th percentile.

At `1401 × 276` device pixels on mains power (240 Hz), no frame in six 1,200-frame runs exceeded `8.5 ms`, and the traced run held 240 Hz with 0 missed frames (`4.3 ms` 99th percentile, `4.4 ms` longest). In the earlier spike, the frame budget held up to a 2335 by 1128 canvas at 156 Hz (`6.4 ms` refresh), while maximised windows (`2709 × 1309` and `5045 × 1831`) missed 1.4% to 2.1% of frames in swap-chain presentation. Full-window stability verification with the 221 CSS pixel lane card stays open in [#46](https://github.com/BradHudsonGrant/RESONANCE/issues/46).

Audio was stable at the device's 480-frame period: over about 14 minutes of two decks playing, no underruns, late callbacks, starved callbacks or stream errors, and no callback over 0.38 ms of its 10 ms. A 256-frame buffer cannot be asked for through cpal's WASAPI host, so that budget stays open in [#47](https://github.com/BradHudsonGrant/RESONANCE/issues/47). The reasons and the two routes to it are in [ADR 0010](../adr/0010-audio-engine-spike.md).
