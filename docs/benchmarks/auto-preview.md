# Auto-preview

The budget in [architecture.md](../architecture.md#performance-budgets): with auto-preview on, a key press in the track table is heard as the next track within 150 ms on a warm cache.

## 19 November 2026, first build

Machine: the ROG Zephyrus G16, Core Ultra 9 285H, 64 GB RAM, Windows 11 Pro, playing through the default output device in shared mode at zero volume. The core was a release build of `resonance serve` with a library of 50 local tracks (20 MP3, 10 FLAC, 8 AIFF, 7 WAV and 5 M4A), and the page ran in Edge from the Vite dev server.

Method: with auto-preview on and previews starting 30 % in, `await measureAutoPreview()` in the page's console presses the down arrow 20 times to read the files into the system cache, then presses the up arrow 20 times over the same rows and times those. Each press is timed from the key event to the first frame of the new track at the output. The core's telemetry reports the position being heard. The time already heard since the start is taken off when the reading arrives, and a reading only counts once 50 ms of the new track has played, more than one device period. The trip over the socket stays in, so each figure is an upper bound. The function is in `ui/src/lib/shell/measure-auto-preview.ts`.

| Run | Median | 95th percentile | Slowest |
| --- | --- | --- | --- |
| 1 | 17.1 ms | 23.4 ms | 23.4 ms |
| 2 | 15.8 ms | 19.9 ms | 19.9 ms |
| 3 | 16.7 ms | 21.1 ms | 21.1 ms |

Every swap in the three runs took between 9 and 23 ms, so the budget is met with room to spare.
