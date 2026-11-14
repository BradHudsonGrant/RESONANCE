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

- [x] Mix view: two decks playing real files, scrolling waveforms locked to the audio clock, meters over the binary channel, measured ([#13](https://github.com/BradHudsonGrant/RESONANCE/issues/13)). Evidence: merged in [#48](https://github.com/BradHudsonGrant/RESONANCE/pull/48); two decks played fixture tracks for about 14 minutes with no underruns, and the view held a 6.4 ms refresh with no missed frames up to a 2335 by 1128 canvas ([benchmarks/mix-view.md](benchmarks/mix-view.md)). Larger canvases miss frames, open in [#46](https://github.com/BradHudsonGrant/RESONANCE/issues/46), and the 256-sample buffer is open in [#47](https://github.com/BradHudsonGrant/RESONANCE/issues/47).

Design and shell:

- [x] Design tokens, both themes, the primitives ([#14](https://github.com/BradHudsonGrant/RESONANCE/issues/14)). Evidence: merged in [#38](https://github.com/BradHudsonGrant/RESONANCE/pull/38); `/design.html` shows every primitive in both themes with the three fonts loaded from the dev server and no request leaving it, and [design/contrast.md](design/contrast.md) has the table, which a test rebuilds from the tokens.
- [x] Scan a folder into the database ([#15](https://github.com/BradHudsonGrant/RESONANCE/issues/15)). Evidence: merged in [#37](https://github.com/BradHudsonGrant/RESONANCE/pull/37); `resonance scan fixtures-local` added 50 tracks with none unreadable, a second run reported 50 unchanged, and a test compares every byte and modified time before and after.
- [x] App shell showing real data from a scan of the fixture folder ([#16](https://github.com/BradHudsonGrant/RESONANCE/issues/16)). Evidence: merged in [#39](https://github.com/BradHudsonGrant/RESONANCE/pull/39); the browser page and the installed app each showed the 50 fixture tracks, and the scroll measurements at 100,000 rows are in [benchmarks/library-table.md](benchmarks/library-table.md). The frame budget stays open in [#40](https://github.com/BradHudsonGrant/RESONANCE/issues/40).
- [x] The wave component ported, with its frame cost measured ([#17](https://github.com/BradHudsonGrant/RESONANCE/issues/17)). Evidence: merged in [#42](https://github.com/BradHudsonGrant/RESONANCE/pull/42); the empty library shows it, and its longest frame over 3,000 was 0.4 ms of UI-thread time against a 1 ms budget ([benchmarks/wave.md](benchmarks/wave.md)).
- [x] Tauri shell, `just app` and Windows installer ([#20](https://github.com/BradHudsonGrant/RESONANCE/issues/20)). Evidence: merged in [#39](https://github.com/BradHudsonGrant/RESONANCE/pull/39); `just installer` built a 3.35 MiB setup, and the installed app scanned the 50 fixture tracks and kept them after a restart ([benchmarks/native-shell.md](benchmarks/native-shell.md)).
- [x] Playwright suite against the real core ([#21](https://github.com/BradHudsonGrant/RESONANCE/issues/21)). Evidence: merged in [#45](https://github.com/BradHudsonGrant/RESONANCE/pull/45); two tests scan a generated folder through the page, select a track, see the inspector fill and see a missing folder reported, locally and in CI.

Docs:

- [x] Architecture and the first decision records ([#2](https://github.com/BradHudsonGrant/RESONANCE/issues/2)). Evidence: merged in [#24](https://github.com/BradHudsonGrant/RESONANCE/pull/24).
- [x] Design docs ([#3](https://github.com/BradHudsonGrant/RESONANCE/issues/3)). Evidence: merged in [#25](https://github.com/BradHudsonGrant/RESONANCE/pull/25).
- [x] Notes from the previous app ([#4](https://github.com/BradHudsonGrant/RESONANCE/issues/4)). Evidence: merged in [#26](https://github.com/BradHudsonGrant/RESONANCE/pull/26).
- [x] README and contributing guide ([#5](https://github.com/BradHudsonGrant/RESONANCE/issues/5)). Evidence: merged in [#27](https://github.com/BradHudsonGrant/RESONANCE/pull/27).
- [x] Product overview and this roadmap ([#1](https://github.com/BradHudsonGrant/RESONANCE/issues/1)). Evidence: merged in [#28](https://github.com/BradHudsonGrant/RESONANCE/pull/28).
- [ ] README finished with a screenshot, install and build steps ([#22](https://github.com/BradHudsonGrant/RESONANCE/issues/22))

Measure and release:

- [ ] `resonance bench` and the first budget measurements ([#19](https://github.com/BradHudsonGrant/RESONANCE/issues/19))
- [ ] Release v0.1.0 ([#23](https://github.com/BradHudsonGrant/RESONANCE/issues/23))

Accept:

- [ ] Every budget that can be measured at this stage has a number recorded in `docs/benchmarks/`.
- [ ] The Mix spike meets its budget.
- [x] A UI change shows up in the browser without a Rust build. Evidence: on 13 November 2026, with `just ui` and `just serve` running, an edit to `App.svelte` showed in the open page without a reload, and `just serve` built nothing.
- [ ] A tagged build installs and runs on the laptop, scans the fixture folder and shows the tracks.
- [ ] The release page exists with an installer.

## M1 Design, v0.2.0

[Milestone on GitHub](https://github.com/BradHudsonGrant/RESONANCE/milestone/14)

The two concept boards in `handoff/` become the design target, with `concept-dark.png` leading. The shell and the existing views are rebuilt to them before the library work, so the library screen is built once.

- [ ] The design spec from both boards, with an ADR for the change ([#49](https://github.com/BradHudsonGrant/RESONANCE/issues/49))
- [ ] The primitives and micro elements to the spec, shown in `/design.html` ([#50](https://github.com/BradHudsonGrant/RESONANCE/issues/50))
- [ ] The app shell to the spec ([#51](https://github.com/BradHudsonGrant/RESONANCE/issues/51))
- [ ] The Library and Mix views to the spec ([#52](https://github.com/BradHudsonGrant/RESONANCE/issues/52))

Accept:

- [ ] Each built screen, set beside the boards, matches the spec in both themes.
- [ ] Every element on the boards is built, scheduled for a named later milestone, or left out with the reason.
- [ ] The frame budgets for the library table ([#40](https://github.com/BradHudsonGrant/RESONANCE/issues/40)) and the Mix view ([#46](https://github.com/BradHudsonGrant/RESONANCE/issues/46)) are measured again.

## M2 Library, v0.3.0

- [ ] Sources with subfolder toggles and watching
- [ ] Identity by audio hash and fingerprint
- [ ] Text search, filters, sort
- [ ] Collections
- [ ] Preview playback
- [ ] Missing files and relink
- [ ] Activity with a persistent job queue

Accept:

- [ ] A scan of 3,000 files stays responsive throughout.
- [ ] Renaming or moving a file outside the app keeps its identity.
- [ ] A synthetic 100,000-row library scrolls and searches smoothly, with numbers recorded.

## M3 Identify and tag, v0.4.0

- [ ] The identification pipeline with free providers
- [ ] Staging with per-field diff
- [ ] Writing tags
- [ ] Filename templates
- [ ] Artwork
- [ ] The Inbox
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

## M9 Review, v0.10.0

- [ ] Analysis of Resonance sessions
- [ ] Analysis of imported recordings
- [ ] Timeline and notes
- [ ] Practise this transition
- [ ] Progress

Accept:

- [ ] On a mix rendered from known tracks with known errors, the review finds the transitions and the errors.
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
