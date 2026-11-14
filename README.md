# Resonance

Resonance is a desktop app for DJs. It looks after a music library and helps the person using it get better at mixing: tagging, audio analysis, mixing with live coaching, and review of recorded mixes.

Most DJs will keep playing on their own hardware. Resonance is where they prepare, practise and review.

Today the app scans a music folder into a local SQLite library without changing any file on disk, shows the tracks in a paged table with an inspector, and runs a two-deck Mix view that plays audio with scrolling WebGL2 waveforms and live meters. Next come the full visual design, watched library sources and search, track identification and tag writing, file quality checks and in-place replacement, audio analysis, full deck controls, live mix coaching, and review of recorded mixes.

![Resonance library view](docs/images/library.png)

## Status

M0 Foundations (v0.1.0) is nearly done: the Cargo workspace, the local core and browser development loop, folder scanning, the library table and inspector, the Mix audio and waveform spike, the Tauri desktop shell, the Windows installer and the performance budget benchmarks are in place. M1 Design (v0.2.0) comes next and rebuilds the shell, Library and Mix views to the design spec. [docs/roadmap.md](docs/roadmap.md) shows what is done and what is next.

## Install

There is no published release yet. Release v0.1.0 ([#23](https://github.com/BradHudsonGrant/RESONANCE/issues/23)) will attach a Windows installer (`Resonance_0.1.0_x64-setup.exe`) to the [GitHub releases page](https://github.com/BradHudsonGrant/RESONANCE/releases).

To build the Windows installer from a clone now, set up the prerequisites under [Build](#build), install the UI dependencies (`npm ci --prefix ui`), and run:

```powershell
just installer
```

That writes `target/release/bundle/nsis/Resonance_0.1.0_x64-setup.exe`. Running it installs Resonance for the current user at `%LOCALAPPDATA%\Resonance\resonance-app.exe`.

## Build

### Prerequisites

- Windows 10 or 11 (x64).
- Visual Studio C++ build tools and the Windows SDK (`Desktop development with C++` in Visual Studio or Build Tools for Visual Studio), which Rust needs on Windows for the `x86_64-pc-windows-msvc` target.
- Rust `1.100.0` with `rustfmt` and `clippy`, pinned in [rust-toolchain.toml](rust-toolchain.toml). With [rustup](https://rustup.rs) installed, running `rustup toolchain install` in the repository root installs it.
- Node.js 24 (with `npm`).
- [`just`](https://github.com/casey/just) `1.59.0`, [`cargo-deny`](https://github.com/EmbarkStudios/cargo-deny) `0.20.2` and [`typos-cli`](https://github.com/crate-ci/typos) (`typos`) `1.52.2`, matching the versions in [.github/workflows/ci.yml](.github/workflows/ci.yml):
  ```powershell
  cargo install just@1.59.0 cargo-deny@0.20.2 typos-cli@1.52.2 --locked
  ```
- Microsoft Edge WebView2 Runtime (included with Windows 11) for the Tauri desktop window, and Microsoft Edge for the Playwright browser tests.

### From a clone to a running app

1. Clone the repository and install the UI packages:
   ```powershell
   git clone https://github.com/BradHudsonGrant/RESONANCE.git
   cd RESONANCE
   npm ci --prefix ui
   ```
   `just` sets `CARGO_HOME` and `NPM_CONFIG_CACHE` inside `target/`, so build outputs and caches stay inside the repository folder.

2. Load tracks into the development library. To scan a music folder into `target/dev/data` (the files on disk are only read):
   ```powershell
   just scan C:\path\to\music
   ```
   Or generate 100,000 made-up tracks in `target/dev/synthetic`:
   ```powershell
   just synthetic-library
   ```

3. Run in a browser (the day-to-day development loop, where UI changes hot-reload without a Rust build). In one terminal, start the local core process:
   ```powershell
   just serve
   ```
   If you generated the synthetic library in step 2, pass its data folder instead:
   ```powershell
   just serve target/dev/synthetic
   ```
   In a second terminal, start the Vite dev server on `http://localhost:5173`:
   ```powershell
   just ui
   ```
   Open `http://localhost:5173` in a browser (`http://localhost:5173/design.html` shows the design primitives in both themes). You can also scan a folder from the form in the page.

4. Or run the packaged desktop window. This builds the frontend bundle and opens the Tauri shell against `target/dev/data`:
   ```powershell
   just app
   ```
   To open the desktop window against the synthetic library instead:
   ```powershell
   just app target/dev/synthetic
   ```

5. Run the verification suite (UI type check, lint, unit tests and build, `cargo fmt`, `clippy`, `typos`, `cargo-deny`, Rust tests, TypeScript binding check, workspace build, Playwright browser tests in Edge, third-party notices, version check and commit attribution check):
   ```powershell
   just verify
   ```

   To measure the performance budgets in [docs/architecture.md](docs/architecture.md#performance-budgets) and write the results to `docs/benchmarks/` (this opens the desktop window and a headed Edge window and plays audio through the output device):
   ```powershell
   just bench
   ```

## Docs

| File | What is in it |
| --- | --- |
| [docs/product.md](docs/product.md) | What Resonance is, the rules every feature follows, and the features |
| [docs/roadmap.md](docs/roadmap.md) | The milestones as checklists |
| [docs/architecture.md](docs/architecture.md) | The stack, the repository layout, the rules, the data model and the performance budgets |
| [docs/adr/](docs/adr/) | The decisions and the reasons for them |
| [docs/design/](docs/design/) | Structure, layout, interaction, the visual language and the wave |
| [docs/benchmarks/](docs/benchmarks/) | Build times and performance budget measurements |
| [docs/v3-notes.md](docs/v3-notes.md) | What the previous app taught us |
| [CONTRIBUTING.md](CONTRIBUTING.md) | How work gets into the repo |

## Stack

A Rust workspace for the core, Tauri 2 for the desktop shell and Svelte 5 for the UI. Windows first.

## Licence

The code is not yet licensed for reuse. A licence comes if and when the repo becomes public.
