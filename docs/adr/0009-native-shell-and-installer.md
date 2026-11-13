# 0009: Native shell and installer

Date: 13 November 2026. Status: accepted.

## Context

The browser shell reads a real library. The installed app needs the same commands, a place for its data and an installer. A scan must not block the window while it reads tags.

## Decision

- `app/` is a thin Tauri 2.12.5 host. One native command, `dispatch`, calls `Api::handle` on a blocking worker. The local socket uses a blocking worker too. Both return the same generated types and errors.
- The window loads bundled assets. Its capability grants only `dispatch`; navigation stays on the bundled origin and new windows are refused. It starts no socket.
- The CLI and the native app use the same platform data folder. `--data-folder` overrides the database and WebView2 storage together. `just app` uses `target/dev/data`, so a development run stays inside the repo.
- The scan form works through either transport. It returns the count of unreadable files and the first 100 problems, keeping the reply bounded even for a damaged source.
- A `.resonance-ignore` file excludes a folder from scans. The repo carries one, so an installed copy still skips development fixtures when reading a parent music folder. A fixture folder can be scanned directly.
- Windows packaging uses NSIS in current-user mode. Packaging tools are cached under `target/`. WebView2 uses Tauri's bootstrapper when it is missing.
- The installer includes `THIRD-PARTY-NOTICES.txt`. `just notices` rebuilds it from the resolved Windows runtime dependencies and the frontend modules and fonts in the production build. `just verify` rejects stale notices. Licence texts missing from crate packages are copied from pinned upstream revisions, with source URLs and hashes in `ui/dev/licences/sources.json`.

## Why

The native host calls the existing API directly. The browser still hot-reloads against `just serve`, with no Rust build for a UI edit.

NSIS gives one installer executable and needs no administrator account for the app itself. WiX is another toolchain without a requirement that needs it yet.

The frontend's build graph identifies what ships. Walking every npm dependency would also include Svelte's compiler and its dependencies, which are absent from the app.

## Dependencies

Tauri 2.12.5, `tauri-build` 2.7.4 and the JavaScript API and CLI 2.12.4 are Apache-2.0 or MIT. These are the current stable Tauri 2 releases checked against their published package metadata and source. The Windows CLI package is about 16 MB unpacked.

`rfd` 0.18.0 is MIT, released on 7 November 2026, with a 64 kB crate. It shows a native error when the window cannot start. A database error stays visible in the library view; a window failure cannot use that view, and the release build has no console.

`target-lexicon` comes through Tauri's build dependencies under Apache-2.0 with the LLVM exception. The exception grants additional permissions, so it has its own entry in the licence allow list.

## What follows from it

`just verify` builds the frontend before Rust embeds it. Windows verifies the whole workspace. The existing Linux PR job checks the platform-neutral crates; native builds and installer checks run on Windows.

An installer still needs an installed-app smoke test. Running the executable from `target/` proves the native transport and bundled assets, but does not prove installation or uninstall behaviour.
