# Library Workspace and Inspector

The Library workspace (`ui/src/lib/library/Library.svelte`, `TrackTable.svelte` and `Inspector.svelte`) occupies the centre column (`x = 257..1164`, width `908px` at the `1586 x 992` reference viewport) below `CommandBar` (`y = 153..752`) and pairs with the `420px` right-hand `Inspector` column (`x = 1166..1585, y = 51..752`).

Every dimension below is measured from `concept-dark.png` (`1586 x 992` at device pixel ratio `1`) and uses the tokens and components defined in [visual-language.md](visual-language.md) and [components.md](components.md). Issue [#52](https://github.com/BradHudsonGrant/RESONANCE/issues/52) updates `Library.svelte`, `TrackTable.svelte` and `Inspector.svelte` to this specification.

## 1. Workspace header and filter pills (`Library.svelte`)

Measured across `x = 276..1130, y = 178..245`:

- **Title row (`y = 178..214`):**
  - Left: `Library` heading (`x = 278..363, y = 178..204`, `--font-display`, `font-weight: 500`, `--text-2xl: 27px`, cap height `20px`, `letter-spacing: -0.02em`, `--color-text`), followed `22px` to the right by the tabular track count `12,846 tracks` (`x = 385..464, y = 187..199`, `--font-mono`, `--text-md: 13px`, `--color-text-2`).
  - Right: Filter sliders button (`Button variant="icon" size="md"`, `x = 1096..1130, y = 181..214`, `34 x 34 px`, `--control-height-md: 34px`, `--radius-sm: 6px`, `1px solid var(--color-border)`, `--color-surface-2`, `16px` `Icon name="sliders"`), plus the `ScanFolder` trigger in M1/M2.
  - **Left out from `concept-dark.png`:** The two-segment Table / Card view switcher (`x = 998..1075, y = 181..214`, `78 x 34 px`) is omitted per Issue [#49](https://github.com/BradHudsonGrant/RESONANCE/issues/49); the library is always a virtualised table.
- **Filter pill strip (`y = 220..245`, `16px` below the title row):**
  - Built from `FilterPill.svelte` (`height: var(--control-height-sm)` `26px`, `border-radius: var(--radius-pill)`):
    1. `All Tracks` (`x = 278..360, y = 220..244`, `83 x 25 px`): active state with inverted monochrome fill (`background: var(--color-text)` `#F2F1EF` dark / `#141413` light, `color: var(--color-background)` `#0B0B0C` dark / `#FAF9F7` light, `--font-sans`, `font-weight: 500`, `--text-sm: 12px`).
    2. `✓ Analysed` (`x = 385..465`): inactive pill (`13px` `Icon name="check"`, `--font-sans`, `--text-sm: 12px`, `--color-text-2`).
    3. `○ Unanalysed` (`x = 493..587`): inactive pill (`13px` `Icon name="unanalysed"`, `--color-text-2`).
    4. `★ Favourites` (`x = 615..702`): inactive pill (`13px` `Icon name="star"`, `--color-text-2`).
    5. `◷ Recent` (`x = 730..792`): inactive pill (`13px` `Icon name="recent"`, `--color-text-2`).
  - In `#52` clicking a `FilterPill` toggles its active visual state; `M2` and `M5` wire the SQLite filter queries behind each pill.

### Collection cards (`Sets` overview in `M10`, not in `Library`)

`concept-dark.png` draws a horizontal strip of four collection cards (`181 x 82 px` each at `y = 262..343, x = 276, 468, 659, 849`, `11px` gap, `--radius-lg: 10px`, `Card variant="interactive" waveArt={true}`) plus a dashed `+ New Collection` card (`101 x 81 px` at `x = 1045..1145`, `Card variant="dashed"`) between the filter pills and the track table.

Per Issue [#49](https://github.com/BradHudsonGrant/RESONANCE/issues/49):

- Those collection cards move to the top of the **`Sets`** workspace (`M10`) so `Library` opens directly into `TrackTable` (`16px` below the filter pills), giving four additional visible track rows without scrolling.
- In the `Library` workspace, collections are navigated from the collapsible `Collections` sub-tree in `Sidebar.svelte` (`M2`).

## 2. Virtualised track table (`TrackTable.svelte`)

Measured across `x = 267..1150` (`884px` wide inside the `908px` centre column) and `y = 393..752`:

- **Row heights (`--table-header-height: 31px`, `--row-height: 33px`):**
  - Header row: `y = 393..424` (`31px` tall, `1px` top and bottom `--color-border` lines, `--font-sans`, `font-weight: 500`, `--text-sm: 12px`, `--color-text-2`).
  - Data rows: 10 visible rows between `y = 424` and `y = 753` (`329px / 10 = 32.9px` -> `--row-height: 33px`), separated by `1px` `--color-border` hairlines. **Table rows keep their `33px` height at every window size.**
- **Columns (`ui/src/lib/library/columns.ts`):**

| Column | Header | Measured `x` in `concept-dark.png` | Width / Grid track | Alignment | Typography & component | Drop tier (`@container`) | Milestone |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Select | `☐` | `x = 277..293` | `30px` fixed | Centre | `16 x 16 px` checkbox (`--radius-xs: 3px`, `1px solid var(--color-border-control)`) | Never drops | `#52` |
| Status | (none) | `x = 304..318` | `22px` fixed | Centre | `14 x 14 px` `Icon`: `▶` (`play` filled) on playing/selected row, `★` (`star`) on favourite (`M2`), `✓` (`check`) on analysed (`M5`) | Never drops | `#52` (`▶`), `M2`/`M5` |
| Index | `#` | `x = 325..340` | `36px` fixed | Right | `--font-mono`, `--text-sm: 12px`, `tabular-nums`, `--color-muted` (`--color-text` when selected) | Never drops | `#52` |
| Title | `Title` | `x = 376..485` | `minmax(150px, 1.8fr)` | Left | `--font-sans`, `font-weight: 500`, `--text-md: 13px`, `--color-text`, `text-overflow: ellipsis` | Never drops | `#52` |
| Waveform | (inline) | `x = 502..641` | `140px` (`88px` `<740px`) | Centre | `Waveform.svelte` `mode="inline"` (`140 x 17 px`, `1px` bars, `1px` gap) | Narrows `<740px`, hides `<640px` | `#52` (seeded), `M5` (real peaks) |
| Artist | `Artist` | `x = 674..750` | `minmax(120px, 1.3fr)` | Left | `--font-sans`, `font-weight: 400`, `--text-md: 13px`, `--color-text-2` (`--color-text` when selected) | Never drops | `#52` |
| BPM | `BPM` | `x = 767..805` | `58px` fixed | Right | `--font-mono`, `--text-md: 13px`, `tabular-nums`, `--color-text` (`128.0`) | Never drops | `#52` (fixture), `M5` (analysed) |
| Key | `Key` | `x = 837..875` | `52px` fixed | Left | `--font-mono`, `--text-md: 13px`, `--color-text` (`A min` / `8A`) | Never drops | `#52` (fixture), `M5` (analysed) |
| Energy | `Energy` | `x = 908..934` | `54px` fixed | Centre | `MeterBar.svelte` (`27 x 12 px`, 5 vertical bars of `3px` width + `3px` gap) | Hides `<820px` container width | `#52` (fixture), `M5` (analysed) |
| Time | `Time` | `x = 1055..1085` | `56px` fixed | Right | `--font-mono`, `--text-md: 13px`, `tabular-nums`, `--color-text-2` (`07:02`) | Never drops | `#52` |
| Actions | (none) | `x = 1120..1144` | `32px` fixed | Centre | `Button variant="icon" size="sm"` (`24 x 24 px`, `Icon name="more"`) | Never drops | `#52` |

- **Left out from `concept-dark.png`:** The `Tension` column (`x = 981..1007`, `27 x 12 px` 5-bar meter) is omitted per Issue [#49](https://github.com/BradHudsonGrant/RESONANCE/issues/49) because `Tension` is not a planned analysis output in `M5`.
- **Selected row state (`TableRow.svelte` `selected={true}`, Row 4 `Echo Chamber — Kaden` at `y = 491..524, x = 267..1150`):**
  - Matches `concept-dark.png` while keeping orange strictly to the `2px` selection mark:
    - Rounded container frame: `884 x 34 px`, `border-radius: var(--radius-sm)` (`6px`), `background: var(--color-raised)` (`#252527` dark / `#ECEAE7` light), `outline: 1px solid var(--color-border-control)` (`#6E6D6A` / `#abaaa5` light control border).
    - Left selection mark: `2px` wide vertical bar in `--color-accent` (`#E4553D` dark / `#D94F3B` light) inside the left edge of the rounded frame.
    - Ticked checkbox (`x = 277..293`): `16 x 16 px` (`--radius-xs: 3px`) filled with `--color-text` (`#F2F1EF` dark / `#141413` light) and a dark checkmark (`--color-background`).
    - Status cell (`x = 304..314`): filled `▶` play triangle (`8 x 10 px`, `--color-text`) beside the row index `4`.
- **Performance budget:** `TrackTable` renders only the visible row slice plus a small overscan buffer (`ui/src/lib/library/track-pages.svelte.ts`) and sets `performance.mark('tracks-filled')` when the first rows populate, holding fast scroll over 100,000 rows under `16 ms` per frame ([#40](https://github.com/BradHudsonGrant/RESONANCE/issues/40)).

## 3. Right-hand track inspector (`Inspector.svelte`)

Measured at `x = 1166..1585` (`420px` wide at the reference viewport, `--inspector-width: 420px`, with a `1px` left border at `x = 1165`) and `y = 51..752` (`702px` tall):

- **No `"INSPECTOR"` top bar:** In `concept-dark.png`, the inspector column has no title bar; it opens directly with the track artwork and title (`padding: 14px 20px 16px`).
- **Horizontal section dividers (`ModularDivider inset="md"`):** Five `1px` `--color-border` lines at `y = 169`, `y = 257`, `y = 321`, `y = 436` and `y = 589` (`x = 1180..1570`, `390px` wide, inset `15–20px` from the column edges).

### Section A — Track header (`y = 65..168`)

- **Artwork (`Thumb.svelte` `size="lg"`, `x = 1175..1271, y = 65..160`):** `96 x 96 px` (`--thumb-lg: 96px`), `border-radius: var(--radius-lg)` (`10px`), `1px solid var(--color-border)`. Shows album artwork (`M3`) or a deterministic `Wave variant="thumb"` fallback (`#52`).
- **Title and artist (`x = 1289..1510`):**
  - Track title (`Echo Chamber` at `y = 79..93`): `--font-display`, `font-weight: 600`, `--text-xl: 20px` (cap height `15px`), `letter-spacing: -0.02em`, `--color-text`.
  - Artist (`Kaden` at `y = 104..116`): `--font-sans`, `font-weight: 400`, `--text-base: 14px` (cap height `10px`), `--color-text-2`.
- **Top-right action buttons (`y = 76..104`):** Favourite star `Button variant="icon"` (`x = 1516`, `28 x 28 px`, `18px` `Icon name="star"`) and overflow `Button variant="icon"` (`x = 1548`, `28 x 28 px`, `18px` `Icon name="more"`).
- **Status pill and audio specs (`y = 131..152`):**
  - `✓ Analysed` (`DataTag variant="pill" leadingIcon="check"`, `x = 1290..1367, y = 131..152`, `78 x 22 px`, `--control-height-xs: 22px`, `--radius-pill`, `--color-surface-2`, `1px solid var(--color-border)`, `--font-mono`, `--text-xs: 11px`, `--color-text`; monochrome checkmark replacing the green icon on the board).
  - Audio format string (`x = 1390..1520, y = 136..147`): `WAV  44.1 kHz  24-bit` in `--font-mono`, `--text-sm: 12px`, `--color-text-2`.

### Section B — 2-row metadata grid (`y = 170..256`)

- **Row 1 (`y = 181..204`, 4 columns at `x = 1180, 1284, 1386, 1482`):**
  - `BPM` (`MonoLabel size="xs"`, `11px` mono `--color-muted`) -> `128.0` (`--font-mono`, `--text-md: 13px`, `--color-text`)
  - `KEY` -> `A Minor (8A)` (`--font-mono`, `--text-md: 13px`, `--color-text`)
  - `TIME` -> `07:02` (`--font-mono`, `--text-md: 13px`, `--color-text`)
  - `RELEASE` -> `2026` (`--font-mono`, `--text-md: 13px`, `--color-text`)
- **Row 2 (`y = 221..244`, 3 columns at `x = 1180, 1284, 1386`):**
  - `GENRE` (`MonoLabel size="xs"`) -> `Techno` (`--font-sans`, `--text-md: 13px`, `--color-text`)
  - `SUBGENRE` -> `Hypnotic` (`--font-sans`, `--text-md: 13px`, `--color-text`)
  - `LABEL` -> `Ostgut Ton` (`--font-sans`, `--text-md: 13px`, `--color-text`)

### Section C — `TAGS` (`y = 258..320`)

- Header (`y = 270..277`): `MonoLabel` `TAGS` (`--text-xs: 11px`, `--color-text-2`).
- Chip row (`y = 290..316`): flex-wrap row (`6px` gap) of `DataTag variant="removable"` (`height: var(--control-height-sm)` `26px`, `border-radius: var(--radius-sm)` `6px`, `--font-sans`, `--text-sm: 12px`: `Hypnotic ×`, `Peak Time ×`, `Industrial ×`, `Rolling Bass ×`) followed by a circular `+` button (`Button variant="circle" size="sm"`, `26 x 26 px` at `x = 1541..1567`, `--radius-pill`). Wired to tag staging and file writes in `M3`.

### Section D — `AUDIO TRAITS` (`y = 322..435`)

- Header (`y = 332..340`): `MonoLabel` `AUDIO TRAITS` with `infoTooltip` (`13px` `ⓘ` icon).
- Five `TraitBar.svelte` rows (`y = 350, 367, 383, 400, 416`, `17px` vertical row pitch):
  - Left label at `x = 1180` (`135px` wide, `--font-sans`, `--text-sm: 12px`, `--color-text-2`): `Danceability`, `Energy`, `Warmth`, `Vocal Presence`, `Groove`.
  - Numeric readout at `x = 1319..1339` (`--font-mono`, `--text-sm: 12px`, `tabular-nums`, `--color-text`: `0.82`, `0.78`, `0.64`, `0.12`, `0.91`).
  - Bar track at `x = 1351..1568` (`218 x 5 px`, `--radius-xs: 3px`, `background: var(--color-surface-2)`, fill in `--color-text`). Populated by `M5` analysis and `M6` embeddings.

### Section E — Energy curve (`y = 437..588`)

- Header pill (`y = 446..468`): `FilterPill label="Energy" active={true}` (`x = 1180..1249`, `70 x 23 px`, `--radius-pill`, inverted `--color-text` fill). The adjacent `Tension` pill shown in `concept-dark.png` (`x = 1258..1325`) is omitted per Issue [#49](https://github.com/BradHudsonGrant/RESONANCE/issues/49).
- Curve plot (`Waveform.svelte` `mode="curve"`, `x = 1185..1569, y = 486..552`, `385 x 67 px`):
  - Per Issue [#49](https://github.com/BradHudsonGrant/RESONANCE/issues/49), renders as a flat `1px` monochrome bar/step plot (`--color-text`) over a `1px` `--color-border` axis line at `y = 552`, with no gradient area fill under the curve.
  - Five evenly spaced timestamps below the baseline (`y = 565..572`, `x = 1185..1569`): `0:00`, `1:45`, `3:30`, `5:15`, `7:02` (`--font-mono`, `--text-xs: 11px`, `--color-muted`). Wired to real phrase and loudness analysis in `M5`.

### Section F — `TRANSITION SUGGESTIONS` (`y = 590..752`)

- Header (`y = 604..611`): `MonoLabel` `TRANSITION SUGGESTIONS` with `infoTooltip` (`ⓘ`) and right-aligned `actionLabel="View All ->"` (`x = 1500..1568`).
- Three compact suggestion rows (`y = 626..653`, `663..689`, `700..726`, `28px` height, `37px` vertical pitch):
  1. Left preview play button (`Button variant="icon" size="sm"`, `x = 1184..1208`, `24 x 24 px`, `--radius-xs: 3px`, `--color-surface-2`, `1px solid var(--color-border)`, `10px` `Icon name="play"`).
  2. Centre two-line stack (`x = 1218..1490`):
     - Line 1: `Title — Artist` (`Luminous — Vera Holt`, `Subzero — Ben Klock`, `Pulse Width — Surgeon`, `--font-sans`, `font-weight: 500`, `--text-md: 13px`, `--color-text`).
     - Line 2: compatibility reason (`126.5 BPM · D min · Key + Energy match`, `--font-mono`, `--text-xs: 11px`, `--color-text-2`).
  3. Right match score badge (`DataTag variant="score"`, `x = 1501..1543`, `43 x 21 px`, `--radius-sm: 6px`, `--font-mono`, `--text-xs: 11px`, `--color-text`: `95%`, `92%`, `89%`) and overflow `...` button (`x = 1550`). Wired to `M6` similarity and `M10` transition scoring.
