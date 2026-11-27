# Mix view

The budgets in [architecture.md](../architecture.md#performance-budgets): the Mix view keeps its median frame at the display's refresh rate, its 99th-percentile frame within two display intervals and no frame over 16 ms. Audio is stable at a 256-sample buffer.

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

## 15 November 2026 — Full-window stability, still-canvas and WebView2 checks (#46)

Measured on the laptop's own `2560 × 1600` 240 Hz display alone on mains power (`RTX 5090 Laptop GPU` in both Edge `157.0.4341.60` and WebView2 `157.0.4341.46`, device pixel ratio `1.25`, release `resonance.exe`, WASAPI shared mode at 48 kHz with a 480-frame period inside a 1,056-frame buffer). Maximised Edge had a `2040 × 1196` CSS pixel viewport and a `1742 × 221` CSS pixel (`2178 × 276` device pixel) Mix lane canvas.

| Run                                       | Canvas     | Refresh | Median | 99th percentile | Longest       | Missed (> 6.3 ms) | View, longest | Longest UI task | Callbacks       | Underruns | Longest callback | Playhead error, 99th / longest |
| ----------------------------------------- | ---------- | ------- | ------ | --------------- | ------------- | ----------------- | ------------- | --------------- | --------------- | --------- | ---------------- | ------------------------------ |
| 5 short runs (1,200 frames each)          | 2178 × 276 | 4.2 ms  | 4.2 ms | 4.3 to 8.4 ms   | 8.4 to 12.8 ms | 5 to 36           | 0.2 ms        | —               | 502 to 513 each | 0         | 0.10 ms          | 0.17–0.76 / 0.20–10.09 ms      |
| Short, traced (1,200 frames)              | 2178 × 276 | 4.2 ms  | 4.2 ms | 8.4 ms          | 8.5 ms        | 33                | 0.2 ms        | 1.30 ms         | 513             | 0         | 0.01 ms          | 0.21 / 0.25 ms                 |
| Stability, 3 minutes (42,857 frames)      | 2178 × 276 | 4.2 ms  | 4.2 ms | 8.4 ms          | 8.8 ms        | 1,187             | 0.3 ms        | —               | 18,351          | 0         | 0.11 ms          | 0.28 / 10.18 ms                |

No run had an underrun, a late callback, a starved callback or a stream error. In the traced run, `CrRendererMain` stayed at or under `1.30 ms` while `CrGpuMain` waited up to `8.29 ms` in `DXGISwapChainImageBacking::Present`. Across all `42,857` frames of the three-minute stability run, no frame exceeded `8.8 ms`.

### Audio at a 256-frame period, 18 November 2026

Same laptop on mains power, release `resonance.exe`, ADAM Audio D3V speakers over USB at 48 kHz. `resonance audio` played two decks and printed the callback's counters every 10 seconds. The device allows 480 frames and nothing shorter in shared mode, and 256 frames in exclusive mode with 16-bit samples ([ADR 0015](../adr/0015-wasapi-exclusive-output.md)). Silent runs played two WAVs of digital silence, which loads the device and the callback exactly as music does without sound.

| Run                                                          | Mode      | Period     | Callbacks | Underruns | Late | Starved | Longest callback |
| ------------------------------------------------------------ | --------- | ---------- | --------- | --------- | ---- | ------- | ---------------- |
| 1 hour (3,600 s), silent, normal light machine use           | Exclusive | 256 frames | 675,010   | 0         | 1    | 0       | 0.34 ms          |
| 20 minutes, silent; `npm ci`, a UI build and clippy at 6 to 8 min | Exclusive | 256 frames | 225,012   | 0         | 11   | 0       | 0.55 ms          |
| 30 s, silent, all 16 threads busy                             | Exclusive | 256 frames | 5,639     | 0         | 0    | 0       | 0.28 ms          |
| 15 opens of 6 s each, silent, each after a shared run         | Exclusive | 256 frames | 1,129 each | 0        | 0    | 0       | 0.16 ms          |
| 8 s each, two MP3s, listened to                               | Shared, then exclusive | 480, 480, 256 and 240 frames | 803 to 1,605 | 0 | 0 | 0 | 0.24 ms |

The late callbacks in the long runs were single ones (one in the 60-minute run on 22 November 2026 at 48 kHz over `CABLE Input (VB-Audio Virtual Cable)`, and eleven in the 20-minute build-load run), none long enough to empty the device, with 0 stream errors. The four clips all sounded clean.

Two exclusive opens earlier on 18 November went wrong from their first callback. The first, a 20-second run straight after a shared one, got 1,940 callbacks where 3,750 were due, and every one of them was late or starved: the device was asking every other period. An hour run that started later crackled throughout and was stopped before it reported. Neither could be made to happen again, under load or across 15 reopens. The engine now plays half a second of silence on each exclusive open and checks the timing. If more than one callback in ten is late, it closes the device, tries once more, and then opens shared.

With that settling check in place, the 1-hour (`3,600 s`) exclusive run completed `675,010` callbacks of `256` frames inside a `256`-frame buffer with `0` underruns (`0` missing frames), `1` late callback, `0` starved callbacks, `0` stream errors and a `0.34 ms` longest callback, meeting the budget in [#47](https://github.com/BradHudsonGrant/RESONANCE/issues/47).

### Still-canvas and WebView2 comparison

With playback stopped and the page replaced by a single WebGL2 `<canvas>` calling only `gl.clear(gl.COLOR_BUFFER_BIT)` on each `requestAnimationFrame` (`1,200` frames per pass), both maximised Edge and the maximised desktop app (`resonance-app.exe`, `2048 × 1258` CSS pixels; live Mix telemetry waits for the M7 native binary channel in [ADR 0011](../adr/0011-bench-in-the-page-and-the-cli.md)) missed the same two percent of 240 Hz refreshes:

| Host and canvas (device pixels)    | Passes | Refresh | Median | 99th percentile | Longest        | Missed per 1,200 |
| ---------------------------------- | ------ | ------- | ------ | --------------- | -------------- | ---------------- |
| Edge 157, still `2178 × 276`       | 3      | 4.2 ms  | 4.2 ms | 4.3 to 8.4 ms   | 8.4 to 8.6 ms  | 8 to 37          |
| Edge 157, still `2178 × 1220`      | 3      | 4.2 ms  | 4.2 ms | 8.3 to 8.4 ms   | 8.4 to 12.5 ms | 15 to 30         |
| WebView2 157, still `2178 × 276`   | 2      | 4.2 ms  | 4.2 ms | 8.4 ms          | 8.4 to 8.6 ms  | 25 to 28         |
| WebView2 157, still `2178 × 1298`  | 2      | 4.2 ms  | 4.2 ms | 8.3 to 8.4 ms   | 8.5 ms         | 25 to 26         |

## Conclusion

The view's own work is not the limit. It takes at most 0.6 ms of UI-thread time a frame at any size (`0.3 ms` across 42,857 full-window frames in the rebuilt view, with a `1.30` to `2.03 ms` longest main-thread task), and each reading from the core lands within 0.4 ms of where the playhead has got to at the 99th percentile.

Because a still `gl.clear` canvas of the same width in both Edge and WebView2 waits two display intervals in `DXGISwapChainImageBacking::Present` on roughly two percent of frames, the 99th percentile on a 240 Hz (`4.2 ms`) panel lands at `8.4 ms` (`2 × 4.2 ms`, 120 Hz) rather than under `8.0 ms`. Across `42,857` full-window frames (three minutes) at `2178 × 276` device pixels on the `RTX 5090 Laptop GPU`, the median frame held `4.2 ms`, the 99th percentile held `8.4 ms` and no frame exceeded `8.8 ms`. [architecture.md](../architecture.md#performance-budgets) and `resonance bench` therefore judge the Mix view on keeping its median frame at the display's refresh rate, its 99th percentile within two display intervals and no frame over `16 ms`.

Audio was stable at the device's 480-frame shared period: over about 18 minutes of two decks playing across the spike and M1 runs, no underruns, late callbacks, starved callbacks or stream errors occurred, and no callback exceeded 0.38 ms of its 10 ms. Exclusive mode reaches a 256-frame period inside a 256-frame buffer ([ADR 0015](../adr/0015-wasapi-exclusive-output.md)) and held it for a full hour (`3,600 s`, `675,010` callbacks) with `0` underruns, `1` late callback, `0` starved callbacks, `0` stream errors and a `0.34 ms` longest callback ([#47](https://github.com/BradHudsonGrant/RESONANCE/issues/47)).

## 26 November 2026, three-band lanes ([#186](https://github.com/BradHudsonGrant/RESONANCE/issues/186))

The lanes now read four bytes a bin (peak and three band levels) from an RGBA texture. Same machine, Edge, a `1418 × 810` viewport, dev build of the core, a 48 kHz output with a 480-frame period, display refresh 6.1 ms (164 Hz). Two real tracks played on both decks, three short runs of 1,200 frames.

| Run | Refresh | 99th percentile | Longest | Missed | View, longest | Underruns |
| --- | ------- | --------------- | ------- | ------ | ------------- | --------- |
| 1   | 6.1 ms  | 6.2 ms          | 6.3 ms  | 0      | 0.2 ms        | 0         |
| 2   | 6.1 ms  | 6.2 ms          | 12.1 ms | 4      | 0.2 ms        | 0         |
| 3   | 6.1 ms  | 6.2 ms          | 6.2 ms  | 0      | 0.2 ms        | 0         |

The three bands cost nothing measurable: the median stays at the refresh, the 99th percentile inside two intervals and every frame under 16 ms.
