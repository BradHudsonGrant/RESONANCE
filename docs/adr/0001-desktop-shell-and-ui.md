# 0001: Desktop shell and UI

Date: 12 November 2026. Status: accepted. The shell and UI framework are fixed. The renderer is a default.

## Context

The old app's stack was never a deliberate choice, and nothing ties this project to it. Two things matter here: the best runtime performance available, and day-to-day work that does not mean rebuilding everything. The old app felt slow in use.

The old app was Tauri 2 with Svelte 5, the same shell proposed here. So the shell is the first suspect. Reading its code points at how it was used: all the Rust was one crate with incremental compilation switched off, every call went through JSON, and the UI polled for the playhead. That is a reading of the code. Nobody has measured it.

## Decision

- Desktop shell: Tauri 2. The current release is 2.12.5.
- UI: Svelte 5 (5.57.3), TypeScript in strict mode (7.0.2), Vite (8.3.8).
- Real-time views draw with WebGL2 through a small hand-written renderer. This one is a default.
- The shell holds no logic. Commands are plain typed functions in the `api` crate, called over Tauri IPC in the packaged app and over a localhost WebSocket in development.

## Why

On Windows the Tauri webview is WebView2, which is Chromium. Electron would draw with the same engine and use more memory, so it would not be faster. A native Rust UI or Qt Quick would draw faster, and each has a cost that matters more here: GPUI has no hot reload and every UI change is a Rust build, and Qt's Rust bridges are not stable yet. The details are in [architecture.md](../architecture.md#what-else-was-considered).

Svelte and Vite give hot reload in an ordinary browser. With the core running as `resonance serve`, a UI change needs no Rust build at all. That answers the rebuild complaint directly.

The performance question is settled by measuring. M0 builds the Mix view first as a spike: two decks playing real files, scrolling waveforms locked to the audio clock, meters over a binary channel. The budget is the display's refresh rate, with the 99th-percentile frame under 8 ms at 120 Hz.

## What follows from it

- Every real-time view draws on the GPU and makes no DOM updates per frame. Meter, playhead and waveform data arrive as binary.
- The core must not depend on the shell. That keeps the fallback open.
- macOS and Linux use the system webview under Tauri 2, and WebKitGTK is the weakest of the three. Those platforms ship on a bundled Chromium runtime. Tauri 3 (3.0.0-alpha.6 today) adds one, and Electron is the other route. Choose when that work starts. The UI code is the same either way.
- Stay on WebGL2 until every platform runs Chromium.

## What would change it

If the Mix spike can't meet its budget after a fair attempt at tuning, work stops there. The fallback is a Flutter UI on the same Rust core.

Look at Qt Quick again when the Qt Bridge for Rust is stable.
