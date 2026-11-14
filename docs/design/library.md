# Library and inspector

`concept-dark.png` shows the Library screen and right-hand Inspector. Issue [#52](https://github.com/BradHudsonGrant/RESONANCE/issues/52) rebuilds `ui/src/lib/library/` (`Library.svelte`, `TrackTable.svelte`, `columns.ts`, `Inspector.svelte`) to this design using the fields already returned by `LibraryTrack`.

## Workspace header and filters

The header sits directly below the shell's command block on `--color-background` (`#0B0B0C` dark / `#F8F7F5` light).

- **Title row (`#52`):** `padding: 16px 16px 10px`, flex row with `justify-content: space-between` and `align-items: center`.
  - Left: `Library` (`Space Grotesk` `20px` `--text-xl`, weight `500`, `--color-text`) beside the track count (`12,846 tracks` or `50 tracks` in `JetBrains Mono` `13px` `--color-text-2`, `tabular-nums`, `12px` gap).
  - Right: Filter/columns icon button (`28 x 28 px`, `--radius-control: 6px`, `--color-surface`, `1px solid var(--color-border-control)`, Lucide `SlidersHorizontal` `15px`), disabled until M2 column/filter configuration lands.
  - **Left out:** The list/grid segmented toggle next to the sliders icon in `concept-dark.png`. `Library` stays a dense table and never switches to decorative cards.
- **Filter pills row (`#52`):** `padding: 0 16px 12px`, `display: flex`, `gap: 8px`, `border-bottom: 1px solid var(--color-border)`.
  - `All Tracks` (`28px` height, `padding: 0 14px`, `--radius-pill`, active inverted fill `--color-text` with `--color-background` label, `Inter` `12px` weight `500`). Active in `#52`.
  - `✓ Analysed`, `○ Unanalysed` (British spelling; replaces board's `Analyzed` / `Unanalyzed`) — scheduled for **M5 Analysis**.
  - `★ Favourites` (British spelling; replaces board's `Favorites`), `◷ Recent` — scheduled for **M2 Library**.
  - Until their milestone lands, inactive pills render in `--color-muted` (`#8C8C8C` dark / `#6C6965` light) with a tooltip naming the milestone (`"Arrives in M2"` or `"Arrives in M5"`).
- **No collection cards above the table:** `concept-dark.png` places four dot-wave collection cards (`Peak Time`, `Warm Up`, `Afterhours`, `Deep / Hypnotic`) and a `+ New Collection` card above the table. Resonance keeps those cards out of `Library` so the track table owns the full workspace height. Collections list under `Library` in the sidebar (M2), and `WaveCard` grids live in `Sets` (M10).

## Track table

`TrackTable.svelte` remains virtualised (`role="grid"`, `aria-label="Tracks"`, overscan `8` rows, `contain: strict` on the scrolling body, `contain: layout paint` on rows) with `--row-height: 30px`.

- **Sticky column header (`#52`):** `30px` height on `--color-surface` (`#141415` dark / `#FFFFFF` light), `1px solid var(--color-border)` bottom divider, `Inter` `11px` weight `500` `--color-text-2`, right-aligned on numeric columns. Sort chevrons (`^` / `v`) arrive with server-side sort in **M2 Library**.
- **Data rows (`#52`):** `30px` height, `--color-background` default, `--color-surface-2` hover (`#1E1E1F` dark / `#ECEAE7` light), `1px solid var(--color-border)` hairline bottom border.
- **Selected row (`#52`):** `aria-selected="true"` draws a rounded selection frame (`--radius-control: 6px`, `1px solid var(--color-border-control)`, `border-left: 2px solid var(--color-accent)`), on `--color-raised` in dark (`#252527`) and `--color-surface-2` in light (`#ECEAE7`). Text stays `--color-text` in `Title` (weight `500`) and `Artist`.

### Columns in `#52` and later milestones

Columns whose data does not exist in `LibraryTrack` today stay out of the table until the milestone that brings their data (per [#52](https://github.com/BradHudsonGrant/RESONANCE/issues/52)).

| Column / cell | Contents and style | Width | Milestone |
| --- | --- | --- | --- |
| Selection checkbox | `16 x 16 px` rounded check box (`--radius-sm: 4px`, `--color-border-control`) | `2rem` | M3 Identify and tag (batch edit) |
| Row status / play icon | `▶` play indicator (M2 preview), `★` favourite (M2), `✓` analysed mark (M5) | `2rem` | M2 Library / M5 Analysis |
| `#` | 1-based row index, `JetBrains Mono` `12px` `--color-muted`, `tabular-nums`, right-aligned | `3rem` | `#52` |
| `Title` | `track.title`, `Inter` `13px` `--color-text` | `minmax(12rem, 3fr)` | `#52` |
| Inline waveform | 1 px whole-pixel peak/RMS bars (`18px` tall, `--color-text-2` / `--color-text`) | `7.5rem` | M5 Analysis (cached Tier 1 waveform) |
| `Artist` | `track.artist`, `Inter` `13px` `--color-text-2` (`--color-text` when selected) | `minmax(9rem, 2fr)` | `#52` |
| `BPM` | `tempo(track.bpm)` via `Provenance`, `JetBrains Mono` `12px` `--color-text`, right-aligned | `4.5rem` | `#52` (sort caret M2) |
| `Key` | `track.key?.camelot ?? track.key_tag` via `Provenance`, `JetBrains Mono` `12px` `--color-text`, right-aligned | `4rem` | `#52` (sort caret M2) |
| `Energy` | 5-bar micro meter (`18 x 12 px`, `--color-text` / `--color-border`) | `4.5rem` | `#50` primitive; M5 Analysis |
| `Tension` | Shown on `concept-dark.png` | — | **Left out:** tension is a pairwise transition score (M8/M10), not a single-track property |
| `Time` | `duration(track.audio?.duration_ms)`, `JetBrains Mono` `12px` `--color-text-2`, right-aligned | `4.5rem` | `#52` |
| `Genre` | `track.genre`, `Inter` `12px` `--color-text-2` | `minmax(7rem, 1fr)` | `#52` |
| `Format` | `track.format.toUpperCase()`, `JetBrains Mono` `11px` `--color-text-2` | `4.5rem` | `#52` |
| Row actions `...` | Quiet `24 x 24 px` overflow trigger opening track actions | `2.5rem` | M2 Library / M3 Tags / M4 Replace |

### Empty, loading and error states (`#52`)

- **Empty (`pages.total === 0`):** `240px` tall `Wave` WebGL2 canvas above a `Space Grotesk` `15px` heading `"No tracks yet"`, `Inter` `13px` `--color-text-2` note `"Add a music folder to see its tracks here. Nothing in the folder is changed."`, and one primary `--color-accent` button `"Add a music folder"`.
- **Loading:** 12 horizontal skeleton rows (`8px` tall, `--radius-sm: 4px`, `--color-surface-2`) with visually hidden `role="status"` text `"Reading the library"`.
- **Error:** `role="alert"` heading `"The library could not be shown"`, message in `--color-text-2`, and `"Try again"` button.

## Inspector

`<aside aria-label="Inspector">` is `320px` wide on `--color-surface` (`#141415` dark / `#FFFFFF` light) with a `1px solid var(--color-border)` left border and `padding: 16px` sections separated by `1px solid var(--color-border)` horizontal dividers.

### Track header and audio facts (`#52`)

1. **Header bar (`40px` height):** `INSPECTOR` mono uppercase label on the left (`11px` `--color-muted`), favourite `★` toggle (M2), `...` actions button (M2/M3) and close button `X` (`28 x 28 px` quiet button, shortcut `I`, `#52`) on the right.
2. **Track identity row (`display: grid`, `grid-template-columns: 64px 1fr`, `gap: 12px`, `align-items: start`):**
   - `64 x 64 px` `WaveThumb` (`--radius-panel: 8px`, `1px solid var(--color-border)`): deterministic monochrome dot-wave still seeded from the track's facts in `#52`; replaced by embedded artwork when available in **M3 Identify and tag**.
   - Title in `Space Grotesk` `16px` weight `600` `--color-text` (`overflow-wrap: anywhere`), artist in `Inter` `13px` `--color-text-2` (`"No artist tag"` in `--color-muted` when null).
   - Badge strip below artist (`margin-top: 8px`, `display: flex`, `flex-wrap: wrap`, `gap: 8px`, `align-items: center`):
     - Status `DataTag`: `"Read"` (`#52` when `track.audio` decoded) or `"Unreadable"` (`1px solid var(--color-accent)` border when `!track.audio`); becomes `"✓ Analysed"` once M5 analysis runs.
     - Audio format strip (`JetBrains Mono` `11px` `--color-text-2`, `tabular-nums`): `WAV · 44.1 kHz · 24-bit` (or `MP3 · 320 kbps · 44.1 kHz`), channels and file size.

### Metadata grid (`#52`)

Every label uses the **mono uppercase label** style (`JetBrains Mono` `11px` `--color-muted`, `letter-spacing: 0.08em`). Every missing tag displays `—` in `--color-muted` instead of hiding the grid structure.

- **Row 1 (4 columns, `grid-template-columns: repeat(4, minmax(0, 1fr))`, `gap: 12px`):**
  - `BPM` (`JetBrains Mono` `13px` `--color-text`, `Provenance`)
  - `KEY` (`track.key` as Camelot + musical name, or `track.key_tag`, `JetBrains Mono` `13px` `--color-text`)
  - `TIME` (`duration`, `JetBrains Mono` `13px` `--color-text`)
  - `YEAR` (`track.year`, `JetBrains Mono` `13px` `--color-text`; replaces board's `RELEASE` label to match `LibraryTrack.year`)
- **Row 2 (3 columns, `grid-template-columns: repeat(3, minmax(0, 1fr))`, `gap: 12px`, `margin-top: 12px`):**
  - `GENRE` (`track.genre`, `Inter` `13px` `--color-text`)
  - `ALBUM` (`track.album`, `Inter` `13px` `--color-text`; replaces board's `SUBGENRE` since `LibraryTrack` carries `album`, not a subgenre field)
  - `LABEL` (`track.label`, `Inter` `13px` `--color-text`)
- **Row 3 (full width, `margin-top: 12px`):**
  - `LOCATION` (`track.location`, `JetBrains Mono` `11px` `--color-text-2`, `user-select: text`, `overflow-wrap: anywhere`), plus `COMMENT` and `ALBUM ARTIST` when present.

### Sections waiting for later milestones

In `#52`, unbuilt sections below the metadata grid either remain hidden or appear as one quiet placeholder line naming the milestone they wait on. They never display made-up tags, trait scores, curves or matches.

| Inspector section | Design from `concept-dark.png` | What Resonance draws | Milestone |
| --- | --- | --- | --- |
| `TAGS` | Mono header, removable tag pills (`24px`, `--surface-2`, `1px solid --border`, `×`), `+` button | Editable track tags and soft search descriptors | **M3** Identify and tag / **M6** Discovery |
| `AUDIO TRAITS` | Mono header + `ⓘ`, horizontal trait bars (`Energy`, `Loudness`, `Vocal activity`, `Brightness`) | Measured Tier 1 and Tier 2 traits. (`Tension`, `Danceability` and `Atmosphere` are left out unless an analyser produces them.) | **M5** Analysis |
| Energy curve plot | Segmented `Energy` pill (`inverted` active style) + flat 1 px monochrome curve with mono axis ticks `0:00` .. end | Monochrome bar/line plot of the energy curve. **No gradient fill** under the line. (`Tension` tab is left out.) | **M5** Analysis |
| `TRANSITION SUGGESTIONS` (`Matches`) | Mono header + `ⓘ` + `View All ->`, 3 rows (`▶` `24 x 24 px`, title/artist, mono `BPM · Key · ΔBPM · Key relation`, `%` `DataTag`, `...`) | Embedding neighbours and transition compatibility with reasons | **M6** Discovery / **M10** Sets and transitions |
| Empty state | Centered prompt when `track` is undefined | `"Select a track to see its details here."` in `Inter` `13px` `--color-text-2` | `#52` |
