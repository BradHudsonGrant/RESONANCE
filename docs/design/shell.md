# App shell

The shell frames every workspace in Resonance (`ui/src/App.svelte` and `ui/src/lib/shell/`). Issue [#51](https://github.com/BradHudsonGrant/RESONANCE/issues/51) rebuilds it from M0's 52 px icon rail and 44 px header to match `concept-dark.png` in both dark and light themes.

## Grid and zones

The outer grid fills `100vh` with `overflow: hidden` on `--color-background` (`#0B0B0C` dark / `#F8F7F5` light):

| Zone | Dimensions | Background (Dark / Light) | Border | Milestone |
| --- | --- | --- | --- | --- |
| Top bar | `40px` height, full width | `#0B0B0C` / `#F8F7F5` (`--color-background`) | `1px` bottom `--color-border` | `#51` |
| Left sidebar | `220px` wide open, `52px` collapsed | `#141415` / `#FFFFFF` (`--color-surface`) | `1px` right `--color-border` | `#51` (`Collections` M2, `Active set` M10) |
| Command block | `auto` height (`~86px`), top of centre column | `#0B0B0C` / `#F8F7F5` (`--color-background`) | `1px` bottom `--color-border` | `#51` (search M2, natural language M6) |
| Workspace | `1fr` width and height, centre column | `#0B0B0C` / `#F8F7F5` (`--color-background`) | None | `#52` (`Library`, `Mix`) |
| Right inspector | `320px` wide when open, `0px` when hidden | `#141415` / `#FFFFFF` (`--color-surface`) | `1px` left `--color-border` | `#51` frame; `#52` track facts |
| A to B dock | `32px` header + `104px` body (`136px` total), full width | `#141415` / `#FFFFFF` (`--color-surface`) | `1px` top `--color-border` | `#51` frame; M2 preview; M10 crossfade |
| Player bar | `48px` height, full width | `#141415` / `#FFFFFF` (`--color-surface`) | `1px` top `--color-border` | `#51` frame; M2 preview; M7 decks |

## Top window bar

- **Height and padding:** `40px` high, `0 16px` horizontal padding, flex row with `justify-content: space-between` and `align-items: center`.
- **Left side:** `Wordmark` (`Mark` at `20 x 20 px` with `--color-accent` centre bar and `--color-text` outer six bars, `8px` gap, `RESONANCE` in `Space Grotesk` `13px` weight `500` uppercase tracked `0.32em`). No `MUSIC INTELLIGENCE FOR DJS` subtitle and no macOS traffic-light buttons; Windows uses the native window frame.
- **Right side:** Quiet icon buttons (`28 x 28 px`, `--radius-control: 6px`, `16px` Lucide icons in `--color-text-2`):
  - Sidebar collapse toggle (`PanelLeft`).
  - Inspector toggle (`PanelRight`, shortcut `I`, active state shows `2px` `--color-accent` bottom indicator or `--color-surface-2` background).
  - Engine status indicator (`Activity` / `Radio` mark) when the Mix audio device is open.
- **What stays out:** `concept-dark.png` shows top-right Settings and profile icons. Resonance places `Settings` and `Activity` once at the bottom of the sidebar so each job has one home.

## Left sidebar

Replaces `ui/src/lib/shell/Rail.svelte`. Open at `220px` by default and collapses to `52px` (icon-only with right-side tooltips, matching the M0 rail width).

### Navigation rows

Each row (`34px` height, `padding: 0 10px`, `gap: 10px`, `--radius-control: 6px`, `border-left: 2px solid transparent`) carries a `16px` Lucide icon, a `13px` `Inter` label (`--text-md`), and an optional right-aligned `12px` `JetBrains Mono` count (`tabular-nums`, `--color-text-2`).

- **Default built space (`Library`, `Mix`):** `--color-text-2` (`#A9A8A6` dark / `#595754` light). Hover sets `--color-surface-2` (`#1E1E1F` dark / `#ECEAE7` light) and `--color-text`.
- **Active space (`aria-current="page"`):** `border-left-color: var(--color-accent)` (`#E4553D` dark / `#D94F3B` light), `--color-text` label and icon, weight `500`, background `--color-raised` in dark (`#252527`) and `--color-surface-2` in light (`#ECEAE7`).
- **Unbuilt space (`aria-disabled="true"`):** `--color-muted` (`#8C8C8C` dark / `#6C6965` light), no hover background, and a tooltip naming the milestone that brings it (`Prep arrives in M3`, `Sets arrives in M10`, `Replay arrives in M9`, `Activity arrives in M2`, `Settings arrives in M2`).

### Sidebar groups

1. **Five spaces (`#51`):**
   - `Library` (Lucide `Library`) — built (`#52`), shows total track count (`50` or `12,846`).
   - `Prep` (Lucide `Layers`) — replaces `Inbox`, taking its name from `concept-dark.png`'s `Preparation`. Arrives in M3 Identify and tag.
   - `Sets` (Lucide `ListMusic`) — arrives in M10 Sets and transitions.
   - `Mix` (Lucide `Disc3`) — built (`#52`).
   - `Replay` (Lucide `ChartLine`) — replaces `Review`. Arrives in M9 Review.
2. **Collections under `Library` (M2):**
   - Header row (`Collections`, folder icon, `+` button at `24 x 24 px`) and indented `28px` rows (`padding-left: 24px`, `12px` `Inter` label, `11px` mono count). Arrives in M2 Library; omitted in `#51` so empty placeholder folders do not clutter the sidebar.
3. **Active set block (M10):**
   - Separated above and below by `1px solid var(--color-border)`, `padding: 12px 10px`. Header `ACTIVE SET` (mono uppercase `11px` `--color-muted`), row with a `6 x 6 px` `--color-accent` dot, set name (`Inter` `12px` `--color-text`) and `...` button. Arrives in M10 Sets and transitions.
4. **Bottom tools (`#51`):**
   - Pinned to the bottom of the sidebar above a `1px solid var(--color-border)` rule: `Activity` (Lucide `Activity`, M2 persistent queue and undo history) and `Settings` (Lucide `Settings`, M2 sources and preferences). The decorative dot wave at the bottom-left of `concept-dark.png` is left out so `Activity` and `Settings` sit there cleanly.

### Board navigation items left out

`concept-dark.png` lists `Intelligence`, `Playlists`, `History` and `Analysis`. Board 2 `[06]` and `[08]`/`[09]` list `Listen`, `Explore`, `For You` and `Profile`. Resonance leaves these out:

- `Intelligence`, `Analysis` and track/session `History` live on the tracks and sessions they describe (`Inspector`, `Activity`, `Replay`), never as separate pages.
- `Playlists` merge into `Library > Collections` (M2) and external library mirrors (M11).
- `Listen`, `Explore`, `For You` and `Profile` are consumer-streaming sections that do not fit a local DJ workspace.

## Command bar and slash-command strip

Sits across the full width of the centre column above the workspace (`padding: 12px 16px 10px`, `--color-background`, `1px solid var(--color-border)` bottom divider).

- **Search and command input (`#51`):**
  - `36px` tall (`--control-height-lg`), `--radius-panel` (`8px`), `--color-surface` fill (`#141415` dark / `#FFFFFF` light), `1px solid var(--color-border-control)` (`#6E6E71` dark / `#8A8681` light).
  - Left `16px` `Search` icon (`--color-muted`), input text in `Inter` `13px` `--color-text` (placeholder `"Search your library, or type a command..."` in `--color-muted`), and right-side key cap `Ctrl K` (`22px` high, `padding: 0 6px`, `--radius-sm: 4px`, `--color-surface-2`, `1px solid var(--color-border)`, `JetBrains Mono` `11px` `--color-text-2`).
  - Focusing or pressing `Ctrl+K` or `/` opens the `role="listbox"` command menu below (`--color-raised`, `1px solid var(--color-border-control)`, `--radius-panel: 8px`, `30px` rows with `--color-surface-2` active highlight and right-aligned mono shortcut).
  - Text search over tags arrives in **M2 Library**; natural-language queries with editable chips arrive in **M6 Discovery**. In `#51`, typing filters available commands.
- **Slash-command strip (`#51`):**
  - Flex row (`margin-top: 8px`, `gap: 8px`, `flex-wrap: wrap`).
  - Each chip is a `26px` button (`--radius-control: 6px`, `--color-surface`, `1px solid var(--color-border)`, `padding: 0 8px 0 4px`, `gap: 8px`) pairing a mono command badge (`20px` high, `padding: 0 6px`, `--radius-sm: 4px`, `--color-surface-2`, `JetBrains Mono` `11px` `--color-text`) with its description (`Inter` `12px` `--color-text-2`).
  - **Built in `#51`:** `/ scan` (`Scan a music folder`), `/ mix` (`Go to Mix`), `/ library` (`Go to Library`), `/ inspector` (`Toggle inspector`), and `...` (opens the full command list).
  - **Scheduled for later milestones (shown disabled with milestone tooltips, or added as each lands):** `/ tag` (`Add tags`, M3), `/ analyse` (`Analyse tracks`, M5 — British spelling replaces board's `/ analyze`), `/ match` (`Find compatible tracks`, M6), `/ set` (`Build a set`, M10 — replaces `/ flow`).

## Inspector frame

Right-hand `<aside aria-label="Inspector">` (`320px` wide, `--color-surface`, `1px solid var(--color-border)` left border). Opens by default on `Library`, collapses with `I` or the header button, and hides automatically in `Mix` so the stacked waveforms take the full width. Its sections and milestone schedule are specified in [library.md](library.md).

## A to B transition dock

`<section aria-label="Transition dock">` spans the full window width directly above the player bar (`--color-surface`, `1px solid var(--color-border)` top border).

- **Header bar (`32px` height, `padding: 0 16px`):**
  - Left: `A -> B` (`Space Grotesk` `13px` weight `600` `--color-text`), `Transition` (`Inter` `12px` `--color-text-2`), `ⓘ` tooltip button (`14px`), and a collapse/expand chevron (`24 x 24 px`).
  - Centre: `Mix Point` readout/selector (`4:15 (60%)` in M10) and `Blend` selector (`Crossfade` in M10). In `#51`, these sit in `--color-muted` as `"Available in M10"`.
- **Expanded body (`104px` height, `padding: 12px 16px`, `display: grid`, `grid-template-columns: 240px 1fr 240px`, `gap: 16px`):**
  - **Deck A** and **Deck B** cards: `--color-surface-2` (`#1E1E1F` dark) or `--color-background` (`#F8F7F5` light), `1px solid var(--color-border)`, `--radius-panel: 8px`, `padding: 8px 10px`, `display: flex`, `align-items: center`, `gap: 10px`. Inside each card: a `28 x 28 px` letter badge (`A` / `B` on `--color-raised` with `1px solid var(--color-border)`, `Space Grotesk` `13px` weight `600`), a `44 x 44 px` `WaveThumb` (`--radius-control: 6px`), track title (`13px` `Inter` weight `500`), artist (`12px` `--color-text-2`), and mono metadata (`BPM · Key · Time`, `11px` `--color-muted`).
  - **Centre crossfade waveform slot:** `--color-background` fill, `1px solid var(--color-border)`, `--radius-panel: 8px`.
  - **M1 (`#51`) behaviour:** Deck A shows the currently selected library track (`WaveThumb`, title, artist, available metadata); Deck B shows an empty grid-dot slot (`"Waiting for Deck B"`). The centre slot states plainly `"Preview playback arrives in M2. A to B transition previews arrive in M10."` No fake waveform bars or blend curves are drawn.

## Player bar

`<footer aria-label="Player">` sits at the bottom (`48px` height, `padding: 0 16px`, `--color-surface`, `1px solid var(--color-border)` top border, `display: flex`, `align-items: center`, `gap: 16px`).

- **Left transport group (`#51` frame; functional in M2 preview / M7 decks):**
  - Circular play button (`32 x 32 px`, `--radius-pill`, `--color-surface-2`, `1px solid var(--color-border-control)`).
  - Pause button (`28 x 28 px`, `--radius-control: 6px`, quiet variant).
  - Tabular time readout (`0:00 / 0:00`, `JetBrains Mono` `12px` `--color-text-2`).
- **Centre progress group (`#51`):**
  - Scrub track (`flex: 1`, `4px` tall in `--color-surface-2`, active fill in `--color-text`, `10 x 10 px` circular thumb). Until M2 preview playback exists, shows `"Nothing playing"` (`--color-muted`) beside the selected track title and artist (`--color-text` / `--color-text-2`), matching M0's honest status rule, with the scrub bar disabled.
- **Right deck mixer controls (`#51` frame; functional in M7 decks):**
  - `A Vol` and `B Vol` micro sliders (`64 x 4 px` track, `JetBrains Mono` `11px` label and `100%` readout in `--color-muted`).
  - `Curve` (`S-curve`) and `EQ` (`Flat`) dropdown buttons (`28px` height, `--radius-control: 6px`, `--color-surface-2`, `1px solid var(--color-border)`).
  - Headphone cue icon button (`28 x 28 px`, Lucide `Headphones`).
  - In `#51`, these mixer controls remain disabled with tooltips naming M7 Decks.
- **Board 2 player elements left out:** `concept are and design 2.png` `[07]`–`[09]` shows `Shuffle`, `Repeat` and `Fullscreen` icons on its mini player. Shuffle and repeat are streaming-player controls and are left out; Resonance uses cue, volume, curve and EQ.
