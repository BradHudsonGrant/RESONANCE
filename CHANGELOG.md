# Changelog

Every release, newest first. Made from the commit history with git-cliff.

## 0.4.0, 25 November 2026

### Added

- Store provider keys in OS credential store ([#123](https://github.com/BradHudsonGrant/RESONANCE/pull/123))
- Add 6-step identification pipeline and providers ([#124](https://github.com/BradHudsonGrant/RESONANCE/pull/124))
- Add three-layer tags and per-field staging diff ([#125](https://github.com/BradHudsonGrant/RESONANCE/pull/125))
- Choose how keys are shown ([#127](https://github.com/BradHudsonGrant/RESONANCE/pull/127))
- Write tags across formats with journal and undo ([#129](https://github.com/BradHudsonGrant/RESONANCE/pull/129))
- Add filename templates and undoable rename ([#130](https://github.com/BradHudsonGrant/RESONANCE/pull/130))
- Pause and resume jobs from Activity ([#132](https://github.com/BradHudsonGrant/RESONANCE/pull/132))
- Cache and display embedded and provider artwork ([#134](https://github.com/BradHudsonGrant/RESONANCE/pull/134))
- Choose, order, wrap and sort columns ([#135](https://github.com/BradHudsonGrant/RESONANCE/pull/135))
- Add auto-preview while moving through tracks ([#137](https://github.com/BradHudsonGrant/RESONANCE/pull/137))
- Filter on any field with a quick filters bar ([#138](https://github.com/BradHudsonGrant/RESONANCE/pull/138))
- Build Prep space and keyboard-only batch tagging ([#139](https://github.com/BradHudsonGrant/RESONANCE/pull/139))
- Add store searches and check matches by ear ([#148](https://github.com/BradHudsonGrant/RESONANCE/pull/148))
- Review inbox with every tag and covers by sight ([#149](https://github.com/BradHudsonGrant/RESONANCE/pull/149))
- Find Bandcamp pages from the file's names ([#150](https://github.com/BradHudsonGrant/RESONANCE/pull/150))
- One sortable list with no pages ([#153](https://github.com/BradHudsonGrant/RESONANCE/pull/153))

### Fixed

- Show selection actions without moving the table ([#120](https://github.com/BradHudsonGrant/RESONANCE/pull/120))
- Look up one AcoustID fingerprint per request ([#152](https://github.com/BradHudsonGrant/RESONANCE/pull/152))

### Faster

- Keep startup UI-thread tasks under 16 ms ([#141](https://github.com/BradHudsonGrant/RESONANCE/pull/141))

### Docs

- Mark M2 v0.3.0 release complete ([#121](https://github.com/BradHudsonGrant/RESONANCE/pull/121))
- Record a clean hour at a 256-frame buffer ([#140](https://github.com/BradHudsonGrant/RESONANCE/pull/140))

### Tests

- Wait for Prep to load before ticking tracks ([#156](https://github.com/BradHudsonGrant/RESONANCE/pull/156))

## 0.3.0, 18 November 2026

### Added

- Add source subfolder toggles and watching ([#81](https://github.com/BradHudsonGrant/RESONANCE/pull/81))
- Identify tracks by audio hash and fingerprint ([#83](https://github.com/BradHudsonGrant/RESONANCE/pull/83))
- Add FTS5 search, filter drawer and column sort ([#91](https://github.com/BradHudsonGrant/RESONANCE/pull/91))
- Add manual collections in sidebar and table ([#94](https://github.com/BradHudsonGrant/RESONANCE/pull/94))
- Add preview playback in PlayerBar ([#97](https://github.com/BradHudsonGrant/RESONANCE/pull/97))
- Flag missing files and relink moved copies ([#98](https://github.com/BradHudsonGrant/RESONANCE/pull/98))
- Add persistent Activity queue and chunked scan ([#101](https://github.com/BradHudsonGrant/RESONANCE/pull/101))

### Fixed

- Coalesce live search and guard asset_fts deletes ([#93](https://github.com/BradHudsonGrant/RESONANCE/pull/93))
- Move row star off play slot and polish menus ([#96](https://github.com/BradHudsonGrant/RESONANCE/pull/96))

### Faster

- Mount secondary shell regions after the first frame ([#102](https://github.com/BradHudsonGrant/RESONANCE/pull/102))
- Support 256-sample low-latency audio buffer ([#105](https://github.com/BradHudsonGrant/RESONANCE/pull/105))

### Docs

- Mark M1 v0.2.0 release complete ([#79](https://github.com/BradHudsonGrant/RESONANCE/pull/79))
- Add grid snapping, DJ cues and crest factor ([#85](https://github.com/BradHudsonGrant/RESONANCE/pull/85))
- Add MIT licence and prepare public docs ([#100](https://github.com/BradHudsonGrant/RESONANCE/pull/100))

## 0.2.0, 15 November 2026

### Added

- Build primitives and micro elements to spec ([#61](https://github.com/BradHudsonGrant/RESONANCE/pull/61))
- Implement application shell to design spec ([#63](https://github.com/BradHudsonGrant/RESONANCE/pull/63))
- Rebuild Library and Mix views to design spec ([#69](https://github.com/BradHudsonGrant/RESONANCE/pull/69))

### Fixed

- Finish #51 layout, tooltip and theme fixes ([#68](https://github.com/BradHudsonGrant/RESONANCE/pull/68))

### Faster

- Verify 240 Hz scroll budget and settle wording ([#70](https://github.com/BradHudsonGrant/RESONANCE/pull/70))
- Verify full-window stability and settle budget ([#71](https://github.com/BradHudsonGrant/RESONANCE/pull/71))
- Split startup tasks and defer sidebar wave init ([#72](https://github.com/BradHudsonGrant/RESONANCE/pull/72))

### Docs

- Mark M0 v0.1.0 release complete ([#60](https://github.com/BradHudsonGrant/RESONANCE/pull/60))
- Restore dropped rules and correct ADR 0012 ([#67](https://github.com/BradHudsonGrant/RESONANCE/pull/67))
- Record bar-synchronous features and transition scoring ([#66](https://github.com/BradHudsonGrant/RESONANCE/pull/66))
- Record M1 design acceptance ([#74](https://github.com/BradHudsonGrant/RESONANCE/pull/74))
- Refresh screenshots and status for v0.2.0 ([#76](https://github.com/BradHudsonGrant/RESONANCE/pull/76))

## 0.1.0, 14 November 2026

### Added

- Add cargo workspace with core and cli crates ([#29](https://github.com/BradHudsonGrant/RESONANCE/pull/29))
- Add just verify ([#30](https://github.com/BradHudsonGrant/RESONANCE/pull/30))
- Add typed commands and resonance serve ([#33](https://github.com/BradHudsonGrant/RESONANCE/pull/33))
- Add the UI package and the typed core client ([#34](https://github.com/BradHudsonGrant/RESONANCE/pull/34))
- Scan a folder into the database ([#37](https://github.com/BradHudsonGrant/RESONANCE/pull/37))
- Add tokens, both themes and the primitives ([#38](https://github.com/BradHudsonGrant/RESONANCE/pull/38))
- Add the desktop shell with a paged library table ([#39](https://github.com/BradHudsonGrant/RESONANCE/pull/39))
- Port the wave and show it in the empty library ([#42](https://github.com/BradHudsonGrant/RESONANCE/pull/42))
- Add the Mix-view spike with two decks and WebGL2 lanes ([#48](https://github.com/BradHudsonGrant/RESONANCE/pull/48))
- Add resonance bench and the first budget measurements ([#56](https://github.com/BradHudsonGrant/RESONANCE/pull/56))

### Fixed

- Keep the build machine's folders out of the program ([#44](https://github.com/BradHudsonGrant/RESONANCE/pull/44))

### Docs

- Add architecture and first decision records ([#24](https://github.com/BradHudsonGrant/RESONANCE/pull/24))
- Add structure, visual language and wave docs ([#25](https://github.com/BradHudsonGrant/RESONANCE/pull/25))
- Add notes from the previous app ([#26](https://github.com/BradHudsonGrant/RESONANCE/pull/26))
- Add readme and contributing guide ([#27](https://github.com/BradHudsonGrant/RESONANCE/pull/27))
- Add product overview and roadmap ([#28](https://github.com/BradHudsonGrant/RESONANCE/pull/28))
- Add the Design milestone before Library ([#54](https://github.com/BradHudsonGrant/RESONANCE/pull/54))
- Add screenshot, install and build steps ([#57](https://github.com/BradHudsonGrant/RESONANCE/pull/57))
- Specify UI from concept boards and record ADR 0012 ([#58](https://github.com/BradHudsonGrant/RESONANCE/pull/58))

### Tests

- Add generated audio and the local set of real tracks ([#36](https://github.com/BradHudsonGrant/RESONANCE/pull/36))
- Add a Playwright suite against the real core ([#45](https://github.com/BradHudsonGrant/RESONANCE/pull/45))

### Build and tooling

- Add gitignore and gitattributes
- Add issue and pull-request templates ([#31](https://github.com/BradHudsonGrant/RESONANCE/pull/31))
- Run just verify on pull requests ([#32](https://github.com/BradHudsonGrant/RESONANCE/pull/32))
- Add just ui, just serve and build times ([#35](https://github.com/BradHudsonGrant/RESONANCE/pull/35))
- Add the version script, changelog and release workflow ([#41](https://github.com/BradHudsonGrant/RESONANCE/pull/41))
