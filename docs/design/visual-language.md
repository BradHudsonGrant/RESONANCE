# Visual language

`concept-dark.png` and `concept are and design 2.png` define Resonance's visual language, with `concept-dark.png` leading on proportions, corner radii and density. Issue [#50](https://github.com/BradHudsonGrant/RESONANCE/issues/50) brings these tokens, primitives and micro elements into `ui/src/lib/design/` and renders every specimen in both themes on `/design.html`.

## Colour and surfaces

| Token | CSS custom property | Dark | Light | Use |
| --- | --- | --- | --- | --- |
| Background | `--color-background` | `#0B0B0C` | `#F8F7F5` | Main canvas, top bar, table rows, chart area |
| Surface | `--color-surface` | `#141415` | `#FFFFFF` | Sidebar, inspector, player bar, cards, sticky table header |
| Surface 2 | `--color-surface-2` | `#1E1E1F` | `#ECEAE7` | Secondary controls, deck pods, data tags, scrub tracks, row hover |
| Raised | `--color-raised` | `#252527` | `#FFFFFF` | Active dark sidebar/table row, dropdown menus, confirm sheets, deck badges |
| Border | `--color-border` | `#343437` | `#D8D4CF` | 1 px area dividers, panel frames, card borders, inactive meter segments |
| Control border | `--color-border-control` | `#6E6E71` | `#8A8681` | Interactive control edges, active row frame, passive focus brackets |
| Text | `--color-text` | `#F2F1EF` | `#171717` | Primary text, active values, active meter bars, solid inverted pill background |
| Text 2 | `--color-text-2` | `#A9A8A6` | `#595754` | Secondary labels, artist column, mono metadata values, peak waveform bars |
| Muted | `--color-muted` | `#8C8C8C` | `#6C6965` | Mono uppercase labels, index marks, disabled controls, missing-tag placeholders |
| Accent | `--color-accent` | `#E4553D` | `#D94F3B` | Playhead, active selection bar, active focus brackets, one primary button per view |
| On accent | `--color-on-accent` | `#0B0B0C` | `#0B0B0C` | Text and icons on a solid `--color-accent` fill |

Board 2 (`[03] UI PALETTE`) matches these tones. Light `Text 2`, `Muted`, `Control border` and `On accent` were adjusted in M0 so every pair clears WCAG AA; `Light` `--color-raised` shares `#FFFFFF` with `Surface`, so an active light sidebar item or selected light table row uses `--color-surface-2` (`#ECEAE7`) to stand apart from its `--color-surface` container. All ratios live in [contrast.md](contrast.md), checked on every test run by `ui/dev/contrast.test.ts`.

### Surface and accent rules

- **Flat surfaces, no gradients.** Panels, bars, buttons, rows, deck pods and charts use solid token fills separated by `1px` borders. No CSS gradients, no drop shadows, no blur and no translucent glass. Depth comes only from the 3D dot wave ([wave.md](wave.md)), where monochrome dots fade with distance in the vertex shader.
- **Accent discipline.** `--color-accent` marks only the playhead, the `2px` left selection bar on the active space or row, active focus brackets, the single control needing action in Coach, and at most one primary button per view. Never set running text or badge labels in `--color-accent` on a surface token, because it drops below 4.5 to 1 in light mode and on dark `surface-2` and `raised`. Active labels stay in `--color-text` beside an accent mark, or in `--color-on-accent` on a solid accent fill.

## Typography and type scale

Three variable fonts are bundled with the UI via `@fontsource-variable`. Nothing loads from a CDN.

- **Heading (`--font-heading`):** `Space Grotesk Variable`, weights `500` and `600`. Used for the wordmark, workspace titles (`Library`, `Mix`, `Sets`), inspector track title, `A` and `B` deck badges, `A -> B` dock heading and large stat numbers.
- **Interface (`--font-ui`):** `Inter Variable`, weights `400` and `500`. Used for body text, sidebar items, `Title`, `Artist` and `Genre` cells, button labels, command descriptions and coach prompts.
- **Mono and data (`--font-mono`):** `JetBrains Mono Variable`, weights `400` and `500`, with `font-variant-numeric: tabular-nums` on every figure. Used for BPM, key, duration, timestamps, sample rate, bit depth, bitrate, percentages, track counts, slash commands (`/scan`), index marks (`001`), data tags (`INDEX 0427`) and mono uppercase section headers.

| Token | Size | Line height | Tracking | Use |
| --- | --- | --- | --- | --- |
| `--text-2xs` | `10px` | `1.2` | `0.08em` | Modular divider `//` mark, micro axis ticks (`0:00 .. 7:02`) |
| `--text-xs` | `11px` | `1.3` | `0.08em` when uppercase | Mono uppercase labels, metadata keys, `Ctrl K` hint, data tags, slash tokens, format badges |
| `--text-sm` | `12px` | `1.35` | `0` | Numeric table cells, sidebar counts, status bar, player time and volume labels |
| `--text-md` | `13px` | `1.4` | `0` | Default interface text, table `Title` and `Artist`, sidebar labels, command input |
| `--text-lg` | `15px` | `1.3` | `0` | Inspector track title (`15px` to `16px`), panel headings, confirm sheet title |
| `--text-xl` | `20px` | `1.2` | `-0.01em` | Workspace title (`Library`, `Prep`, `Sets`, `Mix`, `Replay`) |
| `--text-2xl` | `24px` | `1.15` | `-0.01em` | Set hero title (M10) and bracketed score figures (M8, M9) |

## Spacing, radii and sizing

Spacing stays on a 4 px scale:

- `--space-1`: `4px`
- `--space-2`: `8px`
- `--space-3`: `12px`
- `--space-4`: `16px`
- `--space-5`: `20px`
- `--space-6`: `24px`
- `--space-8`: `32px`

Corner radii follow `concept-dark.png`, replacing the brief's 4 px and 6 px limits:

| Token | Value | Where it applies |
| --- | --- | --- |
| `--radius-sm` | `4px` | Checkboxes (`16 x 16 px`), `Ctrl K` key cap, progress and trait tracks (`2px` to `4px`), skeleton bars |
| `--radius-control` | `6px` | Buttons, text inputs, selects, slash-command chips, data tags, sidebar items, `40 x 40 px` thumbnails, selected row frame |
| `--radius-panel` | `8px` | Command input box, `64 x 64 px` inspector thumbnail, Deck A / B cards, dropdown menus, waveform slot, stat cards |
| `--radius-lg` | `10px` | Confirm sheets, outer framed panels, dot-wave set cards (`WaveCard`) |
| `--radius-pill` | `9999px` | Filter pills (`All Tracks`), `Energy` curve pill, circular transport button (`32 x 32 px`), status dots |

Control and row heights:

- `--control-height-sm`: `24px` — removable tags, inline `+` buttons, compact icon actions
- `--control-height`: `28px` — standard buttons, filter pills, segmented tabs, selects, header icon buttons
- `--control-height-lg`: `36px` — command bar search input, hero primary actions
- `--row-height`: `30px` — virtualised library table rows and command palette options (within the 28 to 32 px rule)

## Micro elements

Every micro element from `concept are and design 2.png` `[05]` and `concept-dark.png` is built in `#50` inside `ui/src/lib/design/` and shown on `/design.html` in both themes.

| Element | Tokens and sizes (Dark / Light) | Where it appears | Milestone |
| --- | --- | --- | --- |
| Mono uppercase label | `--font-mono`, `--text-xs` (`11px`), weight `500`, uppercase, `letter-spacing: 0.08em`, `--color-muted` (`#8C8C8C` / `#6C6965`) | Inspector headers (`TAGS`, `AUDIO TRAITS`, `TRANSITION SUGGESTIONS`), inspector metadata keys (`BPM`, `KEY`, `TIME`), sidebar group labels | `#50` primitive; `#51`/`#52` shell & inspector |
| Index marks | Zero-padded 3-digit numbers (`001`, `002`, `003`) in `--font-mono`, `--text-xs` (`11px`), `tabular-nums`, `--color-muted`, separated by `1 x 10 px` `--color-border` ticks and `12px` horizontal gaps | Set timeline rulers, phrase/bar rulers, `/design.html` | `#50` primitive; M5, M9, M10 rulers |
| Focus brackets | Four corner L-brackets (`6 x 6 px` arms, `1.5px` stroke) plus optional `6 x 6 px` centre `+` crosshair (`1.5px` stroke). Active state in `--color-accent` (`#E4553D` / `#D94F3B`); passive frame in `--color-border-control` (`#6E6E71` / `#8A8681`) | Dot-wave hero focus region, `#52` Mix waveform viewport corners, M8/M9 score cards, `/design.html` | `#50` primitive; `#52` Mix; M8, M9, M10 |
| Data tag | `22px` height, `0 8px` padding, `--radius-control` (`6px`), `--font-mono` `--text-xs` (`11px`), `--color-surface-2` (`#1E1E1F` / `#ECEAE7`) fill, `1px solid var(--color-border)`, `--color-text-2` key and `--color-text` value. Flag variant uses `1px solid var(--color-accent)` border with `--color-text` label, or `--color-accent` fill with `--color-on-accent` (`#0B0B0C`) text | Inspector status pill (`Read`, `✓ Analysed`), compatibility badges (`95%`), set metrics (`INDEX 0427`), `/design.html` | `#50` primitive; `#52` status; M5, M6, M10 |
| Removable tag chip | `24px` height, `0 8px` padding, `--radius-control` (`6px`), `--color-surface-2` fill, `1px solid var(--color-border)`, `--font-ui` `--text-sm` (`12px`) `--color-text` label, `12 x 12 px` `×` button in `--color-text-2` | Inspector `TAGS` section, natural-language query chips | `#50` primitive; M3 tags; M6 search chips |
| Slash-command chip | `26px` height, `0 8px` padding, `--radius-control` (`6px`), `--color-surface` fill, `1px solid var(--color-border)`, containing an inner mono badge (`/ scan`, `/ mix`, `--color-surface-2`, `--font-mono` `11px` `--color-text`, `--radius-sm: 4px`) and description (`--font-ui` `12px` `--color-text-2`, `8px` gap) | Command bar strip below the search input | `#50` primitive; `#51` command bar |
| Modular divider | `12px` tall flex row (`gap: 8px`): `24 x 2 px` lead bar (`--color-text-2`, or `--color-accent` on hero dividers), `1fr x 1px` rule (`--color-border`), `--font-mono` `--text-2xs` (`10px`) `//` mark (`--color-muted`), three `2 x 2 px` dots (`4px` gap, `--color-muted`) | Section breaks on `/design.html` and overview headers; compact panels keep plain `1px solid var(--color-border)` rules | `#50` primitive |
| Grid dots | Repeating `1.5px` square dots on an `8 x 8 px` grid in `--color-border` (`#343437` / `#D8D4CF`) over `--color-background` | Empty A to B dock slots, idle deck tiles, `/design.html` (never behind tables) | `#50` primitive; `#51` dock |
| 5-bar cell meter | Five vertical bars (`2px` wide, `2px` gap; heights `4, 6, 8, 10, 12 px`; `18 x 12 px` box). Active bars in `--color-text`, inactive in `--color-border` (`--color-border-control` on selected row) | Library table `Energy` column, `/design.html` | `#50` primitive; M5 Analysis |
| 5-dot level meter | Five `6 x 6 px` circles (`4px` gap). Active dots in `--color-text`, inactive in `--color-border` | Set summary strips, `/design.html` | `#50` primitive; M10 Sets |
| Horizontal trait bar | `20px` tall grid row: `88px` label (`--font-ui` `12px` `--color-text-2`), `36px` right-aligned value (`--font-mono` `12px` `--color-text`), `1fr x 4px` bar (`--radius-sm` track `--color-surface-2`, fill `--color-text`) | Inspector `AUDIO TRAITS`, `/design.html` | `#50` primitive; M5 Analysis |
| Progress / signal scrub bar | `4px` track (`--color-surface-2`, `--radius-sm`), active fill (`--color-text` in player bar, `--color-accent` when scrubbing or in the compact specimen), `10 x 10 px` circular thumb (`--radius-pill`) | `PlayerBar` timeline and volume sliders, `/design.html` | `#50` primitive; `#51` frame; M2 preview; M7 decks |

## Primitives and cards

- **Buttons (`Button.svelte`, `#50`):** `height: 28px`, `--radius-control` (`6px`), `--font-ui` `--text-sm` (`12px`), weight `500`, `padding: 0 12px`, `gap: 8px`.
  - `primary`: `--color-accent` fill, `--color-on-accent` (`#0B0B0C`) text, `1px solid var(--color-accent)` (one per view).
  - `inverted`: `--color-text` fill (`#F2F1EF` dark / `#171717` light), `--color-background` text (`#0B0B0C` dark / `#F8F7F5` light), used for active filter pills (`All Tracks`), active curve pills (`Energy`) and solid play actions (`▶ Play`).
  - `default`: `--color-surface-2` fill, `1px solid var(--color-border-control)`, `--color-text` label (`--color-raised` on hover in dark, `--color-surface` in light).
  - `quiet`: `transparent` fill, no visible border, `--color-text-2` (`--color-surface-2` and `--color-text` on hover).
  - `icon` and `transport`: `28 x 28 px` (`--radius-control: 6px`) for toolbar/row actions; `32 x 32 px` (`--radius-pill`) for the primary circular transport button (`▶` / `||`).
- **Segmented tabs (`SegmentedTabs.svelte`, `#50`):** Container on `--color-surface-2` with `1px solid var(--color-border)`, `padding: 2px`, `--radius-control` (`6px`), `height: 28px`. Active option takes `--color-text` fill with `--color-background` label (high contrast) or `--color-raised` with `--color-text`; inactive options take `--color-text-2`. Used for curve toggles and inspector/settings mode switches; board 2's `All | Tracks | Albums | Artists` labels stay out because Resonance does not split the library into album or artist grids.
- **Filter pills (`#50` / `#52`):** `28px` height, `padding: 0 12px`, `--radius-pill` (`9999px`), optional leading `14px` icon. Active pill uses the `inverted` button style; inactive pills use `transparent` fill and `--color-text-2` text (`--color-muted` when disabled until M2/M5).
- **Dot-wave artwork thumbnail (`WaveThumb.svelte`, `#50`):** Square frame (`64 x 64 px` in the inspector header with `--radius-panel: 8px`; `44 x 44 px` in the A to B dock and `36 x 36 px` in Next lists with `--radius-control: 6px`; `1px solid var(--color-border)`; `--color-surface` fill). When a track has no embedded cover art (every track until M3, and untagged files thereafter), `WaveThumb` draws a deterministic monochrome dot-wave still seeded from the track's own identity and facts (title/path hash, duration and sample rate in M1; three-band energy once Tier 1 analysis lands in M5). No stock imagery is used. Real artwork replaces the dot wave once tags bring it in M3.
- **Dot-wave card (`WaveCard.svelte`, `#50`, used in M10 Sets):** `--radius-lg` (`10px`), `1px solid var(--color-border)`, `--color-surface` fill, `padding: 12px`. Displays a seeded monochrome dot-wave canvas across the top or full frame (with dots fading cleanly rather than a gradient wash), title in `Space Grotesk` `15px` `--color-text`, subtitle and count in `JetBrains Mono` `12px` `--color-text-2`, and an optional `28 x 28 px` play button. Never placed above the `Library` table.
- **Bracketed stat card (`StatCard.svelte`, `#50`, used in M8 Coach and M9 Replay):** `--color-surface` fill, `1px solid var(--color-border)`, `--radius-panel` (`8px`), `padding: 16px`, framed with active `--color-accent` focus brackets at opposite or all four corners. Holds a `13px` `Inter` heading, 1 px monochrome vertical signal bars with `--color-accent` peaks, a `Space Grotesk` `20px` figure (`0.87`) and a `--text-sm` `--color-text-2` caption.
- **Panels (`Panel.svelte`), confirm sheets (`Sheet.svelte`) and provenance (`Provenance.svelte`):** Kept from M0, updated to `--radius-panel` (`8px`) on panels and `--radius-lg` (`10px`) on sheets. `Provenance` continues to show measured (plain), estimated (hollow dot with tooltip) and confirmed (tick) on every field where data has provenance.

## Waveforms and logo

- **Waveforms:** Bars are one device pixel wide on whole pixel positions with no anti-aliasing. Two tones show peak (`--color-text-2`) and RMS (`--color-text`). No rainbow colouring. `--color-accent` marks only the playhead and cue markers. Overview, mini row waveforms (M5) and scrolling lanes share the WebGL2 renderer and precomputed binary peak format.
- **Logo (`Mark.svelte`, `Wordmark.svelte`):** Seven vertical rounded bars (`rx="2"`) tracing a resonance peak (`Mark.svelte`), six in `currentColor` (`--color-text`) and the centre peak bar in `--color-accent`. `Wordmark.svelte` pairs the `20 x 20 px` mark (`16px` tall bars) with `RESONANCE` in `Space Grotesk` Medium, uppercase, `0.32em` tracking. From `concept are and design 2.png` `[02] LOGO / SYMBOL EXPLORATION`: option `A` is the core mark, option `D` becomes the Grid Dots micro element, option `F` becomes the Focus Brackets micro element, and exploratory options `B`, `C` and `E` are left out.

## Icons, motion and accessibility

- **Icons (`Icon.svelte`):** Lucide outline set with a `1.5px` stroke (`16px` in sidebar rows and inline controls, `18px` in collapsed rail mode, `14px` in pills and chips).
- **Motion:** Transitions last `120ms` (`--motion-fast`) to `180ms` (`--motion-slow`) using `cubic-bezier(0.16, 1, 0.3, 1)` (`--ease-out`) and explain state changes only. Under `prefers-reduced-motion: reduce`, `--motion-fast` and `--motion-slow` become `0ms` and `Wave` defaults to the `Still` preset.
- **Contrast modes:** Under `prefers-contrast: more`, `--color-border` takes `--color-border-control` and `--color-muted` takes `--color-text-2`. Under Windows `forced-colors: active`, `--color-accent` maps to `Highlight`, `--color-on-accent` maps to `HighlightText`, and borders map to `CanvasText`.
