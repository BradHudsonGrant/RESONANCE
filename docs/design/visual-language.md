# Visual language

Resonance looks like a piece of studio hardware: graphite surfaces in the dark theme, warm stone in the light one, 1 px borders, tabular figures, one orange accent, and the dot wave as its art. `concept-dark.png` sets the sizes and density, and `concept are and design 2.png` sets the mark, the type, the palette, the small graphic elements and the light theme ([ADR 0012](../adr/0012-concept-boards-as-design-target.md)).

Every value lives once in `ui/src/lib/design/tokens.css`. Components use the tokens and never a raw size, colour, font, radius or duration ([components.md](components.md)).

## Colour

| Token                    | Dark      | Light     | Used for                                                              |
| ------------------------ | --------- | --------- | --------------------------------------------------------------------- |
| `--color-background`     | `#0B0B0C` | `#F8F7F5` | The window                                                            |
| `--color-surface`        | `#141415` | `#FFFFFF` | Top bar, sidebar, command bar, inspector, deck cards, player bar      |
| `--color-surface-2`      | `#1E1E1F` | `#ECEAE7` | Wells, inactive pills, chips, tags, scrub tracks                      |
| `--color-raised`         | `#252527` | `#FFFFFF` | The selected row, the active space, popovers, hover                   |
| `--color-border`         | `#343437` | `#D8D4CF` | Dividers between areas, card edges, row lines                         |
| `--color-border-control` | `#6E6E71` | `#8A8681` | The edge of a button, field or checkbox, and the selected row's frame |
| `--color-text`           | `#F2F1EF` | `#171717` | Text, active icons, inverted pills, waveform peaks                    |
| `--color-text-2`         | `#A9A8A6` | `#595754` | Artists, column headers, inactive items, units                        |
| `--color-muted`          | `#8C8C8C` | `#6C6965` | Metadata keys, index marks, disabled text, empty states               |
| `--color-accent`         | `#E4553D` | `#D94F3B` | See below                                                             |
| `--color-on-accent`      | `#0B0B0C` | `#0B0B0C` | Text on an accent fill                                                |

Every colour is written once as `light-dark(light, dark)`. The page follows the system theme, and `data-theme` on an element overrides it for that element and everything inside it.

The interface is monochrome. The accent marks the playhead and cue markers, the 2 px selection mark on the active space and the selected row, the recording indicator, a destructive confirm, and the one thing that needs action. It is never used for text on a surface, because it falls short of 4.5 to 1 there. The coloured status dots and badges on the boards are drawn in the monochrome tones.

Every text pair meets WCAG AA and every mark meets 3 to 1. [contrast.md](contrast.md) has the ratios and the tones that moved to get there. Border divides areas. Control border is the edge of a button or a field, where the edge has to be seen.

With more contrast asked for, dividers take the control border's tone and muted text takes Text 2's. Under Windows high contrast the system colours replace the palette, and the accent becomes `Highlight` with `HighlightText` on it.

## Type

Three variable fonts, all bundled with the app. Nothing loads from a CDN.

| Token                                    | Font           | Weights  | Used for                                                                                                             |
| ---------------------------------------- | -------------- | -------- | -------------------------------------------------------------------------------------------------------------------- |
| `--font-display`, alias `--font-heading` | Space Grotesk  | 500, 600 | The wordmark, space titles, the inspector title, deck titles and badges                                              |
| `--font-sans`, alias `--font-ui`         | Inter          | 400, 500 | Navigation, the command bar, table titles and artists, pills, tags, buttons                                          |
| `--font-mono`                            | JetBrains Mono | 400, 500 | BPM, key, times, counts, row numbers, slash commands, shortcut badges, scores, audio specs and every uppercase label |

Mono always uses `font-variant-numeric: tabular-nums`.

| Token         | Size | Line height | Tracking                                    | Used for                                                                                                          |
| ------------- | ---- | ----------- | ------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| `--text-2xs`  | 10px | 1.2         | 0.1em                                       | Index marks, provenance badges                                                                                    |
| `--text-xs`   | 11px | 1.3         | 0.08em on labels, 0 on data                 | Mono labels, shortcut and slash badges, pills, scores, deck stats                                                 |
| `--text-sm`   | 12px | 1.35        | 0                                           | Table headers, sidebar collections and counts, chip descriptions, filter pills, tags, trait rows, player readouts |
| `--text-md`   | 13px | 1.4         | 0                                           | Table cells, the track count, inspector values, suggestion titles                                                 |
| `--text-base` | 14px | 1.4         | -0.005em                                    | The command bar, space labels, the inspector artist, deck titles                                                  |
| `--text-lg`   | 16px | 1.25        | 0.32em on the wordmark, -0.01em on headings | The wordmark, the dock's A to B title, sheet titles                                                               |
| `--text-xl`   | 20px | 1.2         | -0.02em                                     | The inspector title, Mix deck titles                                                                              |
| `--text-2xl`  | 27px | 1.15        | -0.02em                                     | Space titles such as Library                                                                                      |

The line heights are `--leading-tight` (1.15), `--leading-snug` (1.2), `--leading-heading` (1.25), `--leading-mono` (1.3), `--leading-compact` (1.35) and `--leading-normal` (1.4). The tracking tokens are `--tracking-tight` (-0.02em), `--tracking-heading` (-0.01em), `--tracking-base` (-0.005em), `--tracking-normal` (0), `--tracking-mono` (0.08em), `--tracking-mono-wide` (0.1em) and `--tracking-wordmark` (0.32em). The weights are `--weight-regular` (400), `--weight-medium` (500) and `--weight-semibold` (600).

## Space and corners

Spacing is on a 4 px scale with half steps: `--space-0-5` 2px, `--space-1` 4px, `--space-1-5` 6px, `--space-2` 8px, `--space-2-5` 10px, `--space-3` 12px, `--space-3-5` 14px, `--space-4` 16px, `--space-5` 20px, `--space-6` 24px, `--space-8` 32px.

Corners follow `concept-dark.png`, which uses larger radii than the brief's 4 and 6 px:

| Token                                   | Value  | Used for                                                                    |
| --------------------------------------- | ------ | --------------------------------------------------------------------------- |
| `--radius-xs`                           | 3px    | Checkboxes, trait bar tracks, small play buttons                            |
| `--radius-sm`, alias `--radius-control` | 6px    | Buttons, the selected row frame, badges, chips, tags, deck letters, selects |
| `--radius-md`                           | 8px    | The active space, deck artwork, segmented tabs, compact track cards         |
| `--radius-lg`, alias `--radius-panel`   | 10px   | The search box, inspector artwork, deck cards, collection cards             |
| `--radius-xl`                           | 12px   | The player bar, sheets                                                      |
| `--radius-pill`                         | 9999px | Filter pills, the status pill, circular buttons                             |

## Sizes

Controls:

| Token                       | Value | Used for                                               |
| --------------------------- | ----- | ------------------------------------------------------ |
| `--control-height-xs`       | 22px  | Slash badges, the status pill, score badges            |
| `--control-height-keybadge` | 24px  | The Ctrl K badge                                       |
| `--control-height-sm`       | 26px  | Filter pills, tags, the add-tag button, player selects |
| `--control-height`          | 28px  | Compact and icon buttons                               |
| `--control-height-md`       | 34px  | Workspace tool buttons                                 |
| `--control-height-nav`      | 38px  | Sidebar spaces, segmented tabs                         |
| `--control-height-lg`       | 42px  | The search box                                         |
| `--control-circle-lg`       | 44px  | The main play button                                   |

Table and artwork: `--table-header-height` 31px, `--row-height` 33px, `--checkbox-size` 16px, `--thumb-sm` 36px, `--thumb-md` 60px, `--thumb-lg` 96px.

Regions at the reference size, with the rules for other sizes in [ux.md](ux.md#window-sizes):

| Token                         | Value  |
| ----------------------------- | ------ |
| `--topbar-height`             | 50px   |
| `--sidebar-width`             | 256px  |
| `--sidebar-collapsed-width`   | 52px   |
| `--sidebar-wave-height`       | 78px   |
| `--sidebar-collection-indent` | 36px   |
| `--workspace-min-width`       | 520px  |
| `--workspace-min-height`      | 280px  |
| `--command-bar-max-width`     | 1120px |
| `--inspector-width`           | 420px  |
| `--inspector-min-width`       | 340px  |
| `--inspector-max-width`       | 440px  |
| `--dock-header-height`        | 52px   |
| `--dock-card-width`           | 340px  |
| `--dock-card-min-width`       | 260px  |
| `--dock-card-height`          | 87px   |
| `--dock-waveform-height`      | 101px  |
| `--dock-wave-min-width`       | 420px  |
| `--mix-lane-height`           | 110px  |
| `--player-height`             | 60px   |
| `--player-inset-x`            | 20px   |
| `--player-inset-bottom`       | 17px   |
| `--player-scrub-min-width`    | 180px  |
| `--player-cue-width`          | 36px   |
| `--scrub-width-compact`       | 64px   |
| `--sheet-max-width`           | 560px  |
| `--sheet-drawer-width`        | 380px  |

The small parts have their own tokens, listed with each component in [components.md](components.md).

## Borders, not shadows

Borders, not shadows. No blur, no gradients, no glass.

Panels, bars, buttons and rows stay flat and are separated by 1 px borders (`--border-width`). A popover, menu or the command list sits on `--color-raised` with a 1 px border and has no drop shadow. Depth in the art comes from the dot wave itself, whose dots fade with distance.

## Focus and motion

Focus is a 2 px ring (`--focus-ring-width`) drawn inside the element (`--focus-ring-offset: -2px`).

Motion eases out (`--ease-out: cubic-bezier(0.16, 1, 0.3, 1)`), lasts 80 or 140 ms (`--duration-fast`, `--duration-normal`, aliased as `--motion-fast` and `--motion-slow`), and exists only to explain a change of state. With reduced motion set in the OS, every duration is 0.

## Icons

One outline icon set with a 1.5 px stroke: Lucide. Icons come in 13, 16 and 18 px and take the colour of their text.

## Waveforms

Bars are one device pixel wide, on whole pixel positions, with no anti-aliasing. Each bar is drawn in layers of one tone: the peak as a faint outline, then the RMS level of the high (above 2.5 kHz), mid and low (below 150 Hz) bands, each brighter than the last. Kicks and bass read as the solid core of a bar and hats as a fringe. Band energy is shown as brightness, never as colour. The accent is only for the playhead and cue markers.

The data is precomputed and passed as binary: a 140-column overview per recording, kept by audio hash and drawn by table rows and the dock, and 300 bins a second for a loaded deck.

Played bars use `--color-text` and bars still to play use `--color-text-2`. The energy curve in the inspector is a flat line of bars over a 1 px baseline, with no fill under it.

`MixRenderer` (`ui/src/lib/mix/renderer.ts`) draws the scrolling deck lanes in WebGL2 from binary waveform frames. `Waveform.svelte` draws the static `inline`, `crossfade` and `curve` waveforms on whole device pixels in a 2D canvas, since a table row cannot hold a WebGL context of its own.

## The dot wave

`Wave.svelte` draws a field of monochrome dots in WebGL2, which fade with depth. It is the app's art, used in four places:

- the foot of the sidebar, full width and 78 px tall, fading upward into the surface
- behind workspace headers, faint (0.12 opacity in dark, 0.07 in light)
- in collection cards in Sets (M10)
- as artwork for a track that has none: a still frame seeded from the track, so each track keeps the same tile

It never sits behind a table. It costs under 1 ms of UI-thread time per frame, shows a still frame under reduced motion, and pauses while Mix is playing. How it moves and how to tune it is in [wave.md](wave.md).

## Mark and wordmark

The mark is seven bars tracing a resonance peak, at 35, 60, 85, 100, 70, 45 and 25 % of its height. Bars are 2 px wide with 2 px gaps and 1 px rounded ends. It is drawn in `--color-text`. `Mark.svelte` can colour the centre bar with the accent (`accentPeak`), which the top bar leaves off. Sizes: 16 by 16, 19 by 21 in the top bar, and 32 by 36 on the design page.

The wordmark is RESONANCE in Space Grotesk Medium, uppercase, 16 px, tracked out to 0.32 em, 18 px after the mark (`--wordmark-gap`). Under it, at the top of the sidebar, a two-line mono label reads MUSIC INTELLIGENCE / FOR DJS.

The app icon is a separate SVG, `ui/src/lib/design/logo/app-icon.svg`: the mark in light bars on a dark rounded tile.

## Small graphic elements

These come from the brand board and give the interface its instrument feel. Each is one component in `ui/src/lib/design/`.

- Mono labels: uppercase JetBrains Mono, 11 px (or 10 px), weight 500, tracked 0.08 em, in Muted or Text 2. An optional info icon and a right-aligned action such as "View all".
- Focus brackets: four 1 px corner ticks with 8 px arms (`--focus-bracket-size`), in Text, or the accent when armed. They frame an active deck or a drop target.
- Index marks: two-digit numbers in square brackets, such as `[01]`, in 10 px mono and Muted.
- Data tags: small bordered badges in mono for facts such as `128.0 BPM`, `8A` or `WAV 24-bit`, plus removable tags, scores and shortcut badges.
- Modular dividers: 1 px rules in the border colour, with optional 4 px end ticks (`--divider-tick-size`) or an inline label.
- Grid dots: a 1 px dot grid every 16 or 24 px (`--grid-dots-sm`, `--grid-dots-md`) behind empty drop zones and the set timeline.

## Light theme

The light theme changes colour tokens and nothing else. Sizes, type, corners and the 1 px borders are the same in both. Inverted pills swap to a dark fill with light text. The dot wave and the waveforms read their colours from the tokens and redraw when the theme changes.
