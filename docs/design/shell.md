# Application Shell

The application shell (`ui/src/App.svelte` and `ui/src/lib/shell/`) frames every workspace with five persistent regions built from the [component system](components.md):

1. `TopBar.svelte` (`--topbar-height: 50px`, `y = 0..50` across the full `1586 px` window width)
2. `Sidebar.svelte` (`--sidebar-width: 256px`, `x = 0..256, y = 51..752`, collapsing to `--sidebar-collapsed-width: 52px`)
3. `CommandBar.svelte` (`y = 51..152` across the top of the centre column `x = 257..1164`)
4. `TransitionDock.svelte` (`y = 753..913` across `x = 0..1586`)
5. `PlayerBar.svelte` (`--player-height: 60px`, rounded card at `x = 20..1567, y = 915..974`, inset `20px` from the left and right window edges and `17px` from the bottom edge)

Every dimension below is measured from `concept-dark.png` (`1586 x 992` CSS pixels at device pixel ratio `1`) and expressed as CSS custom properties in `ui/src/lib/design/tokens.css`. Issue [#51](https://github.com/BradHudsonGrant/RESONANCE/issues/51) builds this shell.

## Root CSS Grid (`App.svelte`)

```
+-----------------------------------------------------------------------------------+
| TopBar (50px: Mark + RESONANCE 16px left; Audio, Activity, Settings icons right)  |
+----------------+----------------------------------------+-------------------------+
| Sidebar        | CommandBar (42px input + 22px chips)   | Inspector               |
| (256px open /  +----------------------------------------+ (420px at 1586px;       |
|  52px icon     | Active Workspace                       |  clamp(340px,26.5vw,    |
|  rail <1360px) | (Library, Prep, Sets, Mix, Replay)     |  440px))                |
|                |                                        |                         |
| [Bottom Wave]  |                                        |                         |
+----------------+----------------------------------------+-------------------------+
| TransitionDock (52px header + 340x87 Deck A | 844x101 Crossfade | 340x87 Deck B)  |
| +-------------------------------------------------------------------------------+ |
| | PlayerBar (60px rounded card, 12px radius, inset 20px L/R and 17px bottom)    | |
| +-------------------------------------------------------------------------------+ |
+-----------------------------------------------------------------------------------+
```

```css
.app-shell {
  display: grid;
  width: 100vw;
  height: 100vh;
  overflow: hidden;
  background: var(--color-background);
  color: var(--color-text);
  grid-template-columns:
    var(--shell-sidebar-col, var(--sidebar-width))
    minmax(520px, 1fr)
    var(--shell-inspector-col, clamp(var(--inspector-min-width), 26.5vw, var(--inspector-max-width)));
  grid-template-rows:
    var(--topbar-height)
    minmax(280px, 1fr)
    auto
    calc(var(--player-height) + var(--player-inset-bottom));
  grid-template-areas:
    "topbar  topbar     topbar"
    "sidebar workspace  inspector"
    "dock    dock       dock"
    "player  player     player";
}
```

## 1. Top header bar (`TopBar.svelte`)

Measured at `y = 0..49` (`50px` tall including its `1px` bottom border at `y = 50`, `--topbar-height: 50px`), spanning the full window width (`1586 px`):

- **Surface:** `background: var(--color-surface)` (`#141415` dark / `#FAF9F7` light), `border-bottom: 1px solid var(--color-border)`.
- **Left brand cluster (`Wordmark.svelte`):**
  - 7-bar asymmetric `Mark` (`size="md"`, `19 x 21 px`, `y = 15..35`, `--color-text`).
  - `18px` gap (`--space-4`).
  - `RESONANCE` wordmark (`--font-display`, `font-weight: 500`, `--text-lg: 16px`, cap height `12px` at `y = 19..31, x = 133..265`, `letter-spacing: 0.32em`, `--color-text`).
- **Right utility cluster (`x = 1450..1568`, `y = 11..39`):** Three `Button variant="icon"` (`28 x 28 px` hit target, `--control-height: 28px`, `--radius-sm: 6px`, `18 x 18 px` `Icon size="md"`, `color: var(--color-text-2)`, `hover: var(--color-text)`) spaced `30px` centre-to-centre (`18px` gap between buttons):
  1. **Audio engine status (`x = 1454..1470`, `icon="audio"`):** Opens audio device and latency popover (wired to live engine metrics in `M7`).
  2. **Activity (`x = 1502..1519`, `icon="activity"`):** Opens the background scan and analysis job drawer (wired to the persistent job queue in `M2`).
  3. **Settings (`x = 1549..1565`, `icon="settings"`):** Opens the settings sheet (theme switcher Dark / Light / System in `#51`; library sources in `M2`; provider keys in `M3`; audio/MIDI devices in `M7`).

## 2. Collapsible left sidebar (`Sidebar.svelte`)

Measured at `x = 0..255` (`256px` wide including its `1px` right border at `x = 256`, `--sidebar-width: 256px`), spanning `y = 51..752` (`702px` tall at the reference window):

- **Surface:** `background: var(--color-surface)`, `border-right: 1px solid var(--color-border)`.
- **Top subtitle (`y = 75..97, x = 25..195`):** Two-line `MonoLabel` reading:
  ```
  MUSIC INTELLIGENCE
  FOR DJS
  ```
  Rendered in `--font-mono`, `--text-xs: 11px` (cap height `8px`), `font-weight: 500`, `letter-spacing: 0.08em`, `line-height: 15px`, `--color-text-2`, alongside a quiet collapse button (`Button variant="icon"`, shortcut `[`).
- **Primary space navigation (`x = 15..246`, button width `232px`, height `--control-height-nav: 38px`, `41–42px` vertical row pitch):**
  - **Active item (`Library` at `y = 125..162`):** `background: var(--color-raised)` (`#252527` dark / `#ECEAE7` light), `border: 1px solid var(--color-border)`, `border-radius: var(--radius-md)` (`8px`), `2px` vertical `--color-accent` selection mark on the left edge, `16 x 16 px` `Icon size="sm"` at `x = 31..46`, label `Library` at `x = 66` (`--font-sans`, `font-weight: 500`, `--text-base: 14px`, `--color-text`), and right-aligned tabular track count `12,846` at `x = 198..232` (`--font-mono`, `--text-sm: 12px`, `--color-text-2`).
  - **Collapsible `Collections` sub-tree (`y = 180..308`, wired in `M2`, shell placeholder in `#51`):**
    - Group row (`y = 180..195`): `16px` folder/collection icon at `x = 31`, `Collections` label at `x = 66` (`--font-sans`, `--text-base: 14px`, `--color-text-2`), and a `24 x 24 px` `+` `Button variant="icon" size="sm"` at `x = 220..244`.
    - Indented collection rows (`y = 219, 246, 273, 300`, `27px` pitch): label at `x = 80` (`--font-sans`, `--text-sm: 12px`, `--color-text-2`: `Peak Time`, `Warm Up`, `Afterhours`, `Deep / Hypnotic`) and right-aligned count at `x = 212..232` (`--font-mono`, `--text-sm: 12px`, `--color-muted`: `248`, `176`, `312`, `409`).
  - **Remaining four primary spaces (`y = 340, 382, 424, 466`, `38px` button height, `42px` pitch):**
    Per Issue [#49](https://github.com/BradHudsonGrant/RESONANCE/issues/49), the sidebar's five spaces are:
    1. `Library` (`1` / `Ctrl+1`) — active in `M1`/`M2`.
    2. `Prep` (`2` / `Ctrl+2`) — `Inbox`, `Duplicates`, `Quality` (`M3` and `M4`).
    3. `Sets` (`3` / `Ctrl+3`) — collection cards and set timeline (`M10`).
    4. `Mix` (`4` / `Ctrl+4`) — two-deck live performance and coaching (`#52`, `M7`, `M8`).
    5. `Replay` (`5` / `Ctrl+5`) — recorded session review (`M9`).
    Inactive space buttons render with transparent background, `16px` `Icon` at `x = 31` (`--color-text-2`), and `14px` `--font-sans` label at `x = 66` (`--color-text-2`, `--color-text` on hover).
- **Active Set summary block (`y = 562..620`, wired in `M10`, static placeholder in `#51`):**
  - `1px` horizontal `ModularDivider` at `y = 562` (`x = 20..240`).
  - `MonoLabel` `ACTIVE SET` at `y = 580..587` (`--text-xs: 11px`, `--color-muted`).
  - Set row at `y = 603..615`: `6 x 6 px` monochrome status dot (`--color-text-2`, replacing the green dot on the board) at `x = 28`, label `Fri 23:00 — Warehouse` at `x = 46` (`--font-sans`, `--text-md: 13px`, `--color-text`), and `...` button at `x = 225`.
- **Bottom dot-wave art (`Wave.svelte` `variant="sidebar"`, `x = 1..254, y = 655..732`):**
  - A `254 x 78 px` (`--sidebar-wave-height: 78px`) WebGL2 dot-wave field anchored to the bottom of the sidebar with a top CSS gradient mask fading into `--color-surface`.
- **Collapsed state (`--sidebar-collapsed-width: 52px`):**
  - Automatically engages below `1360px` window width or when toggled via `[`.
  - Shows centered `38 x 38 px` icon buttons for the five spaces (`Library`, `Prep`, `Sets`, `Mix`, `Replay`) with instant `Tooltip` labels on hover/focus; hides the `MUSIC INTELLIGENCE FOR DJS` text, collection sub-items, active set text, and bottom wave canvas.

## 3. Search and command bar (`CommandBar.svelte`)

Measured across the top of the centre column (`x = 257..1164`, width `908px` at the reference viewport, `y = 51..152`):

- **Search input box (`TextInput size="lg"`, `x = 276..1130`, `y = 63..104`, `855 x 42 px`):**
  - `height: var(--control-height-lg)` (`42px`), `margin-top: var(--space-3)` (`12px` below `TopBar`), `padding: 0 var(--space-3-5) 0 var(--space-4)`.
  - `background: var(--color-surface)`, `border: 1px solid var(--color-border-control)`, `border-radius: var(--radius-lg)` (`10px`).
  - Leading `16 x 16 px` `Icon name="search"` at `x = 298` (`--color-text-2`), input text / placeholder `"Search your library, or type a command..."` at `x = 332` (`--font-sans`, `--text-base: 14px`, `--color-text` / `--color-muted`), and trailing shortcut badge (`DataTag variant="keybadge"`, `46 x 24 px` at `x = 1070..1115, y = 72..95`, `border-radius: var(--radius-sm)` `6px`, `--font-mono`, `--text-xs: 11px`, showing `Ctrl K` on Windows/Linux and `⌘ K` on macOS).
- **Slash-command chip strip (`y = 116..138`, `12px` below search box, closed by a `1px` `--color-border` divider at `y = 152`):**
  - Horizontal row of `SlashChip` components (`22px` tall code badge with `6px` radius + `10px` gap + `12px` sans description) spaced `24px` apart, with a trailing `...` overflow button (`x = 1112`):
    1. `/ bpm 125-132` — `Filter by tempo` (`x = 291`, wired in `M2`)
    2. `/ key 8A` — `Compatible with A minor` (`x = 512`, wired in `M5`)
    3. `/ similar` — `Find matching tracks` (`x = 756`, wired in `M6`)
    4. `/ build set` — `Generate 90m progression` (`x = 924`, wired in `M10`)
  - Clicking a `SlashChip` in `#51` inserts its command prefix into the search input and focuses it.

## 4. Bottom `A -> B` transition dock (`TransitionDock.svelte`)

Measured at `y = 753..913` (`160px` tall above `PlayerBar`), separated from the upper columns by a full-width `1px` `--color-border` line at `y = 753`:

- **Dock header row (`y = 754..806`, height `--dock-header-height: 52px`, horizontal padding `20px`):**
  - Left: `A -> B` (`x = 28..85, y = 774..786`, `--font-display`, `font-weight: 600`, `--text-lg: 16px`, `--color-text`), `Transition` (`x = 96..162`, `--font-sans`, `--text-md: 13px`, `--color-text-2`), and a `13px` `ⓘ` info icon (`x = 168`).
  - Centre controls:
    - `Mix Point` (`--font-sans`, `--text-sm: 12px`, `--color-text-2`) + `4:15 (60%) v` (`Button variant="quiet" size="xs"`, `--font-mono`, `--text-sm: 12px`, `--color-text`, `x = 622..765`).
    - `Blend` (`--font-sans`, `--text-sm: 12px`, `--color-text-2`) + `Crossfade v` (`Button variant="quiet" size="xs"`, `--font-sans`, `--text-sm: 12px`, `--color-text`, `x = 860..985`).
- **Three-column dock body (`y = 801..901`, `padding: 0 var(--player-inset-x)` `20px`, `gap: 13px`):**
  ```css
  grid-template-columns:
    clamp(260px, 21.5vw, var(--dock-card-width))
    minmax(420px, 1fr)
    clamp(260px, 21.5vw, var(--dock-card-width));
  ```
  1. **Deck A pod (`Deck.svelte` `variant="dock" deck="A"`, `x = 20..358, y = 807..893`, `339 x 87 px`):**
     - Built on `Card variant="surface" radius="lg"` (`--radius-lg: 10px`, `1px solid var(--color-border)`, `--color-surface`).
     - Deck badge `A` (`x = 34..69, y = 834..867`, `35 x 34 px`, `--radius-sm: 6px`, `--color-surface-2`, `1px solid var(--color-border)`, `--font-display` 600 `--text-lg: 16px`).
     - Artwork `Thumb size="md"` (`x = 82..142, y = 821..881`, `60 x 60 px`, `--radius-md: 8px`).
     - Track text stack (`x = 154`): title `Echo Chamber` (`--font-display` 500, `--text-base: 14px`, `--color-text`), artist `Kaden` (`--font-sans`, `--text-sm: 12px`, `--color-text-2`), and mono metadata `128.0 BPM · A min · 7:02` (`--font-mono`, `--text-xs: 11px`, `--color-text-2`).
     - Overflow `...` `Button variant="icon" size="sm"` (`24 x 24 px` at `x = 326, y = 815`).
  2. **Centre crossfade waveform (`Waveform.svelte` `mode="crossfade"`, `x = 371..1214, y = 801..901`, `844 x 101 px` at reference viewport):**
     - Draws `1px` vertical waveform bars (`1px` gap) for outgoing Deck A (`--color-text` fading out left-to-right) and incoming Deck B (`--color-text-2` rising to `--color-text` left-to-right).
     - Overlays two `1.5px` monochrome S-curve volume envelopes showing the crossfade blend window.
     - Vertical `1px` playhead line at `x = 856` (`y = 801..901`) topped with a `5 x 5 px` square cap (`--color-accent` while playing or scrubbing, `--color-text` when paused).
  3. **Deck B pod (`Deck.svelte` `variant="dock" deck="B"`, `x = 1228..1567, y = 807..893`, `340 x 87 px`):**
     - Mirrors Deck A with badge `B`, `60 x 60 px` `Thumb size="md"`, title `Luminous`, artist `Vera Holt`, metadata `126.5 BPM · D min · 6:48`, and `...` button.

## 5. Inset bottom player card (`PlayerBar.svelte`)

In `concept-dark.png`, the player bar is **not** a flat full-width strip: it is a rounded, bordered card (`Card variant="surface" radius="xl"`) measured at `x = 20..1567` (`1548 px` wide, inset `20px` left and `19px` right from the `1586px` window edges) and `y = 915..974` (`60px` tall, `--player-height: 60px`, inset `17px` from the bottom edge `y = 992`):

- **Container:** `height: var(--player-height)` (`60px`), `margin: 0 var(--player-inset-x) var(--player-inset-bottom)`, `background: var(--color-surface)` (`#141415` dark / `#FAF9F7` light), `border: 1px solid var(--color-border)`, `border-radius: var(--radius-xl)` (`12px`), `padding: 0 var(--space-4)` (`16px`).
- **Left transport cluster:**
  - Circular Play button (`Button variant="circle" size="lg"`, `x = 35..78, y = 924..967`, `44 x 44 px`, `--control-circle-lg: 44px`, `border-radius: var(--radius-pill)`, `background: var(--color-surface-2)`, `border: 1px solid var(--color-border-control)`, `16px` filled play triangle in `--color-text`).
  - Pause button (`Button variant="icon"`, `x = 96..124, y = 931..959`, `28 x 28 px` hit target, `12 x 16 px` double pause bars in `--color-text-2`).
  - Elapsed / total time (`x = 160..226, y = 939..950`): `4:15 / 7:02` in `--font-mono`, `--text-sm: 12px`, `tabular-nums` (`4:15` in `--color-text`, `/ 7:02` in `--color-text-2`).
- **Centre timeline scrub bar (`ScrubBar size="md"`, `x = 269..655, y = 939..950`, `387 px` wide at reference viewport, `minmax(180px, 1fr)`):**
  - `4px` tall track (`y = 943..946`, `--color-surface-2`, `border-radius: 2px`), elapsed fill in `--color-text`, and a `12 x 12 px` circular thumb (`x = 494..505, y = 939..950`, `--color-text` with `2px` `--color-surface` ring).
- **Right deck volume & mixer controls (`x = 704..1553`):**
  - `A Vol  100%` (`x = 704..786`, `--font-mono`, `--text-sm: 12px`, `A Vol` in `--color-text-2`, `100%` in `--color-text`) + `ScrubBar size="sm"` (`x = 802..887`, `86 px` wide, `4px` track, `10 x 10 px` thumb).
  - `B Vol  100%` (`x = 924..1006`, `--font-mono`, `--text-sm: 12px`) + `ScrubBar size="sm"` (`x = 1023..1108`, `86 px` wide, `4px` track, `10 x 10 px` thumb).
  - `Curve` label (`x = 1174..1205`, `--font-sans`, `--text-sm: 12px`, `--color-text-2`) + curve selector (`Button variant="select" size="sm"`, `x = 1220..1282, y = 933..958`, `63 x 26 px`, `--radius-sm: 6px`, showing a `16px` S-curve icon + chevron).
  - `EQ` label (`x = 1364..1378`, `--font-sans`, `--text-sm: 12px`, `--color-text-2`) + `Flat v` selector (`Button variant="select" size="sm"`, `x = 1391..1468, y = 933..958`, `78 x 26 px`, `--radius-sm: 6px`, `--font-sans`, `--text-sm: 12px`).
  - Vertical `ModularDivider` (`x = 1507, y = 930..962`, `1 x 33 px`, `--color-border`).
  - Headphone cue button (`Button variant="icon"`, `x = 1518..1553, y = 931..959`, `36 x 28 px`, `--radius-sm: 6px`, `18 x 17 px` `Icon name="headphones"`).
- **Milestone scope:** Issue `#51` builds the complete `TransitionDock` and `PlayerBar` UI with deterministic fixture state; `M2` wires single-track preview playback (`Space`, scrub bar, volume); `M7` and `M10` wire live two-deck crossfade audio, `Mix Point`, `Blend`, `Curve`, `EQ` and headphone cueing.
