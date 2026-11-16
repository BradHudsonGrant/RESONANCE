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

## 15 November 2026, v0.2.0 release installer

`Resonance_0.2.0_x64-setup.exe` (3.78 MiB, `sha256:f67e9bfd2e9bb2baf6c84d16a4f2f7efe2ecf167bbd31afc2b77ff0520cb33d9`) was downloaded from the published [`v0.2.0` pre-release](https://github.com/BradHudsonGrant/RESONANCE/releases/tag/v0.2.0) and installed in current-user mode at `%LOCALAPPDATA%\Resonance\resonance-app.exe` (13.46 MiB).

- Launched against a fresh data folder, the window showed the empty library state (`No tracks yet`).
- Scanning `fixtures-local` through the scan form reported `50 files: 50 added, 0 changed, 0 unchanged.` and filled the track grid with all 50 tracks (`50 tracks`, `aria-rowcount=51`).
- Closing and reopening the installed app kept all 50 tracks in the grid (`aria-rowcount=51`).
