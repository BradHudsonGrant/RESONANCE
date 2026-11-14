# 0012: The concept boards become the design target

Date: 13 November 2026. Status: accepted.

## Context

The M0 skeleton (`v0.1.0`) was built from a short text brief: a `56 px` icon-only left rail, a `40 px` top bar, a flat table with no command header or inspector, and small corner radii (`4 px` on controls, `6 px` on panels).

Two visual concept boards in `handoff/` now show the intended interface in detail:

- `concept-dark.png` (`1586 x 992` CSS pixels at device pixel ratio `1`) shows the complete application window: a `50 px` top header bar with the 7-bar mark, tracked `16 px` wordmark, and top-right audio status, `Activity` and `Settings` icon buttons; a `256 px` collapsible left sidebar with the `MUSIC INTELLIGENCE FOR DJS` monospace line, full space titles, collection sub-items, an active set summary, and a `254 x 78 px` dot-wave canvas at the bottom; a `42 px` search and command bar (`10 px` corner radius) with a `22 px` slash-command chip strip; a dense library table (`31 px` header, `33 px` rows, `6 px` rounded light selection frame) with inline `140 x 17 px` waveforms and 5-bar energy meters; a `420 px` right-hand track inspector starting directly with `96 x 96 px` artwork and a `20 px` track title; an `A -> B` transition dock (`340 x 87 px` deck cards flanking an `844 x 101 px` crossfade waveform); and a `60 px` rounded, bordered player bar card (`12 px` corner radius) inset `20 px` from the left and right window edges.
- `concept are and design 2.png` (`1448 x 1086` CSS pixels) sets out the supporting brand and UI system across nine panels: the 7-bar asymmetric signal mark, `Space Grotesk` / `Inter` / `JetBrains Mono` typography, the generative WebGL2 dot-wave mesh, micro elements (focus brackets, index marks, data tags, modular dividers, grid dots), UI components (track cards, 4-dot stem meters, waveform scrub bars, key/BPM pills, segmented tabs), the monochrome palette with one orange accent, and the warm-stone light mode variant.

Building M2 Library (`v0.3.0`) on the M0 shell and restyling later would mean writing the library view twice. Milestone M1 (`v0.2.0`) updates the design specification ([#49](https://github.com/BradHudsonGrant/RESONANCE/issues/49)), the primitives ([#50](https://github.com/BradHudsonGrant/RESONANCE/issues/50)), the shell ([#51](https://github.com/BradHudsonGrant/RESONANCE/issues/51)) and the Library and Mix views ([#52](https://github.com/BradHudsonGrant/RESONANCE/issues/52)) before M2 begins.

## Decision

The two concept boards become the design target for Resonance, with `concept-dark.png` leading on layout, proportions, pixel measurements and density, and `concept are and design 2.png` supplying the brand mark, micro elements, component patterns and light theme.

Where the earlier brief disagrees with `concept-dark.png`, the image wins:

1. **Measured dimensions from `concept-dark.png` (`1586 x 992`, DPR `1`).** Every region, control, font size and corner radius in the token set is measured directly from the image:
   - Top bar (`TopBar`): `50 px` tall (`--topbar-height: 50px`), holding the 7-bar mark and `16 px` `RESONANCE` wordmark on the left and the audio-engine status, `Activity` and `Settings` icon buttons (`28 x 28 px`) on the right.
   - Left sidebar (`Sidebar`): `256 px` wide (`--sidebar-width: 256px`, collapsing to `52 px`), keeping the two-line `11 px` monospace subtitle `MUSIC INTELLIGENCE / FOR DJS` at the top and the `254 x 78 px` dot-wave canvas (`Wave`) at the bottom.
   - Search and command bar (`CommandBar`): `42 px` tall (`--control-height-lg: 42px`) with `10 px` corners (`--radius-lg: 10px`), followed by a `22 px` slash-command chip row.
   - Workspace heading and table (`TrackTable`): `27 px` `Library` title (`--text-2xl: 27px`), `31 px` table header row (`--table-header-height: 31px`), and `33 px` data row pitch (`--row-height: 33px`). Selected rows draw the image's `6 px` rounded light control frame (`--radius-sm: 6px`, `1 px` `--color-border-control`) and ticked `16 x 16 px` checkbox, with the orange accent restricted to the `2 px` left selection mark.
   - Right-hand inspector (`Inspector`): `420 px` wide (`--inspector-width: 420px`) with no extra `"INSPECTOR"` header bar, opening straight into the `96 x 96 px` artwork thumbnail (`--thumb-lg: 96px`, `10 px` radius) and `20 px` track title (`--text-xl: 20px`).
   - Bottom `A -> B` transition dock (`TransitionDock`) and player card (`PlayerBar`): a `52 px` dock header row (`A -> B Transition`, `Mix Point`, `Blend`) above two `340 x 87 px` rounded deck cards (`10 px` radius, `60 x 60 px` artwork thumbnail) and an `844 x 101 px` crossfade waveform, followed by a `60 px` tall rounded, bordered player card (`--player-height: 60px`, `--radius-xl: 12px`) inset `20 px` from the left and right window edges and `17 px` from the bottom edge.
   - Corner radii: `--radius-xs: 3px` (checkboxes, trait bar tracks), `--radius-sm: 6px` (buttons, keybadges, slash chips, selected table row frame, removable tags, deck badges, selects), `--radius-md: 8px` (active sidebar item, `60 px` deck artwork, segmented tab container), `--radius-lg: 10px` (search box, `96 px` inspector artwork, Deck A/B cards, collection cards), `--radius-xl: 12px` (bottom player card, modals), and `--radius-pill: 9999px` (filter pills, status pill, circular play and add buttons).
   - Card usage: bordered cards are used where the boards show them — Deck A and Deck B pods in the transition dock, the inset bottom player card, collection cards in the `Sets` overview (M10), and compact track cards in `Prep` (M3) — while the library track list remains a virtualised `33 px` table.

2. **Decisions from Issue [#49](https://github.com/BradHudsonGrant/RESONANCE/issues/49) and standing architectural rules.** The only places where the specification departs from the boards are the explicit product decisions in [#49](https://github.com/BradHudsonGrant/RESONANCE/issues/49) and the standing engineering rules:
   - Five primary spaces in the sidebar: `Library`, `Prep` (`Inbox`, `Duplicates`, `Quality`), `Sets`, `Mix` and `Replay` (replacing the board's `Discover`, `Transitions`, `Set Builder`, `Practice`).
   - Monochrome surfaces with one orange accent (`#E4553D` in dark mode, `#D94F3B` in light mode) reserved for the active selection mark, active playhead, live recording indicator and destructive confirmation. All decorative green, blue, purple and amber dots or badges on the boards become monochrome (`--color-text`, `--color-text-2`, `--color-border-control`).
   - All waveforms (`TrackTable` inline, `TransitionDock` crossfade, `Mix` decks) and the inspector `Energy` curve use flat `1 px` monochrome bars or lines with no gradient fills.
   - Collection cards with dot-wave art move from the top of `Library` into the `Sets` space overview (M10) so the library table opens immediately below the filter pills.
   - The undefined `Tension` metric (table column and inspector curve tab) and the duplicate library Grid/Card view toggle are left out.
   - Every text and control pairing meets WCAG AA contrast (`4.5:1` for body copy, `3:1` for large text and active UI boundaries), verified by `ui/dev/contrast.ts`, and respects `prefers-reduced-motion: reduce` and `prefers-contrast: more`.

## Component system and responsive layout

The specification in [docs/design/components.md](../design/components.md) and [docs/design/ux.md](../design/ux.md) defines a reusable component system rather than fixed static templates:

- **Five layers:** `Tokens` (`ui/src/lib/design/tokens.css`) -> `Primitives` (`ui/src/lib/design/*.svelte`) -> `Composed domain components` (`ui/src/lib/wave/`, `ui/src/lib/library/`, `ui/src/lib/mix/`) -> `Shell and workspace regions` (`ui/src/lib/shell/`, `ui/src/lib/library/Inspector.svelte`) -> `Screens` (`ui/src/App.svelte`, `Library.svelte`, `Mix.svelte`, `DesignPage.svelte`).
- **One component per concept, variants via props:** Every element in the board inventory maps to one PascalCase Svelte component (such as `MonoLabel`, `DataTag`, `Thumb`, `Card`, `Waveform`, `DeckPod`). Visual variations are controlled by typed props (`size`, `variant`, `mode`), never by copying markup or CSS across screens.
- **Zero raw values inside components:** Every size, gap, corner radius, colour, font family, font size and transition duration in a component comes from a CSS custom property in `tokens.css`. The pixel measurements from `concept-dark.png` define the token values once.
- **Responsive from `1280 x 720` to `5120 x 2160` at DPR `1`, `1.25`, `1.5` and `2`:** The shell uses CSS Grid with `minmax()` and `clamp()`, and workspace regions use CSS container queries (`@container`). Below `1360 px` width the `256 px` sidebar collapses automatically to `52 px`; between `1280 px` and `1439 px` the inspector narrows to `340 px` (and becomes a slide-over sheet below `1280 px`); below `820 px` height the transition dock collapses to its `52 px` header strip until expanded; `TrackTable` drops secondary columns (`Energy`, then inline `Waveform`) via container queries as its container narrows; and on QHD (`2560 x 1440`) and ultrawide (`5120 x 2160`) monitors the centre workspace absorbs extra width while `Sidebar` (`256 px`), `Inspector` (`max 440 px`), text sizes, and table row heights (`33 px`) stay fixed.

## Why

Measuring directly from `concept-dark.png` removes guesswork: every border, gap, cap height and corner radius matches the reference artwork. Expressing those measurements as tokens inside a five-layer component system means [#50](https://github.com/BradHudsonGrant/RESONANCE/issues/50), [#51](https://github.com/BradHudsonGrant/RESONANCE/issues/51) and [#52](https://github.com/BradHudsonGrant/RESONANCE/issues/52) build each primitive and region once, and later milestones (M2 through M12) wire real Rust IPC data into the same components without redesigning the shell.

## What follows from it

- `docs/design/visual-language.md`, `docs/design/components.md`, `docs/design/ux.md`, `docs/design/contrast.md`, `docs/design/shell.md`, `docs/design/library.md` and `docs/design/mix.md` are the authoritative specification for M1 (`#50`, `#51`, `#52`) and M2–M12.
- Issue [#50](https://github.com/BradHudsonGrant/RESONANCE/issues/50) updates `ui/src/lib/design/tokens.css`, builds the Layer 1 primitives in `ui/src/lib/design/`, and renders all of them on `/design.html` in both themes.
- Issue [#51](https://github.com/BradHudsonGrant/RESONANCE/issues/51) builds `TopBar`, `Sidebar`, `CommandBar`, `TransitionDock` and `PlayerBar` in `ui/src/lib/shell/` and wires the responsive CSS Grid shell in `ui/src/App.svelte`.
- Issue [#52](https://github.com/BradHudsonGrant/RESONANCE/issues/52) updates `Library.svelte`, `TrackTable.svelte`, `Inspector.svelte` and `Mix.svelte` to the spec, and re-runs the frame-budget benchmarks for the 100,000-row table ([#40](https://github.com/BradHudsonGrant/RESONANCE/issues/40)) and the Mix view ([#46](https://github.com/BradHudsonGrant/RESONANCE/issues/46)).

## What would change it

If the post-#52 benchmark run shows that inline waveforms or rounded row frames push the 100,000-row table over the `16 ms` frame budget on the reference laptop, inline waveforms in `TrackTable` will move to a shared single-canvas overlay or render only when rows settle, and the adjustment will be recorded in `docs/benchmarks/`.
