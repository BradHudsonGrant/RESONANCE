# Roadmap

One minor version per milestone. A box is ticked when the work is merged, with the evidence next to it.

A milestone is finished when every box is ticked with evidence, `just verify` passes, the budgets in [architecture.md](architecture.md#performance-budgets) have been measured again, the work is merged and the release is published. Work stops at the end of each milestone for a report.

The build order is library, then analysis, then mixing, then coaching.

## M0 Foundations, v0.1.0

[Milestone on GitHub](https://github.com/BradHudsonGrant/RESONANCE/milestone/1)

Repo and pipeline:

- [x] Private GitHub repo with squash merge only, branches deleted on merge, wiki off. Evidence: settings read back with `gh repo view` on 12 November 2026.
- [x] Local hooks: `commit-msg` rejects co-author trailers and generator footers, `pre-push` blocks direct pushes to `master`. Evidence: both tested by hand on 12 November 2026, each rejecting a bad input and passing a good one.
- [x] Labels, milestones and the M0 issues. Evidence: 28 labels, 13 milestones, issues [#1](https://github.com/BradHudsonGrant/RESONANCE/issues/1) to [#23](https://github.com/BradHudsonGrant/RESONANCE/issues/23).
- [x] Cargo workspace with the `core` and `cli` crates ([#6](https://github.com/BradHudsonGrant/RESONANCE/issues/6)). Evidence: merged in [#29](https://github.com/BradHudsonGrant/RESONANCE/pull/29); 11 tests pass.
- [x] One verify command, `just verify` ([#7](https://github.com/BradHudsonGrant/RESONANCE/issues/7)). Evidence: merged in [#30](https://github.com/BradHudsonGrant/RESONANCE/pull/30); `just verify` exits 0 on a clean branch and 1 on a commit message with a co-author trailer.
- [x] CI on pull requests ([#8](https://github.com/BradHudsonGrant/RESONANCE/issues/8)). Evidence: merged in [#32](https://github.com/BradHudsonGrant/RESONANCE/pull/32); the PR's own check passed, and `just verify` exits 1 on a badly formatted tree.
- [x] Issue and pull-request templates ([#9](https://github.com/BradHudsonGrant/RESONANCE/issues/9)). Evidence: merged in [#31](https://github.com/BradHudsonGrant/RESONANCE/pull/31); GitHub lists the three issue templates and the PR template.
- [x] Version script, changelog and release workflow ([#10](https://github.com/BradHudsonGrant/RESONANCE/issues/10)). Evidence: merged in [#41](https://github.com/BradHudsonGrant/RESONANCE/pull/41); the `v0.1.0-rc.1` tag drafted an unpublished pre-release with the 3.35 MiB installer attached and the changelog as its notes.

Development loop:

- [x] Typed command layer and `resonance serve` ([#11](https://github.com/BradHudsonGrant/RESONANCE/issues/11)). Evidence: merged in [#33](https://github.com/BradHudsonGrant/RESONANCE/pull/33) and [#34](https://github.com/BradHudsonGrant/RESONANCE/pull/34); the page in a browser shows the core's version through the generated client, and a connection without the token gets a 401.
- [x] `just ui` and `just serve`, with build times recorded ([#12](https://github.com/BradHudsonGrant/RESONANCE/issues/12)). Evidence: merged in [#35](https://github.com/BradHudsonGrant/RESONANCE/pull/35); `just serve` restarted the core after a change in `api` and kept the old one running through a build that failed, and the times are in [benchmarks/build-times.md](benchmarks/build-times.md).
- [x] Test fixtures: generated audio, and a local set of real tracks ([#18](https://github.com/BradHudsonGrant/RESONANCE/issues/18)). Evidence: merged in [#36](https://github.com/BradHudsonGrant/RESONANCE/pull/36); generated WAV and AIFF files decode through Symphonia to the samples that were written, and `fixtures-local/` holds 50 tracks (20 MP3, 10 FLAC, 8 AIFF, 7 WAV, 5 M4A, 1.45 GB) with a manifest, copied on 13 November 2026.

The spike:

- [x] Mix view: two decks playing real files, scrolling waveforms locked to the audio clock, meters over the binary channel, measured ([#13](https://github.com/BradHudsonGrant/RESONANCE/issues/13)). Evidence: merged in [#48](https://github.com/BradHudsonGrant/RESONANCE/pull/48); two decks played fixture tracks for about 14 minutes with no underruns, and the view held a 6.4 ms refresh with no missed frames up to a 2335 by 1128 canvas ([benchmarks/mix-view.md](benchmarks/mix-view.md)). Full-window swap-chain cadence is settled in [#71](https://github.com/BradHudsonGrant/RESONANCE/pull/71) ([#46](https://github.com/BradHudsonGrant/RESONANCE/issues/46)), and the 256-sample buffer stays open in [#47](https://github.com/BradHudsonGrant/RESONANCE/issues/47).

Design and shell:

- [x] Design tokens, both themes, the primitives ([#14](https://github.com/BradHudsonGrant/RESONANCE/issues/14)). Evidence: merged in [#38](https://github.com/BradHudsonGrant/RESONANCE/pull/38); `/design.html` shows every primitive in both themes with the three fonts loaded from the dev server and no request leaving it, and [design/contrast.md](design/contrast.md) has the table, which a test rebuilds from the tokens.
- [x] Scan a folder into the database ([#15](https://github.com/BradHudsonGrant/RESONANCE/issues/15)). Evidence: merged in [#37](https://github.com/BradHudsonGrant/RESONANCE/pull/37); `resonance scan fixtures-local` added 50 tracks with none unreadable, a second run reported 50 unchanged, and a test compares every byte and modified time before and after.
- [x] App shell showing real data from a scan of the fixture folder ([#16](https://github.com/BradHudsonGrant/RESONANCE/issues/16)). Evidence: merged in [#39](https://github.com/BradHudsonGrant/RESONANCE/pull/39); the browser page and the installed app each showed the 50 fixture tracks, and the scroll measurements at 100,000 rows are in [benchmarks/library-table.md](benchmarks/library-table.md), settled in [#70](https://github.com/BradHudsonGrant/RESONANCE/pull/70) ([#40](https://github.com/BradHudsonGrant/RESONANCE/issues/40)).
- [x] The wave component ported, with its frame cost measured ([#17](https://github.com/BradHudsonGrant/RESONANCE/issues/17)). Evidence: merged in [#42](https://github.com/BradHudsonGrant/RESONANCE/pull/42); the empty library shows it, and its longest frame over 3,000 was 0.4 ms of UI-thread time against a 1 ms budget ([benchmarks/wave.md](benchmarks/wave.md)).
- [x] Tauri shell, `just app` and Windows installer ([#20](https://github.com/BradHudsonGrant/RESONANCE/issues/20)). Evidence: merged in [#39](https://github.com/BradHudsonGrant/RESONANCE/pull/39); `just installer` built a 3.35 MiB setup, and the installed app scanned the 50 fixture tracks and kept them after a restart ([benchmarks/native-shell.md](benchmarks/native-shell.md)).
- [x] Playwright suite against the real core ([#21](https://github.com/BradHudsonGrant/RESONANCE/issues/21)). Evidence: merged in [#45](https://github.com/BradHudsonGrant/RESONANCE/pull/45); two tests scan a generated folder through the page, select a track, see the inspector fill and see a missing folder reported, locally and in CI.

Docs:

- [x] Architecture and the first decision records ([#2](https://github.com/BradHudsonGrant/RESONANCE/issues/2)). Evidence: merged in [#24](https://github.com/BradHudsonGrant/RESONANCE/pull/24).
- [x] Design docs ([#3](https://github.com/BradHudsonGrant/RESONANCE/issues/3)). Evidence: merged in [#25](https://github.com/BradHudsonGrant/RESONANCE/pull/25).
- [x] Notes from the previous app ([#4](https://github.com/BradHudsonGrant/RESONANCE/issues/4)). Evidence: merged in [#26](https://github.com/BradHudsonGrant/RESONANCE/pull/26).
- [x] README and contributing guide ([#5](https://github.com/BradHudsonGrant/RESONANCE/issues/5)). Evidence: merged in [#27](https://github.com/BradHudsonGrant/RESONANCE/pull/27).
- [x] Product overview and this roadmap ([#1](https://github.com/BradHudsonGrant/RESONANCE/issues/1)). Evidence: merged in [#28](https://github.com/BradHudsonGrant/RESONANCE/pull/28).
- [x] README finished with a screenshot, install and build steps ([#22](https://github.com/BradHudsonGrant/RESONANCE/issues/22)). Evidence: merged in [#57](https://github.com/BradHudsonGrant/RESONANCE/pull/57); `docs/images/library.png` (112 KiB) shows the 100,000-track synthetic library and inspector, and a fresh worktree followed the install and build steps through `just verify`.

Measure and release:

- [x] `resonance bench` and the first budget measurements ([#19](https://github.com/BradHudsonGrant/RESONANCE/issues/19)). Evidence: merged in [#56](https://github.com/BradHudsonGrant/RESONANCE/pull/56); `just bench` measured every budget on 13 November 2026 and wrote [benchmarks/budgets-v0.1.0.md](benchmarks/budgets-v0.1.0.md). Cold start (0.41 s), idle memory (101 MB) and the wave (0.2 ms) are met. The table, the Mix view and audio stay open in [#40](https://github.com/BradHudsonGrant/RESONANCE/issues/40), [#46](https://github.com/BradHudsonGrant/RESONANCE/issues/46) and [#47](https://github.com/BradHudsonGrant/RESONANCE/issues/47), and the UI thread at startup (22.0 ms) in [#55](https://github.com/BradHudsonGrant/RESONANCE/issues/55).
- [x] Release v0.1.0 ([#23](https://github.com/BradHudsonGrant/RESONANCE/issues/23)). Evidence: merged in [#59](https://github.com/BradHudsonGrant/RESONANCE/pull/59) and [#60](https://github.com/BradHudsonGrant/RESONANCE/pull/60); [`v0.1.0`](https://github.com/BradHudsonGrant/RESONANCE/releases/tag/v0.1.0) is tagged on `master` and published as a pre-release with `Resonance_0.1.0_x64-setup.exe` (3.76 MiB), and the installed build passed the smoke test ([benchmarks/native-shell.md](benchmarks/native-shell.md)).

Accept:

- [x] Every budget that can be measured at this stage has a number recorded in `docs/benchmarks/`. Evidence: [benchmarks/budgets-v0.1.0.md](benchmarks/budgets-v0.1.0.md) has a number for seven of the eight budgets and lists text search as not measurable until M2.
- [ ] The Mix spike meets its budget. Carried over to [#46](https://github.com/BradHudsonGrant/RESONANCE/issues/46) (Mix view frame rate on large canvases, settled in [#71](https://github.com/BradHudsonGrant/RESONANCE/pull/71)) and [#47](https://github.com/BradHudsonGrant/RESONANCE/issues/47) (256-sample audio buffer: exclusive mode reached a 256-frame period with no dropouts in 20 minutes in [#105](https://github.com/BradHudsonGrant/RESONANCE/pull/105); the clean hour is still to run), alongside the table scroll frame budget ([#40](https://github.com/BradHudsonGrant/RESONANCE/issues/40), settled in [#70](https://github.com/BradHudsonGrant/RESONANCE/pull/70)) and startup UI-thread task ([#55](https://github.com/BradHudsonGrant/RESONANCE/issues/55), cut to 27.3–29.5 ms in [#72](https://github.com/BradHudsonGrant/RESONANCE/pull/72), M2).
- [x] A UI change shows up in the browser without a Rust build. Evidence: on 13 November 2026, with `just ui` and `just serve` running, an edit to `App.svelte` showed in the open page without a reload, and `just serve` built nothing.
- [x] A tagged build installs and runs on the laptop, scans the fixture folder and shows the tracks. Evidence: `Resonance_0.1.0_x64-setup.exe` from the [`v0.1.0`](https://github.com/BradHudsonGrant/RESONANCE/releases/tag/v0.1.0) release installed to `%LOCALAPPDATA%\Resonance\resonance-app.exe`, scanned `fixtures-local` (50 tracks) through the scan form, and kept all 50 tracks after closing and reopening ([benchmarks/native-shell.md](benchmarks/native-shell.md)).
- [x] The release page exists with an installer. Evidence: [`v0.1.0`](https://github.com/BradHudsonGrant/RESONANCE/releases/tag/v0.1.0) is published with `Resonance_0.1.0_x64-setup.exe` (3.76 MiB) attached.

## M1 Design, v0.2.0

[Milestone on GitHub](https://github.com/BradHudsonGrant/RESONANCE/milestone/14)

The two concept boards become the design target, with `concept-dark.png` leading. The shell and the existing views are rebuilt to them before the library work, so the library screen is built once.

- [x] The design spec from both boards, with an ADR for the change ([#49](https://github.com/BradHudsonGrant/RESONANCE/issues/49)). Evidence: merged in [#58](https://github.com/BradHudsonGrant/RESONANCE/pull/58); [design/visual-language.md](design/visual-language.md), [design/components.md](design/components.md), [design/ux.md](design/ux.md), [design/shell.md](design/shell.md), [design/library.md](design/library.md), [design/mix.md](design/mix.md) and [design/contrast.md](design/contrast.md) specify every element from both boards with PIL measurements, component layers and responsive rules, and [adr/0012-concept-boards-as-design-target.md](adr/0012-concept-boards-as-design-target.md) records the decision.
- [x] The primitives and micro elements to the spec, shown in `/design.html` ([#50](https://github.com/BradHudsonGrant/RESONANCE/issues/50)). Evidence: merged in [#61](https://github.com/BradHudsonGrant/RESONANCE/pull/61); `tokens.css`, the Layer 1 primitives in `ui/src/lib/design/`, `Waveform.svelte` and shared-renderer `Thumb` bitmaps render in both themes on `/design.html`, every colour pair passes [design/contrast.md](design/contrast.md), and five `measureWave()` runs stayed at 0.0 ms median (0.4 ms longest) in [benchmarks/wave.md](benchmarks/wave.md).
- [x] The app shell to the spec ([#51](https://github.com/BradHudsonGrant/RESONANCE/issues/51), [#64](https://github.com/BradHudsonGrant/RESONANCE/issues/64)). Evidence: merged in [#63](https://github.com/BradHudsonGrant/RESONANCE/pull/63) and [#68](https://github.com/BradHudsonGrant/RESONANCE/pull/68); TopBar, collapsible Sidebar, CommandBar, TransitionDock, Deck cards, PlayerBar, Inspector and SettingsSheet sit in `App.svelte` per [design/shell.md](design/shell.md), `Rail.svelte` is removed, unit and 15 browser tests pass across six viewports in both themes, and five 240 Hz mains-power `measureTableScroll()` runs in the native release app stayed under 8.5 ms in [benchmarks/library-table.md](benchmarks/library-table.md).
- [x] The Library and Mix views to the spec ([#52](https://github.com/BradHudsonGrant/RESONANCE/issues/52)). Evidence: merged in [#69](https://github.com/BradHudsonGrant/RESONANCE/pull/69); `Library.svelte`, `TrackTable.svelte`, `columns.ts`, `Inspector.svelte`, `Mix.svelte`, `Deck.svelte`, `Waveform.svelte` and `renderer.ts` match [design/library.md](design/library.md) and [design/mix.md](design/mix.md) in both themes, unit and browser suites pass, and six 240 Hz mains-power `measureMix()` runs stayed at or under 8.5 ms (`2.03 ms` longest UI task, `0` audio underruns) in [benchmarks/mix-view.md](benchmarks/mix-view.md).
- [x] Release v0.2.0 ([#77](https://github.com/BradHudsonGrant/RESONANCE/issues/77)). Evidence: merged in [#78](https://github.com/BradHudsonGrant/RESONANCE/pull/78) and [#79](https://github.com/BradHudsonGrant/RESONANCE/pull/79); [`v0.2.0`](https://github.com/BradHudsonGrant/RESONANCE/releases/tag/v0.2.0) is tagged on `master` and published as a pre-release with `Resonance_0.2.0_x64-setup.exe` (3.78 MiB), and the installed build passed the smoke test ([benchmarks/native-shell.md](benchmarks/native-shell.md)).

Accept:

- [x] Each built screen, set beside the boards, matches the spec in both themes ([#73](https://github.com/BradHudsonGrant/RESONANCE/issues/73)). Evidence: merged in [#74](https://github.com/BradHudsonGrant/RESONANCE/pull/74); `/design.html`, Library (empty and populated with the 50 `fixtures-local` tracks and Inspector open) and Mix (two tracks loaded) were rendered in Edge at 1586 by 992 in dark and light themes and checked directly beside `concept-dark.png`, `concept are and design 2.png` and [design/](design/) on 15 November 2026.
- [x] Every element on the boards is built, scheduled for a named later milestone, or left out with the reason ([#73](https://github.com/BradHudsonGrant/RESONANCE/issues/73)). Evidence: merged in [#74](https://github.com/BradHudsonGrant/RESONANCE/pull/74); every region, primitive and micro element from both boards is either rendered in M1, scheduled for its milestone (M2 to M12) in interface tooltips and [design/shell.md](design/shell.md), [design/library.md](design/library.md) and [design/mix.md](design/mix.md), or left out with the reason in [adr/0012-concept-boards-as-design-target.md](adr/0012-concept-boards-as-design-target.md) and [design/library.md](design/library.md#left-out-from-the-board).
- [x] The frame budgets for the library table ([#40](https://github.com/BradHudsonGrant/RESONANCE/issues/40)) and the Mix view ([#46](https://github.com/BradHudsonGrant/RESONANCE/issues/46)) are measured again. Evidence: merged in [#70](https://github.com/BradHudsonGrant/RESONANCE/pull/70), [#71](https://github.com/BradHudsonGrant/RESONANCE/pull/71) and [#72](https://github.com/BradHudsonGrant/RESONANCE/pull/72); five 240 Hz mains-power release scroll runs on 100,000 rows stayed at a 4.2 ms median and 8.5 ms longest frame ([benchmarks/library-table.md](benchmarks/library-table.md)), a three-minute (42,857-frame) maximised Mix run at 2178 by 276 device pixels stayed at a 4.2 ms median, 8.4 ms 99th percentile and 8.8 ms longest frame with 0 audio underruns ([benchmarks/mix-view.md](benchmarks/mix-view.md)), and cold start (0.42 s), idle memory (130 MB), the wave (0.2 ms) and the startup UI-thread trace (27.3–29.5 ms, open in [#55](https://github.com/BradHudsonGrant/RESONANCE/issues/55)) are recorded in [benchmarks/native-shell.md](benchmarks/native-shell.md).

## M2 Library, v0.3.0

- [x] Sources with subfolder toggles and watching ([#80](https://github.com/BradHudsonGrant/RESONANCE/issues/80)). Evidence: merged in [#81](https://github.com/BradHudsonGrant/RESONANCE/pull/81); per-source `include_subfolders`, `watch`, recursive subfolder toggles and the editable `ignored_folder` list live in `0002_sources.sql` and `SourcesSettings.svelte`, `SourceWatcher` (`notify` `8.2.2`, [ADR 0013](adr/0013-filesystem-watching-with-notify.md)) coalesces filesystem events into read-only scans and pushes `library_changed` revisions so open tables refresh in place, verified by Rust integration tests and Playwright browser tests.
- [x] Identity by audio hash and fingerprint ([#82](https://github.com/BradHudsonGrant/RESONANCE/issues/82)). Evidence: merged in [#83](https://github.com/BradHudsonGrant/RESONANCE/pull/83); `0003_identity.sql`, `crates/core/src/identity.rs` and `crates/library/src/identity.rs` store BLAKE3 `byte_hash`, full-scale `i32` little-endian `audio_hash` (`blake3` `1.8.7`, `symphonia` `0.6.1` with `gapless` and `verify` enabled), 120-second `rusty-chromaprint` `0.3.0` (`preset_test2`) fingerprints and `AssetAlignment` (`offset_samples`, `tempo_ratio`), while `IdentityWorker` decodes in the background (`809.1 MB/s` across 12 threads, `2.3 min` projected for `113 GB`, `49/49` `fpcalc` wire parity and `0.9924` median bitwise agreement in [ADR 0004](adr/0004-identity-model.md) and [benchmarks/identity.md](benchmarks/identity.md)).
- [x] Text search, filters, sort ([#86](https://github.com/BradHudsonGrant/RESONANCE/issues/86), [#92](https://github.com/BradHudsonGrant/RESONANCE/issues/92)). Evidence: merged in [#91](https://github.com/BradHudsonGrant/RESONANCE/pull/91) and [#93](https://github.com/BradHudsonGrant/RESONANCE/pull/93); `0004_search_and_sort.sql`, `0005_asset_fts_delete_trigger.sql`, `crates/core/src/search.rs`, `crates/db/src/search.rs`, `CommandBar.svelte`, `FilterDrawer.svelte`, `TrackTable.svelte` and `Inspector.svelte` add SQLite `FTS5` (`unicode61 remove_diacritics 2`) search with single-in-flight coalescing and automatic delete cleanup, `All tracks`/`Favourites`/`Recent` pills with `track.favourite` toggles, BPM, key, year, label and format drawer filters, `/ bpm` command filtering and sortable paged headers (`#`, `Title`, `Artist`, `BPM`, `Key`, `Time`), verified by Rust workspace, Vitest and Playwright E2E tests.
- [x] Collections ([#87](https://github.com/BradHudsonGrant/RESONANCE/issues/87), [#95](https://github.com/BradHudsonGrant/RESONANCE/issues/95)). Evidence: merged in [#94](https://github.com/BradHudsonGrant/RESONANCE/pull/94) and [#96](https://github.com/BradHudsonGrant/RESONANCE/pull/96); `0006_collections.sql`, `crates/core/src/collections.rs`, `crates/db/src/collections.rs`, `crates/db/src/search.rs` and `crates/api/src/collections.rs` store `track_id`-keyed collections and stable item positions across moves and rescans, while `Sidebar.svelte`, `Library.svelte`, `CollectionMenu.svelte`, `TrackTable.svelte` and `Inspector.svelte` provide sidebar CRUD, safe delete confirmation, keyboard menu navigation, collapsed-rail active state, multi-select assignment and shortcuts (`c`, `Delete`/`Backspace`), verified by `crates/db/tests/collections.rs`, `crates/api/tests/collections_command.rs`, Vitest and Playwright browser tests.
- [x] Preview playback ([#88](https://github.com/BradHudsonGrant/RESONANCE/issues/88)). Evidence: merged in [#97](https://github.com/BradHudsonGrant/RESONANCE/pull/97); `crates/engine/src/preview.rs`, `crates/engine/src/output.rs` and `crates/engine/src/reader.rs` stream Symphonia packets (`OffsetFile` for ID3v2-prefixed MP4 files) through `rubato::SincFixedIn` into a lock-free `rtrb` ring buffer mixed on the shared output with a 5 ms gain ramp and zero callback allocations or locks, while `crates/api`, `preview.svelte.ts`, `PlayerBar.svelte`, `TrackTable.svelte`, `Inspector.svelte` and `App.svelte` add IPC commands, 144-byte telemetry frames, row/Inspector/double-click/`Space` playback, interactive `ScrubBar` seeking, volume/mute controls and automatic Mix handoff, verified by `resonance-engine`, `resonance-api`, Vitest and Playwright browser tests.
- [x] Missing files and relink ([#89](https://github.com/BradHudsonGrant/RESONANCE/issues/89)). Evidence: merged in [#98](https://github.com/BradHudsonGrant/RESONANCE/pull/98); `0007_missing_files.sql`, `crates/core/src/identity.rs`, `crates/db/src/identity.rs` and `crates/library/src/relink.rs` keep missing tracks in the library with `asset.missing_since`, rank candidate replacements across sources or chosen folders by `byte_hash`, `audio_hash` and Chromaprint similarity (`>= 0.85` within `3.0 s`), and relink or merge scanned duplicates without losing `track_id`, favourites or collection memberships, while `crates/api/src/relink.rs`, `app/src/main.rs` (`rfd`), `RelinkSheet.svelte`, `FilterDrawer.svelte`, `TrackTable.svelte` and `Inspector.svelte` add missing indicators, one-click and manual relinking with mismatch confirmation, and quick row selection/deselection, verified by `crates/library/tests/relink.rs`, `resonance-api`, Vitest and Playwright browser tests.
- [x] Activity with a persistent job queue ([#90](https://github.com/BradHudsonGrant/RESONANCE/issues/90)). Evidence: merged in [#101](https://github.com/BradHudsonGrant/RESONANCE/pull/101); `0008_jobs.sql`, `crates/core/src/jobs.rs`, `crates/db/src/jobs.rs` and `crates/jobs` store tiered `queued`, `running`, `completed`, `failed` and `cancelled` jobs with startup crash recovery, coalescing, cancellation, retry, finished-history clearing and Mix playback backoff (`Mix::any_deck_playing`), while `crates/api/src/jobs.rs`, `TopBar.svelte` and `ActivitySheet.svelte` stream `activity_changed` progress and controls, verified by `crates/jobs/tests/queue.rs`, `crates/api/tests/jobs_command.rs`, Vitest and Playwright browser tests.

Accept:

- [x] A scan of 3,000 files stays responsive throughout ([#90](https://github.com/BradHudsonGrant/RESONANCE/issues/90)). Evidence: merged in [#101](https://github.com/BradHudsonGrant/RESONANCE/pull/101); `scan_chunked` in `crates/library/src/scan.rs` walks folders and reads tags outside the `Database` mutex, commits `200` files per SQLite WAL transaction, commits `16` identity results per transaction and serves `/core` WebSocket calls concurrently, keeping concurrent `tracks` reads at `1.4`–`1.9 ms` median (`2.2`–`6.6 ms` p95), `FTS5` searches at `1.0`–`2.4 ms` median (`2.5`–`11.1 ms` p95), and `888` active scrolling frames at a `6.1 ms` median (`12.4 ms` longest frame) across three `3,000`-file (`1.059 GB`) release runs in [benchmarks/scan-responsiveness.md](benchmarks/scan-responsiveness.md).
- [x] Renaming or moving a file outside the app keeps its identity ([#82](https://github.com/BradHudsonGrant/RESONANCE/issues/82)). Evidence: merged in [#83](https://github.com/BradHudsonGrant/RESONANCE/pull/83); `scan` and `IdentityWorker` reconcile unseen paths by `byte_hash` and `audio_hash` across renames, subfolder moves, cross-source moves and lossless WAV-to-FLAC retags while leaving live duplicates untouched, verified by `crates/library/tests/identity.rs`, `crates/api/src/lib.rs` and `ui/e2e/library.e2e.ts`.
- [x] A synthetic 100,000-row library scrolls and searches smoothly, with numbers recorded ([#86](https://github.com/BradHudsonGrant/RESONANCE/issues/86), [#92](https://github.com/BradHudsonGrant/RESONANCE/issues/92)). Evidence: merged in [#91](https://github.com/BradHudsonGrant/RESONANCE/pull/91) and [#93](https://github.com/BradHudsonGrant/RESONANCE/pull/93); against `100,000` synthetic tracks in headless Edge and release `resonance`, FTS5, filter drawer, pill, `50,000`-row offset, facet and column-sort queries ran in `0.9`–`23.8 ms` median (`<= 25.4 ms` p95, inside `< 50 ms`), live `CommandBar` keystroke-to-DOM updates ran in `6.7`–`14.6 ms` median (`<= 21.8 ms` p95), and `6,000` scroll frames held a `4.2 ms` median and `8.6 ms` longest frame inside the `16 ms` ceiling in [benchmarks/library-table.md](benchmarks/library-table.md).

## M3 Identify and tag, v0.4.0

- [ ] The identification pipeline with free providers
- [ ] Staging with per-field diff
- [ ] Writing tags
- [ ] Filename templates
- [ ] Artwork
- [ ] The Prep space
- [ ] A place in Settings to enter provider keys (AcoustID, Discogs, any paid provider), kept in the OS credential store

Accept:

- [ ] A batch of 50 mixed files goes from untagged to tagged using the keyboard only.
- [ ] Hit rates per pipeline step are measured on the local fixtures.
- [ ] User edits survive a re-run.

## M4 Quality and replace, v0.5.0

- [ ] Integrity and transcode checks
- [ ] Duplicates and versions
- [ ] The replace flow with alignment
- [ ] Journal, undo and trash
- [ ] The adapter interface, plus M3U

Accept:

- [ ] On synthetic pairs with known offsets the measured alignment is within 1 ms.
- [ ] Replacing an MP3 with a FLAC of the same recording updates collections and lands fixture cues within 5 ms.
- [ ] A different-length file is recognised as a different version.
- [ ] Undo restores everything.
- [ ] A crash mid-operation recovers cleanly.

## M5 Analysis, v0.6.0

- [ ] ONNX runtime and model registry
- [ ] Beats, downbeats and tempo
- [ ] Key
- [ ] Loudness and energy
- [ ] Phrases
- [ ] Grid editor
- [ ] Confidence shown throughout
- [ ] Benchmarks

Accept:

- [ ] Grid and key accuracy reported against references on the local fixtures.
- [ ] Analysis throughput and VRAM recorded.
- [ ] Corrections survive re-analysis.

## M6 Discovery, v0.7.0

- [ ] Embeddings and "similar"
- [ ] Matches in the inspector
- [ ] Smart collections
- [ ] Natural-language search with editable chips

Accept:

- [ ] Across twenty real searches from everyday use, the understood query is right or fixable in one edit.
- [ ] Search over the full library returns in under a second.

## M7 Decks, v0.8.0

- [ ] The audio engine, two decks and mixer
- [ ] The action layer
- [ ] Keyboard and mouse
- [ ] Gamepad with an on-screen layout
- [ ] MIDI learn
- [ ] A first Rane System One mapping: transport, cues, faders and EQ, with the platters as far as their messages can be decoded
- [ ] Recording with control log
- [ ] The Next panel

Accept:

- [ ] An hour of playback with no dropouts at the chosen buffer size.
- [ ] Latency measured.
- [ ] A full mix can be played from the keyboard alone and from a gamepad alone.
- [ ] A session records and plays back.

## M8 Coach, v0.9.0

- [ ] Live measurements
- [ ] The guidance engine
- [ ] The coach strip
- [ ] Transition cards
- [ ] Session summaries

Accept:

- [ ] On synthetic pairs with known offsets, level differences and key clashes, the coach reports each within tolerance and gives the right prompt.
- [ ] With Coach off there is no measurable cost.

## M9 Replay, v0.10.0

- [ ] Analysis of Resonance sessions
- [ ] Analysis of imported recordings
- [ ] Timeline and notes
- [ ] Practise this transition
- [ ] Progress

Accept:

- [ ] On a mix rendered from known tracks with known errors, Replay finds the transitions and the errors.
- [ ] Exact and estimated figures are labelled.

## M10 Sets and transitions, v0.11.0

- [ ] The set timeline with real durations
- [ ] Suggestions with reasons
- [ ] Transition previews
- [ ] Generation with alternatives

Accept:

- [ ] A generated 90-minute set's stated length is within a minute of its rendered length.

## M11 Other libraries and Organise, v0.12.0

- [ ] rekordbox adapter, read then write
- [ ] Engine DJ adapter, read then write
- [ ] Export
- [ ] Organise with plans, undo and library updates

Accept:

- [ ] Round trips on sandbox copies lose nothing.
- [ ] Every write is backed up and verified.

## M12 Stems and more hardware, v0.13.0

- [ ] Stems on demand
- [ ] Stem controls on the decks
- [ ] More controller mappings, starting with the DDJ-FLX4
- [ ] A first macOS or Linux build on a bundled-Chromium runtime

## 1.0

- [ ] Whatever the earlier milestones left open
- [ ] The v3 clean-up: list every file and folder v3 created (the app folder, its AppData folders, its quarantine folders under `D:\Music`), review the list with sizes, and send what is approved to the Recycle Bin. [v3-notes.md](v3-notes.md) has the starting inventory.
