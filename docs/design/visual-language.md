# Visual Language

Resonance looks like a monochrome hardware audio instrument: carbon-black and graphite surfaces in dark mode, warm stone in light mode, hairline `1px` borders, tabular monospace figures, and a generative WebGL2 dot-wave field (`Wave`) that sits behind headers, at the bottom of the sidebar, and inside collection cards.

Two concept boards in `handoff/` define the visual target ([ADR 0012](../adr/0012-concept-boards-as-design-target.md)):

- `concept-dark.png` (`1586 x 992` CSS pixels at device pixel ratio `1`) leads. Every region dimension, font size, corner radius, control height and row pitch below is measured directly from it.
- `concept are and design 2.png` (`1448 x 1086` CSS pixels) defines the 7-bar signal mark, the font stack, the micro elements (`[05]`), the reusable UI components (`[07]`), the colour palette (`[08]`), and the warm-stone light mode variant (`[09]`).

All tokens live in `ui/src/lib/design/tokens.css` under `:root` (dark default), `:root[data-theme='light']` (light mode), and `@media (prefers-contrast: more)`.

**Zero raw values in components:** No component in `ui/src/` may contain hard-coded pixel sizes, hex colours, font names, border radii or transition durations. Every style rule in a component references the CSS custom properties defined here (see [components.md](components.md)).

## Colour tokens

The interface is strictly monochrome except for a single orange signal accent (`--color-accent`) reserved for four places: the active `2px` selection mark (on the active sidebar space and selected table row), the active playhead and cue marker, the live recording indicator, and destructive action confirmation.

All multi-colour status dots, tag tints and gradient fills shown on the concept boards are rendered in monochrome using `--color-text`, `--color-text-2`, `--color-muted` and `--color-border-control`.

| Token | Dark (`light-dark` second value) | Light (`light-dark` first value) | Role |
| --- | --- | --- | --- |
| `--color-background` | `#0B0B0C` | `#F8F7F5` | Base window canvas (`#0B0B0C` in Board 2 `[08]`; warm stone in `[09]`) |
| `--color-surface` | `#141415` | `#FFFFFF` | Sidebar, top bar, command bar, inspector, Deck A/B cards, player card (`#141415` in `[08]`) |
| `--color-surface-2` | `#1E1E1F` | `#ECEAE7` | Sunken wells, inactive pills, slash chips, tag chips, scrub tracks (`#1C1C1E`/`#1E1E1F` in `[08]`) |
| `--color-raised` | `#252527` | `#FFFFFF` | Selected table row fill, active sidebar space fill, floating popovers, hover states |
| `--color-border` | `#343437` | `#D8D4CF` | Structural `1px` hairlines between regions, card borders, table row dividers |
| `--color-border-control` | `#6E6E71` | `#8A8681` | Interactive control outlines, checkboxes, selected row frame (`>= 3:1` against all four surfaces; see [contrast.md](contrast.md)) |
| `--color-text` | `#F2F1EF` | `#171717` | Primary text, active icons, inverted pill fills, waveform peaks |
| `--color-text-2` | `#A9A8A6` | `#595754` | Secondary text: artists, column headers, inactive nav items, units |
| `--color-muted` | `#8C8C8C` | `#6C6965` | Tertiary metadata keys, index marks, disabled text, empty states (moved from `#777777` / `#8A8783` so all text meets WCAG AA `>= 4.5:1`; see [contrast.md](contrast.md)) |
| `--color-accent` | `#E4553D` | `#D94F3B` | Signal orange (`#E4553D` in `[08]`; `#D94F3B` in light mode to pass `>= 3.4:1` on all light surfaces) |
| `--color-on-accent` | `#0B0B0C` | `#0B0B0C` | Text or icon glyph placed on top of a solid `--color-accent` fill (`5.31:1` dark, `4.81:1` light) |

### More contrast and forced high contrast

In `ui/src/lib/design/tokens.css`, every colour is declared once with `light-dark(light, dark)`:

- When `@media (prefers-contrast: more)` matches, dividers take the control border's tone (`--color-border: light-dark(#8a8681, #6e6e71)`) and muted text takes Text 2's (`--color-muted: light-dark(#595754, #a9a8a6)`).
- When `@media (forced-colors: active)` matches (Windows High Contrast), system colours replace the palette (`--color-accent: Highlight`, `--color-on-accent: HighlightText`, `--color-border: CanvasText`, `--color-border-control: CanvasText`).

## Typography

Board 2 (`[02] Typography`) and `ui/src/lib/design/tokens.css` specify three variable font families, with aliases so both `--font-heading` / `--font-display` and `--font-ui` / `--font-sans` resolve cleanly:

```css
--font-heading: 'Space Grotesk Variable', sans-serif;
--font-display: var(--font-heading);
--font-ui: 'Inter Variable', sans-serif;
--font-sans: var(--font-ui);
--font-mono: 'JetBrains Mono Variable', monospace;
```

- **Display (`--font-display`, Space Grotesk, weights 500 and 600):** Wordmark (`RESONANCE`), workspace heading (`Library`), inspector track title (`Echo Chamber`), Deck A/B card titles and badges (`A`, `B`, `A -> B`), and collection card titles.
- **UI Sans (`--font-sans`, Inter, weights 400 and 500):** Sidebar navigation items, command bar input text, table track titles and artists, filter pills, tag chips, audio trait labels, and buttons.
- **Technical Mono (`--font-mono`, JetBrains Mono, weights 400 and 500):** Always paired with `font-variant-numeric: tabular-nums`. Used for BPM (`128.0`), musical key (`8A` / `A Minor (8A)`), durations and timestamps (`07:02`, `4:15 / 7:02`), track counts (`12,846`), row numbers, slash-command badges (`/ bpm 125-132`), keyboard shortcut badges (`⌘ K` / `Ctrl K`), match percentages (`95%`), audio specs (`WAV  44.1 kHz  24-bit`), and all uppercase section and metadata labels (`MonoLabel`).

### Type scale (measured from `concept-dark.png` at `1586 x 992`)

Every step in the type scale is measured from the pixel cap heights in `concept-dark.png` and `concept are and design 2.png`. **Text never scales with window width:** these CSS pixel sizes stay constant from `1280 x 720` to `5120 x 2160`.

| Token | Size | Cap height in image | Line height | Tracking | Typical font & weight | Where it appears |
| --- | --- | --- | --- | --- | --- | --- |
| `--text-2xs` | `10px` (`0.625rem`) | `7 px` | `1.2` | `0.10em` | `--font-mono` 500 uppercase | Bracketed index marks (`[01]`), provenance badges (`Provenance`), Board 2 micro captions |
| `--text-xs` | `11px` (`0.6875rem`) | `8 px` | `1.3` | `0.08em` (labels) / `0` (data) | `--font-mono` 400–500 | `MonoLabel` headers (`MUSIC INTELLIGENCE FOR DJS`, `ACTIVE SET`, `BPM`, `KEY`, `TIME`, `RELEASE`, `GENRE`, `SUBGENRE`, `LABEL`, `TAGS`, `AUDIO TRAITS`, `TRANSITION SUGGESTIONS`), keybadge (`⌘ K`), slash-command code pills (`/ bpm 125-132`), status pill (`✓ Analysed`), curve timestamps (`0:00..7:02`), match badge (`95%`), Deck A/B pod metadata (`128.0 BPM · A min · 7:02`) |
| `--text-sm` | `12px` (`0.75rem`) | `8.5–9 px` | `1.35` | `0` | `--font-sans` 400–500 / `--font-mono` 400 | Table column headers (`#`, `Title`, `Artist`, `BPM`, `Key`, `Energy`, `Time`), sidebar collection items and track counts (`12,846`, `248`), slash-command descriptions (`Filter by tempo`), filter pills (`All Tracks`, `✓ Analysed`), inspector audio specs (`WAV  44.1 kHz  24-bit`), removable tag chips (`Hypnotic ×`), audio trait labels and values (`0.82`), Deck A/B artist names (`Kaden`), dock `Mix Point` and `Blend` controls, player bar time (`4:15 / 7:02`) and control labels (`A Vol`, `B Vol`, `Curve`, `EQ`) |
| `--text-md` | `13px` (`0.8125rem`) | `9.5–10 px` | `1.4` | `0` | `--font-sans` 400–500 / `--font-mono` 400 | Table body cells (`Title`, `Artist`, `BPM`, `Key`, `Time`), workspace track count (`12,846 tracks`), sidebar active set row (`Fri 23:00 — Warehouse`), inspector metadata values (`128.0`, `A Minor (8A)`, `07:02`, `2026`, `Techno`, `Ostgut Ton`), transition suggestion track titles (`Luminous — Vera Holt`), dock `Transition` label |
| `--text-base` | `14px` (`0.875rem`) | `10 px` | `1.4` | `-0.005em` | `--font-sans` 400–500 / `--font-display` 500 | Command bar input text and placeholder (`Search your library, or type a command...`), primary sidebar space labels (`Library`, `Prep`, `Sets`, `Mix`, `Replay`), inspector artist subtitle (`Kaden`), Deck A/B pod track titles (`Echo Chamber`, `Luminous`), Sets collection card titles (`Peak Time`) |
| `--text-lg` | `16px` (`1rem`) | `12 px` | `1.25` | `0.32em` (wordmark) / `-0.01em` (headings) | `--font-display` 500–600 | Top bar `RESONANCE` wordmark (`133 x 13 px` at `x = 133..265, y = 19..31`), dock `A -> B` transition title (`17 px` in image, tokenised as `--text-lg: 16px`), modal titles |
| `--text-xl` | `20px` (`1.25rem`) | `15 px` | `1.2` | `-0.02em` | `--font-display` 600 | Inspector track title (`Echo Chamber` at `x = 1289, y = 79..93`), Mix deck primary track title |
| `--text-2xl` | `27px` (`1.6875rem`) | `20 px` | `1.15` | `-0.02em` | `--font-display` 500 | Workspace title (`Library` at `x = 278..363, y = 178..204`) |

## Spacing, radii and dimension tokens

### Spacing scale

```css
--space-0-5: 2px;
--space-1:   4px;
--space-1-5: 6px;
--space-2:   8px;
--space-2-5: 10px;
--space-3:   12px;
--space-3-5: 14px;
--space-4:   16px;
--space-5:   20px;
--space-6:   24px;
--space-8:   32px;
```

### Corner radii (measured from `concept-dark.png`)

Where the earlier brief used `4 px` and `6 px` corners, `concept-dark.png` uses a six-step hierarchy from `3 px` checkboxes up to `12 px` on the floating player card:

| Token | Value | Measured elements in `concept-dark.png` and Board 2 |
| --- | --- | --- |
| `--radius-xs` | `3px` | Table row checkboxes (`16 x 16 px`), inspector trait bar tracks (`218 x 5 px`), transition suggestion `▶` icon boxes (`24 x 24 px`) |
| `--radius-sm` | `6px` | Alias `--radius-control`. Selected table row frame (`884 x 34 px`), standard buttons, `⌘ K` keybadge (`46 x 24 px`), slash-command chips (`22 px` tall), removable tag chips (`26 px` tall), workspace tool buttons (`34 x 34 px`), Deck `A` / `B` badges (`35 x 34 px`), player `Curve` and `EQ` selects, headphone cue button (`36 x 28 px`) |
| `--radius-md` | `8px` | Active sidebar space button (`232 x 38 px`), Deck A/B artwork thumbnail (`60 x 60 px`), Board 2 `[07]` segmented tab bar (`485 x 38 px`) and compact track card (`485 x 56 px`) |
| `--radius-lg` | `10px` | Alias `--radius-panel`. Search and command input box (`855 x 42 px`), inspector artwork thumbnail (`96 x 96 px`), Deck A and Deck B cards (`340 x 87 px`), Sets collection cards (`181 x 82 px` and `101 x 81 px`) |
| `--radius-xl` | `12px` | Bottom player bar card (`1548 x 60 px`, inset `20 px` from window edges), modal sheets |
| `--radius-pill` | `9999px` | Workspace filter pills (`All Tracks`, `25 px` tall), inspector `✓ Analysed` status pill (`78 x 22 px`), inspector `Energy` curve pill (`70 x 23 px`), circular `+` tag button (`26 x 26 px`), player circular play button (`44 x 44 px`), Board 2 `[05]`/`[07]` data tags |

### Control, row, thumbnail and region tokens

```css
/* Control heights */
--control-height-xs:  22px;  /* Slash-command code badge, ✓ Analysed status pill, 95% match badge */
--control-height-sm:  26px;  /* Filter pills, removable tag chips, circular + tag button, player Curve/EQ selects */
--control-height:     28px;  /* Compact buttons, top-bar icon buttons, player pause & headphone buttons */
--control-height-md:  34px;  /* Workspace header tool buttons (34x34), Deck A/B badges (35x34) */
--control-height-nav: 38px;  /* Sidebar primary space buttons (232x38) and Board 2 segmented tab bar */
--control-height-lg:  42px;  /* Search and command input box (855x42) */
--control-circle-lg:  44px;  /* PlayerBar primary play circle button (44x44) */

/* Table dimensions */
--table-header-height: 31px; /* TrackTable header row (y = 393..424) */
--row-height:          33px; /* TrackTable data row pitch (10 rows across y = 424..753 = 32.9 px) */

/* Artwork thumbnails (Thumb.svelte) */
--thumb-sm: 36px;            /* Compact track cards in Prep / Board 2 [07] */
--thumb-md: 60px;            /* Deck A and Deck B cards in TransitionDock (x = 82..142, y = 821..881) */
--thumb-lg: 96px;            /* Inspector header artwork (x = 1175..1271, y = 65..160) */

/* Shell region dimensions at 1586 x 992 reference window */
--topbar-height:           50px;  /* y = 0..50 */
--sidebar-width:           256px; /* x = 0..256 */
--sidebar-collapsed-width: 52px;  /* Collapsed icon rail below 1360px or when toggled */
--sidebar-wave-height:     78px;  /* Bottom dot wave in sidebar (y = 655..732) */
--inspector-width:         420px; /* x = 1166..1585 */
--inspector-min-width:     340px; /* Narrowed docked width on 1280..1439px windows */
--inspector-max-width:     440px; /* Maximum width cap on 2560px+ monitors */
--dock-header-height:      52px;  /* y = 754..806 */
--dock-card-width:         340px; /* Deck A (339px) and Deck B (340px) */
--dock-card-height:        87px;  /* y = 807..893 */
--dock-waveform-height:    101px; /* y = 801..901 */
--player-height:           60px;  /* y = 915..974 */
--player-inset-x:          20px;  /* Left (20px) and right (19px) inset from window edges */
--player-inset-bottom:     17px;  /* Bottom inset (y = 974..991) */
```

### Focus, elevation and motion tokens

```css
--border-width: 1px;
--focus-ring-width: 2px;
--focus-ring-offset: -2px;
--shadow-popover: 0 12px 32px rgba(0, 0, 0, 0.48);
--duration-fast: 80ms;
--duration-normal: 140ms;
```

In light mode, `--shadow-popover` is `0 8px 24px rgba(20, 20, 19, 0.10)`. Under `@media (prefers-reduced-motion: reduce)`, `--duration-fast` and `--duration-normal` resolve to `0ms`.

## Brand mark (`[01]` in Board 2 and `TopBar` in `concept-dark.png`)

The Resonance brand mark (`Mark.svelte`) is a 7-bar asymmetric vertical signal motif (`2px` bar width, `2px` gap, pill caps `1px` radius) whose bar heights follow a rising-and-falling envelope (`35%, 60%, 85%, 100%, 70%, 45%, 25%` of container height).

- In `TopBar`, `Mark` renders at `19 x 21 px` (`x = 97..115, y = 15..35` in `concept-dark.png`, or `20 px` left padding in standard alignment) in `--color-text`, followed `18 px` to the right by `Wordmark` (`RESONANCE` in `Space Grotesk` 500, `--text-lg: 16px`, `letter-spacing: 0.32em`, `--color-text`).
- Below the top bar, at the top of `Sidebar` (`x = 25, y = 75..97`), a two-line `MonoLabel` reads `MUSIC INTELLIGENCE / FOR DJS` (`--font-mono`, `--text-xs: 11px`, uppercase, `letter-spacing: 0.08em`, `line-height: 15px`, `--color-text-2`).

## Micro elements (`[05]` in Board 2)

Board 2 (`[05] Micro Elements`) defines six technical graphic primitives in `ui/src/lib/design/` that give the interface its hardware-instrument character:

1. **Uppercase monospace labels (`MonoLabel.svelte`):** `--font-mono`, `--text-xs: 11px` (or `--text-2xs: 10px`), `font-weight: 500`, `text-transform: uppercase`, `letter-spacing: 0.08em`, `--color-muted` (or `--color-text-2` for prominent headers). Supports an optional trailing `ⓘ` info icon (`12 px`) and right-aligned action link (`View All ->`).
2. **Focus brackets (`FocusBrackets.svelte`):** Four `1px` L-shaped corner ticks (`8px` arm length, `--color-text` when focused or `--color-accent` when armed) that frame an active drop target, deck header, or keyboard-focused panel (`155 x 70 px` specimen in Board 2 `[05]`).
3. **Index marks (`IndexMarks.svelte`):** Two-digit zero-padded monospace indices in square brackets (`[01]`, `[02]`, `[03]`), `--font-mono`, `--text-2xs: 10px`, `--color-muted`. Used on `/design.html` specimen sections, inspector diagnostics, and set cue lists.
4. **Data tags (`DataTag.svelte`):** Compact metadata chips in five variants (`pill`, `rect`, `removable`, `score`, `keybadge`):
   - `pill`: `height: var(--control-height-xs)` (`22px`), `padding: 0 var(--space-2-5)` (`0 10px`), `border-radius: var(--radius-pill)`, `background: var(--color-surface-2)`, `border: 1px solid var(--color-border)`, `--font-mono`, `--text-xs: 11px`, `--color-text`. Used for inspector `✓ Analysed` (`78 x 22 px`) and Board 2 `[05]`/`[07]` tags (`128.0 BPM`, `8A`, `WAV 24-bit`, `-7.4 LUFS`).
   - `removable`: `height: var(--control-height-sm)` (`26px`), `padding: 0 var(--space-2-5)`, `border-radius: var(--radius-sm)` (`6px`), `background: var(--color-surface-2)`, `border: 1px solid var(--color-border)`, `--font-sans`, `--text-sm: 12px`, `--color-text` with a `12px` `×` button (`--color-muted`). Used for inspector `TAGS` (`Hypnotic ×`, `Peak Time ×`, `Industrial ×`, `Rolling Bass ×`).
   - `score`: `height: var(--control-height-xs)` (`22px`), `min-width: 43px`, `border-radius: var(--radius-sm)` (`6px`), `background: var(--color-surface-2)`, `border: 1px solid var(--color-border)`, `--font-mono`, `--text-xs: 11px`, `--color-text`. Used for transition match scores (`95%`, `92%`, `89%`).
   - `keybadge`: `height: 24px`, `padding: 0 var(--space-2)`, `border-radius: var(--radius-sm)` (`6px`), `background: var(--color-surface-2)`, `border: 1px solid var(--color-border)`, `--font-mono`, `--text-xs: 11px`, `--color-text-2`. Used for `⌘ K` / `Ctrl K` (`46 x 24 px`) inside `CommandBar`.
5. **Modular dividers (`ModularDivider.svelte`):** `1px` solid `--color-border` horizontal or vertical rules that separate regions and inspector sections (`15–20 px` horizontal inset inside `Inspector`), optionally terminating in `4px` perpendicular tick marks or carrying an inline `MonoLabel`.
6. **Grid dots (`GridDots.svelte`):** A subtle `1px` dot matrix (`16px` or `24px` spacing, `--color-border`) rendered on empty workspace drop zones, the `Sets` timeline bed, and `/design.html` specimen backgrounds.

## Reusable UI components (`[07]` in Board 2 and `concept-dark.png`)

Full prop signatures, file paths and usage sites for every component are in [components.md](components.md). Their measured visual specifications are:

- **Slash-command chip (`SlashChip.svelte`):** Code badge (`height: var(--control-height-xs)` `22px`, `padding: 0 var(--space-2)` `8px`, `border-radius: var(--radius-sm)` `6px`, `background: var(--color-surface-2)`, `border: 1px solid var(--color-border)`, `--font-mono`, `--text-xs: 11px`, `--color-text`) followed at a `10px` gap (`--space-2-5`) by a plain description (`--font-sans`, `--text-sm: 12px`, `--color-text-2`).
- **Filter pill (`FilterPill.svelte`):** `height: var(--control-height-sm)` (`26px`), `padding: 0 var(--space-3)` (`0 12px`), `border-radius: var(--radius-pill)`, `--font-sans`, `--text-sm: 12px`, `font-weight: 500`.
  - Active (`All Tracks`, `83 x 25 px`; inspector `Energy`, `70 x 23 px`): inverted high-contrast fill `background: var(--color-text)` (`#F2F1EF` dark / `#141413` light) with `color: var(--color-background)` (`#0B0B0C` dark / `#FAF9F7` light).
  - Inactive (`✓ Analysed`, `○ Unanalysed`, `★ Favourites`, `◷ Recent`): transparent background, `color: var(--color-text-2)`, `13px` monochrome leading icon, `24px` (`--space-6`) horizontal spacing between pills.
- **Segmented tabs (`SegmentedTabs.svelte`, Board 2 `[07]`):** Outer container `height: var(--control-height-nav)` (`38px`, `y = 982..1020` in Board 2), `padding: 3px`, `background: var(--color-surface-2)`, `border: 1px solid var(--color-border)`, `border-radius: var(--radius-md)` (`8px`). Active segment has `height: 32px`, `background: var(--color-raised)`, `border: 1px solid var(--color-border-control)`, `border-radius: var(--radius-sm)` (`6px`), `color: var(--color-text)`, `--font-sans`, `--text-sm: 12px`, `font-weight: 500`.
- **Artwork thumbnail (`Thumb.svelte`):** Square frame with `1px solid var(--color-border)`, overflow hidden, rendering album cover art or a deterministic `Wave` dot-mesh fallback seeded from the track ID:
  - `size="sm"`: `36 x 36 px` (`--thumb-sm`), `border-radius: var(--radius-sm)` (`6px`).
  - `size="md"`: `60 x 60 px` (`--thumb-md`), `border-radius: var(--radius-md)` (`8px`), used in Deck A and Deck B pods (`x = 82..142, y = 821..881`).
  - `size="lg"`: `96 x 96 px` (`--thumb-lg`), `border-radius: var(--radius-lg)` (`10px`), used in the `Inspector` header (`x = 1175..1271, y = 65..160`).
- **5-bar vertical energy meter (`MeterBar.svelte`):** `27 x 12 px` (`x = 908..934` in `TrackTable`), five vertical bars (`3px` wide, `3px` gap, `1px` radius) with ascending heights (`4px, 6px, 8px, 10px, 12px`). Active bars (`1..value`) use `--color-text`; inactive bars use `--color-border-control` at `35%` opacity (`--color-surface-2` with `1px` border in high contrast).
- **4-dot stem level indicator (`LevelDots.svelte`, Board 2 `[07]`):** Monospace stem label (`DRUMS`, `BASS`, `VOCALS`, `OTHER`, `--text-xs: 11px`, `--color-text-2`) beside four `4px` circular dots (`2px` gap). Active dots use `--color-text`; inactive dots use `--color-border`.
- **Audio trait progress bar (`TraitBar.svelte`):** `17px` row pitch in `Inspector` (`y = 350, 367, 383, 400, 416`): left label (`135px` column, `--font-sans`, `--text-sm: 12px`, `--color-text-2`), tabular numeric value (`32px` column, `--font-mono`, `--text-sm: 12px`, `--color-text`), and a `218 x 5 px` horizontal track (`border-radius: var(--radius-xs)` `3px`, `background: var(--color-surface-2)`) with a `--color-text` fill bar (`width: calc(value * 100%)`).
- **Scrub and volume slider (`ScrubBar.svelte`):**
  - `size="md"` (main player scrub bar, `387 px` wide at reference viewport, `x = 269..655, y = 943..946`, and Board 2 `[07]` `260 px` scrub bar): `4px` tall track (`border-radius: 2px`, `background: var(--color-surface-2)`), elapsed fill in `--color-text`, and a `12 x 12 px` circular thumb (`background: var(--color-text)`, `border: 2px solid var(--color-surface)`).
  - `size="sm"` (`A Vol` and `B Vol` sliders, `86 px` wide, `x = 802..887` and `1023..1108`): `4px` tall track, elapsed fill in `--color-text`, and a `10 x 10 px` circular thumb.

## Dot-wave field (`Wave.svelte`) and 1 px waveforms (`Waveform.svelte`)

- **Generative dot-wave field (`ui/src/lib/wave/Wave.svelte`, Board 2 `[04]`):** WebGL2 point-sprite mesh drawing `1.5–2.5 px` monochrome particles (`#F2F1EF` in dark mode, `#141413` in light mode, depth-attenuated by alpha) arranged on a perspective grid displaced by low-frequency simplex/harmonic waves. Runs in four placements:
  1. **Sidebar footer (`variant="sidebar"`):** `254 x 78 px` (`x = 1..254, y = 655..732` in `concept-dark.png`) at the bottom of `Sidebar`, fading smoothly into `--color-surface` above.
  2. **Ambient workspace backdrop (`variant="ambient"`):** Low-opacity (`0.12` dark / `0.07` light) field behind workspace headers.
  3. **Collection card backdrop (`variant="card"`):** Inside the `181 x 82 px` collection cards in `Sets` (M10).
  4. **Artwork fallback (`variant="thumb"`):** Static single-frame render inside `Thumb.svelte` when a track has no embedded cover art.
  Full shader and performance rules (`< 1 ms` UI thread time per frame, static frame when `prefers-reduced-motion: reduce` is active or when `Mix` is live) are in [wave.md](wave.md).
- **1 px bar waveforms (`ui/src/lib/mix/Waveform.svelte`):** All track waveforms render as crisp `1px` CSS vertical bars (`Math.max(1, Math.round(devicePixelRatio))` device pixels wide, `1px` gap) snapped to integer device pixels:
  - `mode="inline"` (`TrackTable` `Waveform` column, `140 x 17 px` at `x = 502..641`): unplayed bars in `--color-text-2`, played bars in `--color-text`.
  - `mode="crossfade"` (`TransitionDock` centre canvas, `844 x 101 px` at `x = 371..1214, y = 801..901`): outgoing Deck A bars and incoming Deck B bars overlaid with two monochrome `1.5px` S-curve volume envelopes and a `1px` vertical playhead line (`x = 856`) topped with a `5 x 5 px` square cap (`--color-accent` when actively playing/scrubbing, `--color-text` at rest).
  - `mode="deck"` (`Mix` view scrolling deck waveforms): WebGL2 `1px` bar renderer locked to the audio clock at the display refresh rate (`<= 8 ms` p99 frame time at 120 Hz).
  - `mode="curve"` (`Inspector` `Energy` plot, `385 x 67 px` at `x = 1185..1569, y = 486..552`): flat `1px` monochrome bar/step plot over a `1px` `--color-border` baseline at `y = 552`, with no gradient area fill.

## Light mode (`[09]` in Board 2)

Board 2 (`[09] Light Mode Variant`) shows a warm-stone daylight theme (`#F0EDEB` / `#F4F3F1` background, `#FAF9F7` cards, `#141413` ink text, `#DCDAD6` hairline borders). Light mode changes only colour tokens (`:root[data-theme='light']`):

- Every dimension, font size, corner radius, border width (`1px`) and layout grid rule is identical to dark mode.
- Inverted active pills (`FilterPill` active state) swap cleanly to `#141413` fill (`--color-text`) with `#FAF9F7` text (`--color-background`).
- `Wave` and `Waveform` read `--color-text`, `--color-text-2` and `--color-accent` from CSS custom properties on theme change and redraw with dark ink particles and bars on the warm-stone canvas.
