# Structure, layout and interaction

Resonance follows the layout and density of `concept-dark.png` and the system tokens and micro elements of `concept are and design 2.png` ([ADR 0011](../adr/0011-concept-boards-as-design-target.md)). Screen details sit in [shell.md](shell.md), [library.md](library.md) and [mix.md](mix.md).

## Structure

Five plain-word spaces sit in a collapsible left sidebar: `Library`, `Prep`, `Sets`, `Mix` and `Replay`. `Activity` and `Settings` sit at the bottom of the sidebar.

Each space is a job with a beginning and an end. Analysis, history and "intelligence" are properties of tracks and sessions, so they appear where the track or session is shown and have no page of their own. `Collections` list under `Library` once they exist (M2), and an `Active set` block sits under the spaces once sets exist (M10).

| Job | Where it lives | Milestone |
| --- | --- | --- |
| Add folders, choose subfolders | Settings > Sources, and the `Library` sidebar | `#51` scan command, M2 Sources |
| Browse, search, filter, collections | `Library` | `#52` table, M2 search & collections |
| New tracks, identify, fix tags, quality flags, duplicates, missing files | `Prep` (replaces `Inbox`; same tools also on any track in `Library`) | M3 Identify & tag, M4 Quality & replace |
| Track details, grid and cue editing, replace | `Inspector`, expandable to a full track view | `#52` facts, M3 tags, M4 replace, M5 grid |
| Similar tracks, what mixes next | `Inspector > Matches`, and the `Next` panel in `Mix` | M6 Discovery, M7 Decks |
| Build and generate sets | `Sets` | M10 Sets and transitions |
| Decks, coach, recording | `Mix` | `#52` two-deck spike, M7 Decks, M8 Coach |
| Recorded mixes, scores, progress | `Replay` (replaces `Review`) | M9 Review |
| Background work and undo history | `Activity` | M2 Library |
| Organise folders | `Library > Organise` (opens a diff plan) | M11 Other libraries and Organise |
| Other DJ libraries, controllers, models, audio device | `Settings` | M2+, M11, M12 |

Always present: the top bar with the Resonance mark and wordmark, the full-width command bar (`Ctrl+K`) and slash-command strip above the workspace, the collapsible left sidebar, the right-hand inspector (`I` toggles it outside `Mix`), and the bottom player area (the A to B transition dock above the player bar).

## Layout

```
+-----------------------------------------------------------------------+
| mark  RESONANCE                                       sidebar / I / A |
+-------------+---------------------------------------+-----------------+
| sidebar     | Search your library, or type... CtrlK | inspector       |
|             | /scan  /mix  /library  /inspector ... | [wave] Title    |
| Library   n +---------------------------------------+ Artist          |
|   Colls     | Library  n tracks               [flt] | [Read] WAV 44.1 |
| Prep        | [All Tracks] [Analysed] [Favourites]  |-----------------|
| Sets        |---------------------------------------| BPM KEY TIME YR |
| Mix         | #  Title      Artist   BPM  Key  Time | GENRE ALB LABEL |
| Replay      | 1  Catalyst   Riven    128  8A   6:24 |-----------------|
|-------------| 2  Echo Ch... Kaden    128  8A   7:02 | TAGS (M3)       |
| Active set  | 3  Luminous   V. Holt  126  7A   6:48 | TRAITS (M5)     |
|-------------| ...                                   | ENERGY (M5)     |
| Activity    |                                       | MATCHES (M6/10) |
| Settings    |                                       |                 |
+-------------+---------------------------------------+-----------------+
| A -> B  Transition          Mix Point (M10)  Blend (M10)              |
| [ A  Echo Chamber ]   [ Preview M2 / Crossfade M10 ]   [ B  Empty ]   |
+-----------------------------------------------------------------------+
| [>] [||]  0:00 / 0:00   ========o====   A Vol ===  B Vol ===  Crv  EQ |
+-----------------------------------------------------------------------+
```

Detailed breakdowns per view:

- **Shell frame, sidebar, command bar, A to B dock and player bar (#51):** [shell.md](shell.md)
- **Library table, filter bar and inspector (#52):** [library.md](library.md)
- **Mix view and later space layouts (#52, M7–M10):** [mix.md](mix.md)

Selecting one track fills the inspector and sets it as Deck A's candidate in the A to B dock. Selecting two tracks offers an A to B preview once transition previews arrive (M10). Selecting many shows batch actions in `Library` and `Prep` (M3).

## Interaction rules

- Everything is reachable from the keyboard. Shortcuts appear in menus and tooltips.
- No chains of modals. Anything that changes files uses one confirm sheet (`Sheet.svelte`) that shows the diff.
- `Ctrl+Z` works for every journaled operation. `Activity` lists past operations with an undo on each.
- Background work never blocks. A job that needs a decision becomes a `Prep` item.
- Every view has designed empty, loading and error states. An error says what happened and what to do next.
- Panels and controls whose data arrives in a later milestone either stay out until that milestone or say plainly what they are waiting for. Never show made-up numbers, waveforms or matches.
- Dense by default: 28 to 32 px rows (`--row-height: 30px`) and tabular numerals. Never place decorative collection cards above the library table.
- Honour the OS settings for reduced motion and high contrast.

## Showing where a value came from

- Measured: plain.
- Estimated by an algorithm or model: the value with a small hollow dot. Hover shows the source, version and confidence.
- Confirmed by the user: a small tick.
- Low confidence: a muted value, plus a `Prep` item.

`Provenance.svelte` renders this once. Every data field that carries provenance uses it.

## Names

Use plain words: `Library`, `Prep`, `Sets`, `Mix`, `Replay`, `Activity`, `Settings`, `Collections`, `Coach`, `Replace`, `Organise`, `Identify`.

Do not bring back names from earlier drafts or consumer-streaming mockups: `Inbox` (replaced by `Prep`), `Review` (replaced by `Replay`), `Job Orchestrator`, `Disk Steward`, `Sonic Constellations`, `Anchor`, `Catalyst`, `Intelligence`, `Listen`, `Explore`, `For You`, `Discovery Mix`, `Taste Profile`, `Listening Insight`, `Profile`.

Interface copy uses British spelling (`Analysed`, `Unanalysed`, `Favourites`, `Organise`, `Colour`, `/analyse`). Code identifiers follow each language's conventions.

## Board element inventory

Every element visible on `concept-dark.png` (Board 1) and `concept are and design 2.png` (Board 2) is listed below with its tokens, sizes, placement and milestone disposition (`#50`, `#51`, `#52`, a named later milestone, or `Left out` with reason).

| Element | Board & location | Tokens & sizes (Dark / Light) | Where it appears in Resonance | Disposition |
| --- | --- | --- | --- | --- |
| macOS traffic-light buttons | Board 1 & 2, top-left | Native window buttons | Not used; Windows uses native frame | Left out (Windows first) |
| Resonance 7-bar mark (`A / CORE MARK`) | Board 1 top-left; Board 2 header & `[02]` | `20 x 20 px` (`16px` bars), `--color-text` + centre `--color-accent` | Top bar (`Mark.svelte`), `/design.html` | `#50` / `#51` |
| `RESONANCE` uppercase wordmark | Board 1 top-left; Board 2 header | `Space Grotesk` `500`, `13px`, uppercase, `0.32em` tracking, `--color-text` | Top bar (`Wordmark.svelte`), `/design.html` | `#50` / `#51` |
| Tagline `"A MUSIC INTELLIGENCE EXPERIENCE"` | Board 2 top header | `JetBrains Mono` `11px` uppercase | Not used in app | Left out (board branding only) |
| Top-bar audio/engine icon | Board 1 top-right | `28 x 28 px` quiet button, `16px` Lucide icon, `--color-text-2` | Top bar right (sidebar, `I`, engine status) | `#51` |
| Top-bar target and gear icons | Board 1 top-right | `28 x 28 px` icons | Moved to sidebar bottom (`Activity`, `Settings`) | Left out of top bar (one home per job) |
| Sidebar subtitle `"MUSIC INTELLIGENCE FOR DJS"` | Board 1 top-left of sidebar | `JetBrains Mono` `11px` uppercase | Not used | Left out (drops "intelligence" branding) |
| Left sidebar container (`220px` / `52px`) | Board 1 & Board 2 `[08]`/`[09]` left | `220px` open / `52px` collapsed, `--color-surface` (`#141415` / `#FFFFFF`), `1px` `--color-border` right | Left column (`Sidebar.svelte`), open by default | `#51` |
| `Library` nav row + track count | Board 1 sidebar | `34px` high, `--radius-control: 6px`, `16px` icon, `13px` `Inter`, `12px` mono count, `2px` `--color-accent` left bar + `--color-raised` (`#252527`) / `--color-surface-2` (`#ECEAE7`) when active | Sidebar primary space | `#51` / `#52` |
| `Collections` group, `+` button & sub-items | Board 1 sidebar (`Peak Time`, `Warm Up`, `Afterhours`, `Deep / Hypnotic`) | `28px` indented rows, `12px` `Inter` `--color-text-2`, `11px` mono count `--color-muted`, `24 x 24 px` `+` button | Indented under `Library` in sidebar | M2 Library |
| `Playlists 24` nav row | Board 1 sidebar; Board 2 `[08]`/`[09]` | `28px`–`34px` rows with list icon | Merged into `Collections` and read-only library mirrors | M2 Library / M11 Other libraries |
| `Preparation` nav row | Board 1 sidebar | `34px` row, `16px` `Layers` icon, `--radius-control: 6px` | Renamed `Prep` (replaces `Inbox`); disabled in `#51` | `#51` slot; M3 Identify and tag |
| `Sets` nav row | Brief & `#49` space list | `34px` row, `16px` `ListMusic` icon | Sidebar space; disabled in `#51` | `#51` slot; M10 Sets and transitions |
| `Mix` nav row | Brief & `#49` space list | `34px` row, `16px` `Disc3` icon | Sidebar active deck view | `#51` / `#52` |
| `Review` / `Replay` nav row | Brief & `#49` space list | `34px` row, `16px` `ChartLine` icon | Named `Replay`; disabled in `#51` | `#51` slot; M9 Review |
| `Intelligence`, `History`, `Analysis` nav rows | Board 1 sidebar | `34px` nav rows | Facts sit on tracks/sessions (`Inspector`, `Activity`, `Replay`) | Left out (no separate pages) |
| `Listen`, `Explore`, `For You`, `Profile` nav & IA tree | Board 2 `[06]`, `[08]`, `[09]` | Consumer streaming IA | Replaced by the 5 DJ spaces | Left out (consumer streaming model) |
| `ACTIVE SET` sidebar block | Board 1 sidebar (`Fri 23:00 — Warehouse`) | `1px` `--color-border` dividers, `11px` mono uppercase header, `6px` `--color-accent` dot, `12px` `Inter`, `...` button | Sidebar below spaces | M10 Sets and transitions |
| Sidebar bottom decorative dot wave | Board 1 bottom-left | Monochrome dot mesh | Replaced by `Activity` and `Settings` rows | Left out (bottom holds tools) |
| `Activity` & `Settings` bottom rows | Brief, `#49` & Board 2 `[08]`/`[09]` bottom icon | `34px` rows at sidebar bottom | Sidebar footer group (`#51` disabled until M2) | `#51` slot; M2 Library |
| Command bar search input & `Ctrl K` badge | Board 1 top of centre column | `36px` tall (`--control-height-lg`), `--radius-panel: 8px`, `--color-surface`, `1px` `--color-border-control`, `13px` `Inter`, `22px` `Ctrl K` cap (`--surface-2`, `11px` mono) | Top of centre column (`CommandBar.svelte`) | `#51` (search M2, NL search M6) |
| Slash-command strip & `...` overflow | Board 1 below search input | `26px` chips (`--radius-control: 6px`, `--surface`), inner `20px` mono badge (`--surface-2`, `11px` `--text`), `12px` `Inter` label | Below command input (`/scan`, `/mix`, `/library`, `/inspector` in `#51`) | `#51` strip; M3 `/tag`; M5 `/analyse`; M6 `/match`; M10 `/set` |
| `Library` heading & track count | Board 1 workspace top-left | `Space Grotesk` `20px` `--color-text` + `JetBrains Mono` `13px` `--color-text-2` | `Library` header | `#52` |
| List / grid view toggle | Board 1 workspace top-right | Dual-icon segmented button | Not used (`Library` is always a table) | Left out (no card view for library) |
| Filter / columns sliders button | Board 1 workspace top-right | `28 x 28 px` button, `--radius-control: 6px`, `--color-surface`, `1px` `--color-border-control` | `Library` header right (`#52` disabled) | `#52` control; M2 Library |
| Filter pills (`All Tracks`, `Analysed`, `Unanalysed`, `Favourites`, `Recent`) | Board 1 below `Library` title | `28px` height, `--radius-pill: 9999px`; active `--color-text` fill / `--color-background` label; inactive `--color-text-2` (`--color-muted` when disabled) | `Library` filter row (British spelling) | `#52` (`All Tracks`); M2 (`Favourites`, `Recent`); M5 (`Analysed`, `Unanalysed`) |
| Collection dot-wave cards (`Peak Time`, `Warm Up`, `Afterhours`, `Deep / Hypnotic`, `+ New Collection`) & media card (`Atmospheric Electronica`) | Board 1 above table; Board 2 `[07]` | `WaveCard` (`--radius-lg: 10px`, `--color-surface`, `1px` `--color-border`, `Wave` canvas, `15px` `Space Grotesk`, `12px` mono, `28px` play icon) | Built as `WaveCard` primitive in `#50`; **kept out of Library** | `#50` primitive; M10 Sets and transitions |
| Track table row selection checkbox | Board 1 table col 1 | `16 x 16 px` `--radius-sm: 4px`, `1px` `--color-border-control` | `TrackTable` multi-select | M3 Identify and tag |
| Row status / play icons (`▶`, `★`, `✓`) | Board 1 table col 2 | `14px` glyphs beside `#` | `TrackTable` status column | M2 Library (`▶`, `★`); M5 Analysis (`✓`) |
| Track table `#`, `Title`, `Artist`, `BPM`, `Key`, `Time` (+ `Genre`, `Format`) | Board 1 track table | `30px` rows (`--row-height`), `13px` `Inter` title/artist, `12px` `JetBrains Mono` numbers (`tabular-nums`), sticky `--color-surface` header | `TrackTable.svelte` | `#52` (sort carets M2) |
| Track table inline mini waveform | Board 1 table col 4; Board 2 `[08]`/`[09]` | `120px x 18px`, 1 px whole-pixel bars (`--color-text-2` peak, `--color-text` RMS) | `TrackTable` (M5), `Mix` `Next` list (M7) | M5 Analysis / M7 Decks |
| Track table `Energy` 5-bar meter | Board 1 table | `18 x 12 px` (`5 x 2px` bars, `2px` gap), active `--color-text`, inactive `--color-border` | `/design.html` (`#50`), `TrackTable` (M5) | `#50` primitive; M5 Analysis |
| Track table `Tension` 5-bar meter | Board 1 table | `18 x 12 px` 5-bar meter | Not a single-track property | Left out (transition property in M8/M10) |
| Track table row `...` button | Board 1 table last col | `24 x 24 px` quiet button | Track context menu | M2 / M3 / M4 |
| Active/selected table row frame | Board 1 row 4 (`Echo Chamber`) | `--radius-control: 6px`, `1px` `--color-border-control`, `2px` `--color-accent` left bar, `--color-raised` (`#252527`) / `--color-surface-2` (`#ECEAE7`) | `TableRow.svelte` | `#52` |
| Inspector panel frame & close `X` | Board 1 right column | `320px` wide, `--color-surface`, `1px` `--color-border` left, `28 x 28 px` `X` button (`I`) | Right shell column (`Inspector.svelte`) | `#51` frame; `#52` facts |
| Track artwork thumbnail (`64px`, `44px`, `36px`) | Board 1 inspector & dock; Board 2 `[07]`–`[09]` | `WaveThumb` (`--radius-panel: 8px` at `64px`/`44px`, `--radius-control: 6px` at `36px`, `1px` `--color-border`, deterministic monochrome dot wave) | Inspector, A to B dock, `Next` list | `#50` primitive; `#52` seeded fallback; M3 embedded art |
| Inspector track header (`Title`, `Artist`, `★`, `...`, status `DataTag`, format specs) | Board 1 inspector top | `16px` `Space Grotesk` title, `13px` `Inter` artist, `22px` `DataTag` (`Read` / `✓ Analysed`), `11px` mono specs | `Inspector.svelte` header | `#52` (`★` M2, `✓ Analysed` M5) |
| Inspector 2-row mono metadata grid | Board 1 inspector | `11px` mono uppercase `--color-muted` labels, `13px` `--color-text` values (`BPM`, `KEY`, `TIME`, `YEAR`, `GENRE`, `ALBUM`, `LABEL`, `LOCATION`) | `Inspector.svelte` grid (`SUBGENRE` left out; `RELEASE` -> `YEAR`, `ALBUM` added) | `#52` |
| Inspector `TAGS` section & removable chips | Board 1 inspector | `24px` chips (`--radius-control: 6px`, `--surface-2`, `1px` `--border`, `12px` `Inter`, `×` button) + `+` button | `Inspector.svelte` & NL search chips | `#50` chip primitive; M3 tags / M6 search |
| Inspector `AUDIO TRAITS` bars (`Energy`, `Brightness`, etc.) | Board 1 inspector | `20px` rows (`88px` `12px` `--text-2`, `36px` `12px` mono `--text`, `4px` track `--surface-2`, fill `--text`) | `/design.html` (`#50`), `Inspector.svelte` (`Tension`, `Danceability`, `Atmosphere` left out unless analysed) | `#50` primitive; M5 Analysis |
| `Energy` / `Tension` pills & curve plot | Board 1 inspector | `SegmentedTabs` (`Energy` active inverted pill) + flat 1 px plot + `10px` mono axis ticks (**no gradient fill**) | `/design.html` (`#50`), `Inspector.svelte` (`Tension` tab left out) | `#50` tabs; M5 Analysis |
| `TRANSITION SUGGESTIONS` (`Matches`) rows | Board 1 inspector bottom | `24px` `▶` button, `12px` `Inter` title/artist, `11px` mono compatibility line, `22px` `%` `DataTag`, `...` | `Inspector.svelte` matches | M6 Discovery / M10 Sets |
| `A -> B  Transition` dock frame (`Mix Point`, `Blend`, Deck A/B cards, crossfade slot) | Board 1 bottom dock | `32px` header + `104px` body on `--color-surface`; `--radius-panel: 8px` deck pods (`--surface-2` / `--background`, `28px` `A`/`B` badge, `44px` `WaveThumb`); `1px` `--color-border` | Shell bottom dock (`#51` selects Track A and names M2/M10) | `#51` frame; M2 preview; M10 crossfade |
| Bottom player bar (`▶`, `||`, time, scrub bar, `A Vol`, `B Vol`, `Curve`, `EQ`, headphone cue) | Board 1 bottom; Board 2 `[07]`–`[09]` | `48px` height, `--color-surface`, `32px` circular transport button (`--radius-pill`), `4px` scrub bar + `10px` thumb, `28px` `Curve`/`EQ` selects | `PlayerBar.svelte` (`#51` disabled until M2/M7; `Shuffle`/`Repeat`/`Fullscreen` left out) | `#50` scrub; `#51` frame; M2 preview; M7 decks |
| Brand palette swatches (`LIGHT MODE` / `DARK MODE`) | Board 2 `[03]` | All 11 `--color-*` tokens in `tokens.css` (AA-adjusted `Text 2`, `Muted`, `Border control`, `On accent`) | App-wide & `/design.html` | `#50` (existing M0 + `contrast.md`) |
| Type specimen (`Space Grotesk`, `Inter`, `JetBrains Mono`) | Board 2 `[04]` | `--text-2xs` (`10px`) to `--text-2xl` (`24px`), bundled `@fontsource-variable` | App-wide & `/design.html` | `#50` |
| Exploration symbols `B / WAVE`, `C / RESONANCE`, `E / SIGNAL` | Board 2 `[02]` | Exploratory vector glyphs | Not used as app icons or logos | Left out (only `A` is logo; `D` & `F` are micro elements) |
| `GRID DOTS` (`D / INDEX`) | Board 2 `[02]`, `[05]` | `1.5px` dots on `8 x 8 px` grid, `--color-border` on `--color-background` | Empty dock slots, `/design.html` | `#50` |
| `INDEX MARKS` (`001 | 002 | 003`) | Board 2 `[05]` | `11px` mono `tabular-nums` `--color-muted` + `1 x 10 px` `--color-border` ticks | `/design.html` (`#50`), timeline/phrase rulers | `#50` primitive; M5, M9, M10 |
| `FOCUS MARKER` (`F / FOCUS`) | Board 2 `[02]`, `[05]`, header, `[08]`/`[09]` | `6 x 6 px` L-brackets + optional `6 x 6 px` `+`, `1.5px` stroke, `--color-accent` or `--color-border-control` | `/design.html` (`#50`), `Mix` viewport (`#52`), hero/score cards | `#50` primitive; `#52` Mix; M8–M10 |
| `MODULAR DIVIDER` | Board 2 `[05]` | `12px` row: `24 x 2 px` bar, `1px` `--color-border` rule, `10px` mono `//`, three `2px` dots | `/design.html` (`#50`), major overview breaks | `#50` |
| `DATA TAG` (`INDEX 0427`, `SIMILARITY 0.87`, `AI`) | Board 2 `[05]` | `22px` height, `--radius-control: 6px`, `--surface-2`, `1px` `--border` (or `1px` `--accent`), `11px` mono `--color-text` | Inspector status, matches `%`, set metrics, `/design.html` | `#50` primitive; `#52`, M5, M6, M10 |
| Action button trio (`▶ Play`, `+ Add`, `... More`) | Board 2 `[07]` | `28px` height, `--radius-control: 6px`, `inverted` / `default` / `primary` variants | App-wide (`Button.svelte`), `/design.html` | `#50` |
| `SegmentedTabs` (`All \| Tracks \| Albums \| Artists`) | Board 2 `[07]` | `28px` `--surface-2` container, `--radius-control: 6px`, active `--color-text` fill / `--color-background` label | `/design.html` (`#50`), Inspector/Settings tabs (`Tracks/Albums/Artists` tabs left out) | `#50` |
| Vertical icon strip (`♡`, `+`, `↓`, `shuffle`, `repeat`, `bars`) | Board 2 `[07]` | Lucide `1.5px` stroke outline icons (`16px`–`18px`) | `Icon.svelte` (`shuffle` and `repeat` left out) | `#50` |
| Bracketed `StatCard` (`Taste Profile`, bar plot, `0.87`) | Board 2 `[07]` | `--radius-panel: 8px`, `--color-surface`, `1px` `--color-border`, `--color-accent` corner brackets, `20px` `Space Grotesk` stat | `/design.html` (`#50`), transition & mix score cards (`Taste Profile` title left out) | `#50` primitive; M8 Coach / M9 Replay |
| Hero 3D dot-wave banner, `Refine` button & 5-column mono strip (`ENERGY` 5-dot meter) | Board 2 `[08]`/`[09]` | `Wave.svelte` header, `36px` `primary` + `default` buttons, `11px` mono labels + `15px` values + `6px` 5-dot meter | `Wave` (`M0`), 5-dot meter (`#50`); adapted for `Sets` header (`Generate`/`Refine`; `For You`/`Discovery Mix`/`Mood` left out) | `#50` meter; M10 Sets and transitions |
| `Next in Mix` rows | Board 2 `[08]`/`[09]` | `36 x 36 px` `WaveThumb`, `13px` title, `12px` artist, 1 px mini waveform, `12px` mono time, `♡`, `...` | `Mix` `Next` panel & `Inspector` matches | M6 Discovery / M7 Decks |
| `Listening Insight` panel | Board 2 `[08]`/`[09]` | `--radius-panel: 8px`, `--color-surface`, `1px` `--color-border`, bar icon, `13px` `Inter` text | Adapted as `Replay` transition takeaway card (`"Listening Insight"` copy left out) | M9 Review |
