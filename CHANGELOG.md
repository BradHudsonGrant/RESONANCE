# Changelog

Every release, newest first. Made from the commit history with git-cliff.

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
