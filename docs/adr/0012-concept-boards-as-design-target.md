# 0012: The concept boards become the design target

Date: 13 November 2026. Status: accepted. Corrected on 14 November 2026 ([#62](https://github.com/BradHudsonGrant/RESONANCE/issues/62)): the decisions below now say which came from [#49](https://github.com/BradHudsonGrant/RESONANCE/issues/49) and which are this ADR's own, and Activity and Settings moved to the top bar.

## Context

The M0 skeleton (v0.1.0) followed the brief's own layout: a 56 px icon rail, a 40 px top bar, a plain table beside the inspector, and corners of 4 px on controls and 6 px on panels. The brief treated the dark concept image as mood only, with its own layout taking priority.

Two concept boards in `handoff/` show the intended interface in detail:

- `concept-dark.png`, 1586 by 992, is the whole window. A top bar holds the mark, the wordmark, and audio, Activity and Settings buttons at the right. Under it sit a 256 px sidebar with full names, collections, an active set and the dot wave at its foot, a search and command bar with slash commands, collection cards, a dense table with inline waveforms and energy meters, and an inspector with artwork, traits, an energy curve and transition suggestions. At the bottom are an A to B dock with two decks and the crossfade between them, and a player bar inset as a card.
- `concept are and design 2.png`, 1448 by 1086, is the brand and UI system: the mark and its explorations, the palette, the type, small graphic elements, an information architecture, sample components, and the product UI in both themes.

Building M2 Library on the M0 shell and restyling it later would mean building the library screen twice. So M1 Design (v0.2.0) updates the spec ([#49](https://github.com/BradHudsonGrant/RESONANCE/issues/49)), the primitives ([#50](https://github.com/BradHudsonGrant/RESONANCE/issues/50)), the shell ([#51](https://github.com/BradHudsonGrant/RESONANCE/issues/51)) and the Library and Mix views ([#52](https://github.com/BradHudsonGrant/RESONANCE/issues/52)) first.

## Decision

The two boards are the design target. `concept-dark.png` leads on layout, proportions and density. The brand board supplies the mark, the type, the palette, the small graphic elements, the components and the light theme. Where the brief and `concept-dark.png` disagree, the image wins, except where a decision below or a standing rule says otherwise.

### Taken from the image, as #49 decided

- Rounded corners on panels, cards and controls, from 3 to 12 px, larger than the brief's 4 and 6 px.
- The clean, readable density of the table and panels: 33 px rows and 13 px table text.
- The inspector on the right, as the image lays it out, opening straight into the artwork and title.
- The dot-wave art used throughout, not only in empty states.
- The top bar with the wordmark, and the search and command bar under it.
- A left sidebar with full names, open by default, that collapses.
- The A to B dock with the two decks and the crossfade between their waveforms, and the player bar under it.

### Where the boards and the brief disagreed, as #49 decided

- Five spaces, each named in one word: Library, Prep, Sets, Mix and Replay. Prep replaces the brief's Inbox and takes its name from the image's Preparation. Replay replaces the brief's Review. The boards' other names are not used.
- No collection cards above the library table. The table stays the library. Cards with dot-wave art suit generated sets, so they move to Sets (M10).
- An Active set block under the spaces arrives with sets (M10).
- No gradient fills on the interface. Panels, bars, buttons and rows stay flat, separated by borders. Depth in the art comes from the dot wave itself, in monochrome with the accent only for a focus marker.
- A track with no artwork gets a dot-wave thumbnail drawn from its own data. Real artwork is shown as it is once tags bring it (M3).

### Standing rules that still apply

- Monochrome with one orange accent. The coloured status dots and badges on the boards are drawn in monochrome.
- Borders, not shadows. No blur, no gradients, no glass.
- Waveform bars are one device pixel wide, drawn in WebGL2 from precomputed peaks, with one renderer for the overview and the scrolling view.
- Every text pair meets WCAG AA and every mark 3 to 1, checked by a test against the tokens. Reduced motion, more contrast and Windows high contrast are honoured.
- Where each job lives, what is always on screen, what selection does, and the names, as [ux.md](../design/ux.md) sets out.

### Decided here

- Tension is left out: the table column and the tab beside Energy in the inspector. #49 doesn't mention it. It is not a planned analysis output and has no definition.
- The Table and Card view switch is left out, and the library is always a table. #49 says the table stays the library but doesn't name the switch.
- Activity and Settings sit at the top right of the top bar, as `concept-dark.png` shows. This changes #49, which said "Activity and Settings sit at the bottom, as before". The change was made on 14 November 2026, because the image leads on layout.

## Component system and responsive layout

[components.md](../design/components.md) defines one set of components in five layers: tokens, primitives, composed components, regions and screens. Each concept is one component and its placements differ by props, never by copied markup. Components take every size, colour, font, radius and duration from `tokens.css`, and a measurement from the boards sets a token once.

The layout works from 1280 by 720 to 5120 by 2160 at device pixel ratios from 1 to 2, using CSS media and container queries ([ux.md](../design/ux.md#window-sizes)):

- The sidebar collapses to 52 px below 1360 px wide.
- The inspector is `clamp(340px, 26.5vw, 440px)`: 340 at 1280 wide, 420 at 1586 and 440 from 1660.
- Below 1280 px wide the inspector becomes a sheet, closed until a track is selected or I is pressed.
- The dock collapses to its 52 px header below 820 px high.
- The table drops columns with container queries as its own width shrinks.
- The centre workspace takes the extra width on large screens. The sidebar, the inspector's 440 px cap, text sizes and row heights stay fixed.

## Why

Taking the sizes from the image removes guesswork. Writing them as tokens in one component system means #50, #51 and #52 build each piece once, and later milestones feed real data into the same components without redesigning the shell.

## What follows from it

- `docs/design/` is the spec for M1 and the milestones after it.
- #50 updates `tokens.css`, builds the primitives in `ui/src/lib/design/` and shows them all on `/design.html` in both themes.
- #51 builds the top bar, sidebar, command bar, dock and player bar in `ui/src/lib/shell/` and the shell grid in `ui/src/App.svelte`.
- #52 brings `Library.svelte`, `TrackTable.svelte`, `Inspector.svelte` and `Mix.svelte` to the spec, and measures the frame budgets for the 100,000-row table ([#40](https://github.com/BradHudsonGrant/RESONANCE/issues/40)) and the Mix view ([#46](https://github.com/BradHudsonGrant/RESONANCE/issues/46)) again.

## What would change it

If the run after #52 shows the inline waveforms or the rounded row frame pushing the 100,000-row table over its 16 ms frame budget on the reference laptop, the inline waveforms move to one shared canvas over the table or draw only once scrolling stops, and the change is recorded in `docs/benchmarks/`.
