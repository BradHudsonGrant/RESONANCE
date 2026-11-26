# Native shell

## 13 November 2026, installed app

Machine: ROG Zephyrus G16, Core Ultra 9 285H, 64 GB RAM, Windows 11 Pro.

The 3.35 MiB Windows installer installed the app at `%LOCALAPPDATA%\Resonance\resonance-app.exe`.

- The first launch showed an empty library.
- The scan form read `fixtures-local` and reported `50 files: 50 added, 0 changed, 0 unchanged`. The table showed 50 tracks.
- Closing and reopening the installed app kept all 50 tracks.

Uninstall behaviour was not tested.

## Startup observation

The installed process was closed, then the registered app `com.resonance.app` was launched through UI automation. The accessibility tree was polled until the window showed both the 50-track count and the track grid. One timer covered the launch call through the first matching observation, within the same automation call.

That interval was **714 ms**, including automation overhead. This was one observation with a new process, a warm filesystem cache and the 50 fixture tracks.

The [cold-start budget](../architecture.md#performance-budgets) remains open. A disk-cold run and a 100,000-track library still need to be measured.

## 13 November 2026, v0.1.0 release installer

`Resonance_0.1.0_x64-setup.exe` (3.76 MiB, `sha256:83f5ccbf9a4e48ecbc6f78052a52f539e152dd842e97856b9761597b6d0a9007`) was downloaded from the published [`v0.1.0` pre-release](https://github.com/BradHudsonGrant/RESONANCE/releases/tag/v0.1.0) into `target/release-check` and installed in current-user mode at `%LOCALAPPDATA%\Resonance\resonance-app.exe` (13.44 MiB).

- Launched against a fresh data folder, the window showed the empty library state (`No tracks yet`).
- Scanning `fixtures-local` through the scan form filled the track table with all 50 tracks (`50 tracks`, `aria-rowcount=51`).
- Closing and reopening the installed app kept all 50 tracks in the table.

## 15 November 2026, M1 startup UI-thread trace and cold start

Machine: the same ROG Zephyrus G16 on mains power, RTX 5090 Laptop GPU, WebView2 157.0.4341.46 at 1440 by 900 CSS pixels, device pixel ratio 1.25. All runs used the release `resonance-app.exe` built in benchmark mode against the 100,000-track synthetic library, measured with `ui/dev/bench.ts` (`--trace-startup=toplevel` over the first 5 s, plus five warm-profile launches reading the `tracks-filled` mark).

After M1 placed `<Wave variant="sidebar" />` in the always-mounted sidebar, three `toplevel` startup traces recorded longest main-thread tasks of **126.2, 138.4 and 151.9 ms**. A detailed `toplevel,devtools.timeline,blink` trace (`116.7 ms` longest task) put **73.7 ms** in `Wave.svelte` opening its WebGL2 context and linking shaders synchronously while WebView2's GPU process was still starting, **19.3 ms** in evaluating the single entry bundle with synchronous `mount(App)`, **19.2 ms** in forced initial style and layout from reading canvas size and colour tokens on mount, and **21.5 ms** in the microtask checkpoint where the first track page mounted `TrackTable` with synchronous `clientHeight` and `offsetHeight` bindings.

One change split those steps across separate tasks: `main.ts` imports `App` after parsing finishes and mounts it on a timer, `Mix`, `SettingsSheet`, `ScanFolder` and the benchmark helpers wait until needed, `Wave` and `TrackTable` read dimensions from `ResizeObserver` callbacks after layout, and `WaveRenderer.prepare` opens WebGL2 after the first frame presents and polls `KHR_parallel_shader_compile` across animation frames.

| Startup `toplevel` trace | Longest `RunTask` | Longest microtask checkpoint |
| ------------------------ | ----------------- | ---------------------------- |
| 1                        | 27.3 ms           | 10.1 ms                      |
| 2                        | 28.8 ms           | 10.7 ms                      |
| 3                        | 28.2 ms           | 10.2 ms                      |
| 4                        | 29.5 ms           | 9.9 ms                       |
| 5                        | 28.4 ms           | 10.1 ms                      |

Cold-start launches to the first filled rows of 100,000 tracks, plus idle memory, wave and scroll-plus-wave main-thread checks on the same build:

| Launch                            | To the page starting | To filled rows |
| --------------------------------- | -------------------- | -------------- |
| First, with a new webview profile | 0.50 s               | 0.65 s         |
| 1                                 | 0.27 s               | 0.39 s         |
| 2                                 | 0.27 s               | 0.39 s         |
| 3                                 | 0.28 s               | 0.42 s         |
| 4                                 | 0.28 s               | 0.41 s         |
| 5                                 | 0.28 s               | 0.41 s         |

- Idle memory 10 s after launch (7 processes): **130 MB** private working set, **456 MB** working set.
- `measureWave()` over 600 frames: **0.0 ms** median, **0.1 ms** 99th percentile, **0.2 ms** longest.
- Traced `measureTableScroll()` + `measureWave()` longest main-thread task: **11.1 ms**.

### Conclusion

Cold start (**0.42 s** at the longest of five launches, under `1.5 s`), idle memory (**130 MB**, under `400 MB`), the wave (**0.2 ms**, under `1 ms`) and post-startup UI-thread tasks (**11.1 ms**, under `16 ms`) all meet their budgets, and splitting startup cut the M1 main-thread stall from **126.2–151.9 ms** to **27.3–29.5 ms**, so the change is kept. A detailed trace on the updated build (`29.2 ms` longest task) shows three startup tasks still over `16 ms`: Blink's first `BeginMainFrame` pass (**29.2 ms**, with `5.2 ms` initial style and `17.7 ms` initial layout across 323 layout objects), WebView2's warm `canvas.getContext('webgl2')` setup (**26.5 ms**), and `mount(App)` (**18.2 ms**). The startup UI-thread budget remains open in [#55](https://github.com/BradHudsonGrant/RESONANCE/issues/55) for M2, pointing at moving `WaveRenderer` to an `OffscreenCanvas` worker and mounting secondary shell regions after the first paint.

## 18 November 2026, M2 startup UI-thread trace

Machine and method as on 15 November: the release `resonance-app.exe` in benchmark mode against the 100,000-track synthetic library, `--trace-startup=toplevel` launches, longest `RunTask` on the page's main thread. One launch before the change created the webview profile and is left out (135.7 ms).

By M2 the shell had grown search, filters, collections, preview and the Activity queue, and the longest startup task was back to 24.7–36.5 ms. A detailed trace showed three tasks over 16 ms:

- **38.6 ms**: `mount(App)` creating the whole shell, including a 15.8 ms minor garbage collection.
- **30.8 ms**: Blink's first frame, with 4.8 ms style and 17.2 ms layout.
- **29.0 ms**: the sidebar wave's `canvas.getContext('webgl2')`, 21.0 ms of it waiting on the GPU process.

One change: `App.svelte` mounts the library frame first, and the sidebar with its wave, the inspector dock, the transition dock and the player in a task after that frame presents.

| Startup `toplevel` trace | Before  | After   |
| ------------------------ | ------- | ------- |
| 1                        | 36.1 ms | 20.4 ms |
| 2                        | 31.5 ms | 20.4 ms |
| 3                        | 36.5 ms | 26.7 ms |
| 4                        | 24.7 ms | 20.2 ms |
| 5                        |         | 26.9 ms |

The detailed trace after the change has no long `mount(App)` task, and opening the WebGL2 context fell to 15.1 ms. Two tasks are still over 16 ms: Blink's first frame (**27.4 ms**: 4.0 ms style, 10.9 ms layout and 7.0 ms of `ResizeObserver` callbacks) and a forced layout (**16.6 ms**, 14.4 ms of it layout).

### Conclusion

The change cut the longest startup task by about 10 ms, so it is kept. The budget is still missed, and [#55](https://github.com/BradHudsonGrant/RESONANCE/issues/55) moves to M3 for the first frame's layout and the `ResizeObserver` work inside it. Cold start was not measured again on this build.

## 15 November 2026, v0.2.0 release installer

`Resonance_0.2.0_x64-setup.exe` (3.78 MiB, `sha256:f67e9bfd2e9bb2baf6c84d16a4f2f7efe2ecf167bbd31afc2b77ff0520cb33d9`) was downloaded from the published [`v0.2.0` pre-release](https://github.com/BradHudsonGrant/RESONANCE/releases/tag/v0.2.0) and installed in current-user mode at `%LOCALAPPDATA%\Resonance\resonance-app.exe` (13.46 MiB).

- Launched against a fresh data folder, the window showed the empty library state (`No tracks yet`).
- Scanning `fixtures-local` through the scan form reported `50 files: 50 added, 0 changed, 0 unchanged.` and filled the track grid with all 50 tracks (`50 tracks`, `aria-rowcount=51`).
- Closing and reopening the installed app kept all 50 tracks in the grid (`aria-rowcount=51`).

## 18 November 2026, v0.3.0 release installer

`Resonance_0.3.0_x64-setup.exe` (4.55 MiB, `sha256:262f600f8c7048673c43a01699e99ba7ddaf838a522c44edb616d7841754b7fa`) was downloaded from the [`v0.3.0`](https://github.com/BradHudsonGrant/RESONANCE/releases/tag/v0.3.0) release draft and installed silently (`/S`) in current-user mode over `0.2.0`, at `%LOCALAPPDATA%\Resonance\resonance-app.exe` (16.74 MiB, product version `0.3.0`). The release was published as a pre-release after these checks.

- Launched against a fresh data folder, the window showed the empty library state (`No tracks yet`).
- Scanning `fixtures-local` through the scan form reported `50 files: 50 added, 0 changed, 0 unchanged.` and filled the track grid with all 50 tracks (`50 tracks`, `aria-rowcount=51`).
- Closing and reopening the installed app kept all 50 tracks in the grid (`aria-rowcount=51`).

## 22 November 2026, M3 startup UI-thread trace

Machine and method as on 15 and 18 November: the release `resonance-app.exe` in benchmark mode against the 100,000-track synthetic library, five `--trace-startup=toplevel` launches after one warmup launch (128.6 ms, left out), longest `RunTask` on `CrRendererMain`.

Before the change, five `toplevel` traces recorded longest main-thread tasks of **32.0, 22.2, 22.8, 22.5 and 22.9 ms**. A detailed trace with the V8 CPU profiler showed four startup tasks over 16 ms:

- **28.7 ms**: Blink's second frame (`21.1 ms` layout across 819 dirty layout objects) after `App.svelte` mounted the sidebar, inspector, transition dock and player in the same frame window as `TrackTable` adding 16 overscan rows.
- **28.3 ms**: `mount(App)` creating the top bar, command bar and library shell (including closed filter and save-view sheets) and starting nine IPC reads in one timer task, followed by **25.9 ms** when the first track page resolved (8.5 ms of it initialising V8's `en-GB` number formatter on the first `trackCount` call).
- **25.1 ms**: WebView2's `AutofillAgent` form scan forcing the document's first style and layout pass (`5.9 ms` style and `18.0 ms` layout) before the first `BeginMainFrame`, followed by **18.7 ms** in `BeginMainFrame` where `TrackTable`'s `ResizeObserver` synchronously set `viewportHeight`, mounted 15 more rows and forced a second layout pass (`9.9 ms` inside `NotifyResizeObservers`).
- **17.9 ms**: `WaveRenderer.prepare` calling `canvas.getContext('webgl2')` on the main thread while waiting on the GPU process, plus `Thumb` opening a synchronous WebGL2 context and reading `clientWidth` when the player bar mounted.

One change: `main.ts` warms `en-GB` number formatting in its own early task, `App.svelte` mounts and flushes layout for one shell region per task (`TopBar`, `CommandBar`, `Library`, `Inspector`, `TransitionDock`, `PlayerBar`, `Sidebar`), `Library.svelte` and `TrackTable.svelte` defer closed sheets and menus until opened, `TrackTable.svelte` records `ResizeObserver` sizes without mutating state inside `NotifyResizeObservers` and mounts initial and overscan rows in four-row tasks, `Thumb` renders its 38 by 16 dot grid on a cached 2D canvas without reading `clientWidth`, and `Wave.svelte` transfers the sidebar canvas to an `OffscreenCanvas` worker after the first frame establishes the GPU channel.

| Startup `toplevel` trace | Before  | After   |
| ------------------------ | ------- | ------- |
| 1                        | 32.0 ms | 12.5 ms |
| 2                        | 22.2 ms | 11.7 ms |
| 3                        | 22.8 ms | 11.6 ms |
| 4                        | 22.5 ms | 11.2 ms |
| 5                        | 22.9 ms | 11.4 ms |

### Conclusion

All five startup `toplevel` runs are under the 16 ms UI-thread budget (**11.2–12.5 ms**, median **11.6 ms**, down from **22.2–32.0 ms**). This closes [#55](https://github.com/BradHudsonGrant/RESONANCE/issues/55).

## 26 November 2026, startup task at v0.5.0

The v0.4.0 bench traced a 35.5 ms task while the desktop app started. The v0.5.0 bench, with the same trace of the first 5 s, records 11.4 ms, in line with the 11.2 to 12.5 ms above ([budgets-v0.5.0.md](budgets-v0.5.0.md)). No startup code was changed for it, and one trace does not show what made the v0.4.0 task long. The budget is met, which closes [#154](https://github.com/BradHudsonGrant/RESONANCE/issues/154). If a later bench reports a long startup task again, trace it then.
