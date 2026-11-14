# Component System

Resonance's UI is built from a five-layer component system rather than fixed per-screen templates. Issues [#50](https://github.com/BradHudsonGrant/RESONANCE/issues/50), [#51](https://github.com/BradHudsonGrant/RESONANCE/issues/51) and [#52](https://github.com/BradHudsonGrant/RESONANCE/issues/52) build directly from this catalogue, and milestones M2 through M12 wire real Rust IPC data into the same components.

## Layer architecture

```
Layer 0: Tokens       ui/src/lib/design/tokens.css
                      ↓
Layer 1: Primitives   ui/src/lib/design/*.svelte
                      ↓
Layer 2: Composed     ui/src/lib/wave/, ui/src/lib/library/, ui/src/lib/mix/
                      ↓
Layer 3: Regions      ui/src/lib/shell/, ui/src/lib/library/Inspector.svelte
                      ↓
Layer 4: Screens      ui/src/App.svelte, Library.svelte, Mix.svelte, DesignPage.svelte
```

### Three rules for every component

1. **One component per concept; variants are props, never copies.** Every element on `concept-dark.png` and `concept are and design 2.png` maps to one PascalCase Svelte 5 component (`one file = one component`). Visual differences between placements are controlled by typed props (`size`, `variant`, `mode`, `active`), never by duplicating markup or styles across screens.
   - `MonoLabel.svelte` renders the sidebar subtitle (`MUSIC INTELLIGENCE FOR DJS`), sidebar section headers (`ACTIVE SET`), inspector metadata keys (`BPM`, `KEY`, `TIME`, `RELEASE`, `GENRE`, `SUBGENRE`, `LABEL`), inspector section headers (`TAGS`, `AUDIO TRAITS`, `TRANSITION SUGGESTIONS`), and Board 2 panel labels.
   - `DataTag.svelte` renders the inspector `✓ Analysed` status pill (`variant="pill"`), removable tag chips (`variant="removable"`), transition match percentages (`variant="score"`), keyboard shortcut badges (`variant="keybadge"`), and Board 2 metadata pills (`128.0 BPM`, `8A`, `WAV 24-bit`, `-7.4 LUFS`).
   - `Thumb.svelte` renders square bordered artwork with generative `Wave` fallback in `Inspector` (`size="lg"`, `96 px`), Deck A and B pods (`size="md"`, `60 px`), and compact track cards (`size="sm"`, `36 px`).
   - `Card.svelte` renders the bordered containers for Deck A and B pods (`variant="surface", radius="lg"`), the floating bottom player bar (`variant="surface", radius="xl"`), Sets collection cards (`variant="interactive", radius="lg"`), the `+ New Collection` slot (`variant="dashed", radius="lg"`), and Prep track cards (`variant="surface", radius="md"`).
   - `Waveform.svelte` renders `1px` vertical bar waveforms in `TrackTable` (`mode="inline"`), `TransitionDock` (`mode="crossfade"`), `Mix` decks (`mode="deck"`), and the `Inspector` energy curve (`mode="curve"`).
   - `Deck.svelte` renders both the compact Deck A and B cards in `TransitionDock` (`variant="dock"`, `340 x 87 px`) and the full Deck A and B strips in `Mix` (`variant="full"`).
2. **Zero raw values inside components.** Every width, height, padding, gap, corner radius, colour, font family, font size and duration inside a `.svelte` file must reference a CSS custom property from `ui/src/lib/design/tokens.css` (see [visual-language.md](visual-language.md)). Pixel measurements taken from `concept-dark.png` set the token values in `tokens.css`; they are never copied as raw numbers into component CSS.
3. **Domain names only.** Components are PascalCase `.svelte` files named after what they are in the domain (`Deck`, `Waveform`, `Inspector`, `TrackTable`, `MonoLabel`, `ScrubBar`). Supporting TypeScript modules use lowercase domain nouns (`clock.ts`, `frames.ts`, `columns.ts`, `format.ts`, `provenance.ts`). Never name a file or module `utils`, `helpers`, `manager` or `service`.

## Folder layout (reconciling `docs/architecture.md` with `ui/src/lib/`)

[docs/architecture.md](../architecture.md#repository-layout) summarises the UI folder tree as `design/`, `components/`, `spaces/` and `ipc/`. In the working tree built in M0, domain components and their space views live together by domain (`ui/src/lib/library/`, `ui/src/lib/mix/`, `ui/src/lib/shell/`, `ui/src/lib/wave/`) so each domain slice keeps its view, composed components, state and tests in one folder.

To keep existing imports and tests clean while matching the architecture rules, `ui/src/` uses the following layout for M1 (`#50`–`#52`) and later milestones:

```
ui/src/
  App.svelte                        Layer 4: root responsive CSS Grid shell
  main.ts                           App entry point
  design-page/
    DesignPage.svelte               Layer 4: /design.html verification specimen (#50)
    Specimen.svelte                 Specimen section wrapper for /design.html
    main.ts                         /design.html entry point
  lib/
    design/                         Layer 0 (tokens) & Layer 1 (primitives, #50)
      tokens.css                    All colour, font, size, spacing, radius and layout tokens
      base.css                      Reset, typography defaults, focus ring rules
      design.css                    Entry stylesheet importing tokens.css and base.css
      logo/
        mark.svg                    7-bar asymmetric signal mark vector
        app-icon.svg                Application icon vector
      Mark.svelte                   7-bar signal mark (existing, update in #50)
      Wordmark.svelte               RESONANCE tracked wordmark (existing, update in #50)
      Icon.svelte                   1.5px stroke monochrome icons (existing, expand in #50)
      Button.svelte                 Action and icon buttons (existing, update in #50)
      TextInput.svelte              Search and text input (existing, update in #50)
      MonoLabel.svelte              NEW (#50): uppercase tracked monospace header/key label
      DataTag.svelte                NEW (#50): pill, removable, score and keybadge tags
      SlashChip.svelte              NEW (#50): slash-command code badge + description
      FilterPill.svelte             NEW (#50): selectable pill with inverted active fill
      SegmentedTabs.svelte          NEW (#50): bordered segmented tab bar
      Thumb.svelte                  NEW (#50): 36/60/96px bordered artwork with Wave fallback
      Card.svelte                   NEW (#50): surface, raised, interactive and dashed cards
      MeterBar.svelte               NEW (#50): 5-bar vertical discrete energy indicator
      LevelDots.svelte              NEW (#50): 4-dot horizontal stem level indicator
      TraitBar.svelte               NEW (#50): labelled horizontal audio trait meter
      ScrubBar.svelte               NEW (#50): timeline scrub bar and compact volume slider
      FocusBrackets.svelte          NEW (#50): 4-corner L-shaped focus/drop-target brackets
      IndexMarks.svelte             NEW (#50): bracketed [01] monospace index prefix
      ModularDivider.svelte         NEW (#50): 1px rule with optional end ticks or label
      GridDots.svelte               NEW (#50): 1px dot matrix background canvas
      Provenance.svelte             Source & confidence badge (existing, update in #50)
      Panel.svelte                  Bordered panel container (existing, update in #50)
      Sheet.svelte                  Slide-over drawer / modal sheet (existing, update in #50)
      Tooltip.svelte                Hover/focus popover tooltip (existing, update in #50)
      TableRow.svelte               33px table row with 6px selected frame (existing, update in #50)
      TableCell.svelte              Table cell primitive (existing, update in #50)
      provenance.ts                 Provenance types and formatting
      primitives.test.ts            Unit tests for Layer 1 primitives
    wave/                           Layer 2: WebGL2 dot-wave field
      Wave.svelte                   Generative particle wave (existing, add sidebar/card variants in #50/#51)
      renderer.ts                   WebGL2 point-sprite shader and buffer setup
      motion.ts                     Reduced-motion media query listener
      measure-wave.ts               Frame-time benchmark hook
    shell/                          Layer 3: persistent application shell regions (#51)
      TopBar.svelte                 NEW (#51): 50px window header (Mark, Wordmark, Activity, Settings)
      Sidebar.svelte                NEW (#51, replaces Rail.svelte): 256px/52px collapsible left sidebar
      CommandBar.svelte             Search box (42px) + SlashChip strip (existing, update in #51)
      TransitionDock.svelte         NEW (#51): bottom A -> B dock (header, two Deck cards, crossfade Waveform)
      PlayerBar.svelte              Inset 60px rounded player card (existing, update in #51)
    library/                        Layer 2, 3 & 4: Library domain components, Inspector & screen (#52)
      Library.svelte                Layer 4: Library workspace screen (existing, update in #52)
      TrackTable.svelte             Layer 2: virtualised 33px-row track table (existing, update in #52)
      Inspector.svelte              Layer 3: 420px right-hand track inspector (existing, update in #52)
      ScanFolder.svelte             Layer 2: folder source picker bar (existing, update in #52)
      columns.ts                    Column widths, alignment and responsive drop tiers
      format.ts                     BPM, key, duration and sample-rate formatters
      track-pages.svelte.ts         Paged track window cache over IPC
      fake-library.ts               Deterministic fixture tracks for M1 and browser tests
      measure-scroll.ts             100,000-row scroll benchmark hook
    mix/                            Layer 2 & 4: Mix domain components & screen (#52)
      Mix.svelte                    Layer 4: two-deck Mix workspace screen (existing, update in #52)
      Deck.svelte                   NEW (#51/#52): Deck A / Deck B card (dock variant & full mix variant)
      Waveform.svelte               NEW (#50/#52): 1px bar waveform (inline, crossfade, deck, curve modes)
      renderer.ts                   WebGL2 scrolling waveform and meter renderer
      clock.ts                      Audio clock extrapolation
      frames.ts                     Binary frame decoder
      probe.ts                      Latency probe
      measure-mix.ts                120 Hz Mix frame benchmark hook
    ipc/                            Generated Rust bindings and transport client
```

When future milestones add new spaces (`Prep` in M3, `Sets` in M10, `Replay` in M9), each gets its own domain folder under `ui/src/lib/` (`ui/src/lib/prep/`, `ui/src/lib/sets/`, `ui/src/lib/replay/`) following the same pattern as `ui/src/lib/library/` and `ui/src/lib/mix/`.

---

## Layer 1 — Primitives (`ui/src/lib/design/`)

All Layer 1 primitives are built or updated in [#50](https://github.com/BradHudsonGrant/RESONANCE/issues/50) and rendered side by side in dark, light and high-contrast themes on `/design.html` (`DesignPage.svelte`).

### `Mark.svelte`
- **Path:** `ui/src/lib/design/Mark.svelte`
- **Purpose:** Renders the 7-bar asymmetric Resonance signal mark (`35%, 60%, 85%, 100%, 70%, 45%, 25%` bar heights, `2px` bar width, `2px` gap, `1px` pill radius).
- **Props:**
  - `size?: 'sm' | 'md' | 'lg'` — `'sm'` (`16 x 16 px`), `'md'` (`19 x 21 px`, used in `TopBar`), `'lg'` (`32 x 36 px`, used in `/design.html` `[01]`).
  - `accentPeak?: boolean` — when `true`, colours the tallest centre bar (`bar 4`) with `--color-accent`; defaults to `false` (all bars `currentColor` / `--color-text`, matching `concept-dark.png`).
- **Tokens:** `--color-text`, `--color-accent`.
- **Used in:** `TopBar`, `Wordmark`, `DesignPage`.

### `Wordmark.svelte`
- **Path:** `ui/src/lib/design/Wordmark.svelte`
- **Purpose:** Pairs `Mark` with the tracked `RESONANCE` display wordmark (`133 x 13 px` glyph box at `16px`).
- **Props:**
  - `showMark?: boolean` (default `true`)
  - `size?: 'md' | 'lg'` — `'md'` (`--text-lg: 16px`, `letter-spacing: 0.32em`, `18px` gap after `Mark`), `'lg'` (`--text-xl: 20px` for `/design.html` `[01]`).
- **Tokens:** `--font-display`, `--text-lg`, `--text-xl`, `--color-text`, `--space-4`.
- **Used in:** `TopBar`, `DesignPage`.

### `Icon.svelte`
- **Path:** `ui/src/lib/design/Icon.svelte`
- **Purpose:** Renders crisp `1.5px`-stroke monochrome vector icons on a pixel-aligned grid.
- **Props:**
  - `name: 'search' | 'library' | 'prep' | 'sets' | 'mix' | 'replay' | 'collections' | 'activity' | 'settings' | 'audio' | 'play' | 'pause' | 'star' | 'check' | 'unanalysed' | 'recent' | 'sliders' | 'plus' | 'close' | 'more' | 'info' | 'chevron-down' | 'chevron-right' | 'headphones' | 'sidebar-collapse' | 'arrow-right'`
  - `size?: 'xs' | 'sm' | 'md'` — `'xs'` (`13px`, filter pills and info badges), `'sm'` (`16px`, sidebar items, search icon, table status icons), `'md'` (`18px`, top-bar right buttons, inspector star/more buttons, headphone cue icon).
  - `filled?: boolean` (default `false`; `true` for active `play` triangle andstarred `star`).
- **Tokens:** `currentColor` (`--color-text`, `--color-text-2`, `--color-muted`).
- **Used in:** `TopBar`, `Sidebar`, `CommandBar`, `FilterPill`, `TrackTable`, `Inspector`, `TransitionDock`, `PlayerBar`.

### `Button.svelte`
- **Path:** `ui/src/lib/design/Button.svelte`
- **Purpose:** Interactive button primitive supporting text, icon, quiet, primary and circular transport variants.
- **Props:**
  - `variant?: 'default' | 'primary' | 'quiet' | 'icon' | 'circle' | 'select'`
    - `'default'`: `background: var(--color-surface-2)`, `border: 1px solid var(--color-border-control)`, `color: var(--color-text)`.
    - `'primary'`: inverted monochrome fill (`background: var(--color-text)`, `color: var(--color-background)`).
    - `'quiet'`: transparent at rest, `background: var(--color-raised)` on hover/focus, `color: var(--color-text-2)` (`--color-text` on hover).
    - `'icon'`: square icon button (`28 x 28 px` default, `34 x 34 px` for `size="md"`, `24 x 24 px` for `size="sm"`), `border-radius: var(--radius-sm)`.
    - `'circle'`: circular button (`border-radius: var(--radius-pill)`), used for the `44 x 44 px` main play button in `PlayerBar` (`size="lg"`) and the `26 x 26 px` `+` tag button in `Inspector` (`size="sm"`).
    - `'select'`: bordered dropdown trigger (`background: var(--color-surface-2)`, `border: 1px solid var(--color-border)`, `border-radius: var(--radius-sm)`, trailing `chevron-down` icon), used for `Curve`, `EQ`, `Mix Point` and `Blend`.
  - `size?: 'xs' | 'sm' | 'md' | 'lg'` — maps to `--control-height-xs` (`22px`), `--control-height-sm` (`26px`), `--control-height` (`28px`), `--control-height-md` (`34px`), or `--control-circle-lg` (`44px`).
  - `active?: boolean`, `disabled?: boolean`, `ariaLabel?: string`, `onclick?: (e: MouseEvent) => void`.
- **Tokens:** `--control-height-xs`, `--control-height-sm`, `--control-height`, `--control-height-md`, `--control-circle-lg`, `--radius-xs`, `--radius-sm`, `--radius-pill`, `--color-surface-2`, `--color-raised`, `--color-border`, `--color-border-control`, `--color-text`, `--color-text-2`, `--font-sans`, `--text-sm`, `--duration-fast`.
- **Used in:** Every region and screen.

### `TextInput.svelte`
- **Path:** `ui/src/lib/design/TextInput.svelte`
- **Purpose:** Single-line text and search input with optional leading icon and trailing shortcut badge.
- **Props:**
  - `value: string`, `placeholder?: string`, `size?: 'md' | 'lg'` (`'md'` = `--control-height: 28px`, `border-radius: var(--radius-sm)`; `'lg'` = `--control-height-lg: 42px`, `border-radius: var(--radius-lg)` for `CommandBar`).
  - `leadingIcon?: IconName`, `shortcutBadge?: string` (renders `DataTag variant="keybadge"` on the right).
- **Tokens:** `--control-height`, `--control-height-lg`, `--radius-sm`, `--radius-lg`, `--color-surface`, `--color-border`, `--color-border-control`, `--color-text`, `--color-muted`, `--font-sans`, `--text-base`, `--space-3`, `--space-4`.
- **Used in:** `CommandBar`, `ScanFolder`, `DesignPage`.

### `MonoLabel.svelte`
- **Path:** `ui/src/lib/design/MonoLabel.svelte`
- **Purpose:** Uppercase tracked monospace label used for section headers, sidebar group headers, and metadata keys across the entire app.
- **Props:**
  - `text: string` (or default snippet)
  - `size?: '2xs' | 'xs'` — `'2xs'` (`--text-2xs: 10px`, `letter-spacing: 0.10em`), `'xs'` (default, `--text-xs: 11px`, `letter-spacing: 0.08em`).
  - `tone?: 'muted' | 'secondary' | 'primary'` — maps to `--color-muted` (default for stat keys `BPM`, `KEY`, `TIME`), `--color-text-2` (`MUSIC INTELLIGENCE FOR DJS`, `ACTIVE SET`, `TAGS`, `AUDIO TRAITS`, `TRANSITION SUGGESTIONS`), or `--color-text`.
  - `infoTooltip?: string` — renders a trailing `13px` `ⓘ` icon with `Tooltip`.
  - `actionLabel?: string`, `onAction?: () => void` — renders a right-aligned action link (`View All ->` in `--font-sans`, `--text-xs`, `--color-text-2`).
- **Tokens:** `--font-mono`, `--text-2xs`, `--text-xs`, `--color-muted`, `--color-text-2`, `--color-text`, `--space-1-5`.
- **Used in:** `Sidebar` (`MUSIC INTELLIGENCE FOR DJS`, `ACTIVE SET`), `Inspector` (`BPM`, `KEY`, `TIME`, `RELEASE`, `GENRE`, `SUBGENRE`, `LABEL`, `TAGS`, `AUDIO TRAITS`, `TRANSITION SUGGESTIONS`), `DesignPage`.

### `DataTag.svelte`
- **Path:** `ui/src/lib/design/DataTag.svelte`
- **Purpose:** Compact bordered badge for technical metadata, status indicators, removable user tags, match scores and keyboard shortcuts.
- **Props:**
  - `label: string`
  - `variant?: 'pill' | 'rect' | 'removable' | 'score' | 'keybadge'`
    - `'pill'`: `height: var(--control-height-xs)` (`22px`), `border-radius: var(--radius-pill)`, `--font-mono`, `--text-xs: 11px`. Used for `✓ Analysed` (`78 x 22 px` in `Inspector`) and Board 2 `[05]`/`[07]` tags (`128.0 BPM`, `8A`, `WAV 24-bit`, `-7.4 LUFS`).
    - `'rect'`: `height: var(--control-height-xs)` (`22px`), `border-radius: var(--radius-sm)` (`6px`), `--font-mono`, `--text-xs: 11px`.
    - `'removable'`: `height: var(--control-height-sm)` (`26px`), `border-radius: var(--radius-sm)` (`6px`), `--font-sans`, `--text-sm: 12px`, trailing `×` button (`onRemove`). Used for `Inspector` tags (`Hypnotic ×`, `Peak Time ×`, `Industrial ×`, `Rolling Bass ×`).
    - `'score'`: `height: var(--control-height-xs)` (`22px`), `min-width: 43px`, `border-radius: var(--radius-sm)` (`6px`), `--font-mono`, `--text-xs: 11px`, tabular-nums. Used for `95%`, `92%`, `89%` in `Inspector` transition suggestions.
    - `'keybadge'`: `height: 24px`, `padding: 0 var(--space-2)`, `border-radius: var(--radius-sm)` (`6px`), `--font-mono`, `--text-xs: 11px`, `--color-text-2`. Used for `⌘ K` / `Ctrl K` (`46 x 24 px`) in `CommandBar`.
  - `leadingIcon?: IconName`, `onRemove?: () => void`.
- **Tokens:** `--control-height-xs`, `--control-height-sm`, `--radius-sm`, `--radius-pill`, `--color-surface-2`, `--color-border`, `--color-text`, `--color-text-2`, `--color-muted`, `--font-mono`, `--font-sans`, `--text-xs`, `--text-sm`, `--space-2`, `--space-2-5`.
- **Used in:** `CommandBar`, `Inspector`, `Mix`, `DesignPage`.

### `SlashChip.svelte`
- **Path:** `ui/src/lib/design/SlashChip.svelte`
- **Purpose:** Interactive slash-command suggestion chip pairing a monospace command badge with a plain-English description.
- **Props:**
  - `command: string` (e.g. `'/ bpm 125-132'`)
  - `description: string` (e.g. `'Filter by tempo'`)
  - `onclick?: () => void`
- **Tokens:** `--control-height-xs` (`22px`), `--radius-sm` (`6px`), `--color-surface-2`, `--color-raised`, `--color-border`, `--color-text`, `--color-text-2`, `--font-mono`, `--font-sans`, `--text-xs` (`11px`), `--text-sm` (`12px`), `--space-2`, `--space-2-5` (`10px` internal gap).
- **Used in:** `CommandBar`, `DesignPage`.

### `FilterPill.svelte`
- **Path:** `ui/src/lib/design/FilterPill.svelte`
- **Purpose:** Rounded pill button with an inverted monochrome active state (`--color-text` fill with `--color-background` text), used for library quick-filters and inspector curve selection.
- **Props:**
  - `label: string`
  - `active?: boolean` (default `false`)
  - `icon?: IconName` (optional `13px` monochrome leading icon: `'check'`, `'unanalysed'`, `'star'`, `'recent'`)
  - `count?: number`, `onclick?: () => void`
- **Tokens:** `--control-height-sm` (`26px`), `--radius-pill`, `--color-text`, `--color-background`, `--color-text-2`, `--color-raised`, `--font-sans`, `--text-sm` (`12px`), `--space-3` (`12px` horizontal padding), `--space-1-5` (`6px` icon gap).
- **Used in:** `Library` (`All Tracks`, `✓ Analysed`, `○ Unanalysed`, `★ Favourites`, `◷ Recent`), `Inspector` (`Energy`), `DesignPage`.

### `SegmentedTabs.svelte`
- **Path:** `ui/src/lib/design/SegmentedTabs.svelte`
- **Purpose:** Bordered multi-option segmented control from Board 2 `[07]` (`38px` outer height, `8px` outer radius, `32px` raised active segment with `6px` radius).
- **Props:**
  - `items: ReadonlyArray<{ id: string; label: string; count?: number }>`
  - `value: string`, `onChange: (id: string) => void`
- **Tokens:** `--control-height-nav` (`38px`), `--radius-md` (`8px`), `--radius-sm` (`6px`), `--color-surface-2`, `--color-raised`, `--color-border`, `--color-border-control`, `--color-text`, `--color-text-2`, `--font-sans`, `--text-sm` (`12px`).
- **Used in:** `Prep` sub-views (`Inbox`, `Duplicates`, `Quality` in M3/M4), `DesignPage`.

### `Thumb.svelte`
- **Path:** `ui/src/lib/design/Thumb.svelte`
- **Purpose:** Square bordered artwork frame that displays track cover art when available (`M3`) or a deterministic static `Wave` dot-mesh seeded from the track ID.
- **Props:**
  - `size?: 'sm' | 'md' | 'lg'`
    - `'sm'`: `var(--thumb-sm)` (`36 x 36 px`), `border-radius: var(--radius-sm)` (`6px`). Used in compact track cards (`Prep`, Board 2 `[07]`).
    - `'md'`: `var(--thumb-md)` (`60 x 60 px`), `border-radius: var(--radius-md)` (`8px`). Used in `Deck` pods inside `TransitionDock` (`x = 82..142, y = 821..881`).
    - `'lg'`: `var(--thumb-lg)` (`96 x 96 px`), `border-radius: var(--radius-lg)` (`10px`). Used at the top-left of `Inspector` (`x = 1175..1271, y = 65..160`).
  - `src?: string | null`, `alt?: string`, `seed: number`.
- **Tokens:** `--thumb-sm`, `--thumb-md`, `--thumb-lg`, `--radius-sm`, `--radius-md`, `--radius-lg`, `--color-surface-2`, `--color-border`.
- **Used in:** `Inspector`, `Deck`, `Card`, `DesignPage`.

### `Card.svelte`
- **Path:** `ui/src/lib/design/Card.svelte`
- **Purpose:** Reusable bordered container for deck pods, the inset bottom player bar, collection cards, and compact track cards.
- **Props:**
  - `variant?: 'surface' | 'raised' | 'interactive' | 'dashed'`
    - `'surface'`: `background: var(--color-surface)`, `border: 1px solid var(--color-border)`.
    - `'raised'`: `background: var(--color-raised)`, `border: 1px solid var(--color-border-control)`.
    - `'interactive'`: `background: var(--color-surface)`, `border: 1px solid var(--color-border)`, hover border `--color-border-control`.
    - `'dashed'`: `background: transparent`, `border: 1px dashed var(--color-border-control)` (`+ New Collection` slot in `Sets`).
  - `radius?: 'md' | 'lg' | 'xl'` — maps to `--radius-md` (`8px`), `--radius-lg` (`10px`), or `--radius-xl` (`12px`).
  - `selected?: boolean` — adds `border-color: var(--color-border-control)` and a `2px` left `--color-accent` indicator bar.
  - `waveArt?: boolean`, `waveSeed?: number` — mounts a background `Wave variant="card"` canvas inside the card (`Sets` collection cards).
- **Tokens:** `--color-surface`, `--color-raised`, `--color-border`, `--color-border-control`, `--color-accent`, `--radius-md`, `--radius-lg`, `--radius-xl`, `--space-3`, `--space-4`.
- **Used in:** `Deck`, `PlayerBar`, `Sets` (M10), `Prep` (M3), `DesignPage`.

### `MeterBar.svelte`
- **Path:** `ui/src/lib/design/MeterBar.svelte`
- **Purpose:** 5-bar vertical discrete level meter (`27 x 12 px`) used in the `TrackTable` `Energy` column (`x = 908..934`).
- **Props:**
  - `value: 0 | 1 | 2 | 3 | 4 | 5 | null` (`null` when unanalysed, rendering 5 muted bars)
  - `label?: string` (for `aria-label`, e.g. `'Energy 4 of 5'`)
- **Tokens:** `--color-text` (lit bars `1..value`), `--color-border-control` (unlit bars), `--color-surface-2`, `--radius-xs`.
- **Used in:** `TrackTable`, `DesignPage`.

### `LevelDots.svelte`
- **Path:** `ui/src/lib/design/LevelDots.svelte`
- **Purpose:** 4-dot horizontal stem presence indicator (`DRUMS`, `BASS`, `VOCALS`, `OTHER`) from Board 2 `[07]`.
- **Props:**
  - `label: string`
  - `level: 0 | 1 | 2 | 3 | 4`
- **Tokens:** `--font-mono`, `--text-xs` (`11px`), `--color-text`, `--color-text-2`, `--color-border`, `--space-1`, `--space-2`.
- **Used in:** `Mix` stem controls (`M12`), `DesignPage` (`#50`).

### `TraitBar.svelte`
- **Path:** `ui/src/lib/design/TraitBar.svelte`
- **Purpose:** Single horizontal audio-trait row (`17px` pitch) combining a sans label (`Danceability`), a tabular mono value (`0.82`), and a `218 x 5 px` progress bar.
- **Props:**
  - `label: string`
  - `value: number | null` (`0.0` to `1.0`, or `null` when unanalysed)
  - `formattedValue?: string` (defaults to `value.toFixed(2)` or `'—'`)
- **Tokens:** `--font-sans`, `--font-mono`, `--text-sm` (`12px`), `--color-text`, `--color-text-2`, `--color-surface-2`, `--radius-xs` (`3px`), `--space-3`.
- **Used in:** `Inspector` (`AUDIO TRAITS` section), `DesignPage`.

### `ScrubBar.svelte`
- **Path:** `ui/src/lib/design/ScrubBar.svelte`
- **Purpose:** Horizontal range slider for timeline scrubbing (`387 px` in `PlayerBar`, `260 px` in Board 2 `[07]`) and deck channel volume (`86 px` `A Vol` / `B Vol` in `PlayerBar`).
- **Props:**
  - `value: number`, `min?: number` (default `0`), `max?: number` (default `100`), `step?: number`
  - `size?: 'sm' | 'md'` — `'md'` (`4px` track, `12 x 12 px` thumb for timeline scrub), `'sm'` (`4px` track, `10 x 10 px` thumb for `A Vol` and `B Vol`).
  - `label?: string` (e.g. `'A Vol'`), `valueText?: string` (e.g. `'100%'`), `onInput?: (v: number) => void`.
- **Tokens:** `--color-surface-2`, `--color-text`, `--color-surface`, `--color-border-control`, `--font-mono`, `--text-sm` (`12px`), `--space-2`, `--space-3`.
- **Used in:** `PlayerBar`, `DesignPage`.

### `FocusBrackets.svelte`, `IndexMarks.svelte`, `ModularDivider.svelte`, `GridDots.svelte`, `Provenance.svelte`
- **Paths:** `ui/src/lib/design/FocusBrackets.svelte`, `IndexMarks.svelte`, `ModularDivider.svelte`, `GridDots.svelte`, `Provenance.svelte`
- **Purpose & Props:**
  - `FocusBrackets`: `{ active?: boolean; accent?: boolean; children: Snippet }` — wraps a drop target or active panel with four `8 x 8 px` `1px` corner L-ticks.
  - `IndexMarks`: `{ index: number }` — renders zero-padded `[01]` in `--font-mono`, `--text-2xs: 10px`, `--color-muted`.
  - `ModularDivider`: `{ orientation?: 'horizontal' | 'vertical'; inset?: 'none' | 'md'; ticks?: boolean; label?: string }` — renders a `1px` `--color-border` divider (`15–20 px` inset when `inset="md"` inside `Inspector`, `33 px` vertical rule at `x = 1507` inside `PlayerBar`).
  - `GridDots`: `{ spacing?: 16 | 24 }` — renders a `1px` `--color-border` dot grid behind empty drop zones and timelines.
  - `Provenance`: `{ source: 'file' | 'provider' | 'model' | 'user'; provider?: string; confidence?: number }` — renders tag/analysis provenance (`--font-mono`, `--text-2xs: 10px`).
- **Used in:** `Inspector`, `PlayerBar`, `Sidebar`, `ScanFolder`, `Mix`, `DesignPage`.

### `Panel.svelte`, `Sheet.svelte`, `Tooltip.svelte`, `TableRow.svelte`, `TableCell.svelte`
- **Paths:** `ui/src/lib/design/Panel.svelte`, `Sheet.svelte`, `Tooltip.svelte`, `TableRow.svelte`, `TableCell.svelte`
- **Updates in `#50`:**
  - `TableRow.svelte` sets `height: var(--row-height)` (`33px`). When `selected={true}`, it draws the `6px` rounded light frame from `concept-dark.png` (`border-radius: var(--radius-sm)`, `background: var(--color-raised)`, `outline: 1px solid var(--color-border-control)`, `outline-offset: -1px`) plus the `2px` left `--color-accent` selection mark.
  - `Sheet.svelte` uses `border-radius: var(--radius-xl)` (`12px`) for modals and serves as the slide-over container for `Inspector` when toggled on narrow viewports (`< 1280px`).

---

## Layer 2 — Composed Domain Components (`ui/src/lib/wave/`, `ui/src/lib/library/`, `ui/src/lib/mix/`)

### `Wave.svelte`
- **Path:** `ui/src/lib/wave/Wave.svelte`
- **Purpose:** WebGL2 point-sprite dot-wave field (`< 1 ms` UI thread time per frame; static single frame under `prefers-reduced-motion: reduce` or while `Mix` is playing).
- **Props:**
  - `variant?: 'ambient' | 'sidebar' | 'card' | 'thumb'`
    - `'sidebar'`: `width: 100%`, `height: var(--sidebar-wave-height)` (`78px`, `254 x 78 px` at `y = 655..732` in `concept-dark.png`), top CSS mask fade into `--color-surface`.
    - `'ambient'`: workspace header background mesh (`opacity: 0.12` dark / `0.07` light).
    - `'card'`: `181 x 82 px` collection card background (`Sets` in M10).
    - `'thumb'`: static single-frame fallback inside `Thumb.svelte`.
  - `seed?: number`, `paused?: boolean`.
- **Tokens:** `--color-text`, `--color-surface`, `--sidebar-wave-height`.
- **Used in:** `Sidebar`, `Thumb`, `Card`, `Library`, `DesignPage`.

### `Waveform.svelte`
- **Path:** `ui/src/lib/mix/Waveform.svelte`
- **Purpose:** Renders `1px` vertical bar waveforms (`1px` bar width, `1px` gap, snapped to device pixels via `devicePixelRatio`) and flat `1px` energy step curves.
- **Props:**
  - `mode: 'inline' | 'crossfade' | 'deck' | 'curve'`
    - `'inline'`: `140 x 17 px` (`x = 502..641` in `TrackTable`), static peak bars (`--color-text-2` unplayed, `--color-text` played).
    - `'crossfade'`: `100% x var(--dock-waveform-height)` (`844 x 101 px` at reference viewport in `TransitionDock`), Deck A and Deck B `1px` bars, two `1.5px` S-curve volume envelopes, and a `1px` vertical playhead line with a `5 x 5 px` top square cap (`--color-accent` when playing/scrubbing, `--color-text` at rest).
    - `'deck'`: full-width scrolling WebGL2 deck waveform in `Mix.svelte` locked to the audio clock (`120 Hz`).
    - `'curve'`: `385 x 67 px` (`x = 1185..1569, y = 486..552` in `Inspector`), flat `1px` monochrome energy curve over a `1px` `--color-border` baseline at `y = 552` with five `11px` mono timestamps below (`0:00  1:45  3:30  5:15  7:02`).
  - `peaks?: Float32Array | null`, `secondaryPeaks?: Float32Array | null`, `progress?: number`, `mixPoint?: number`, `durationSec?: number`.
- **Tokens:** `--color-text`, `--color-text-2`, `--color-muted`, `--color-border`, `--color-accent`, `--dock-waveform-height`, `--font-mono`, `--text-xs`.
- **Used in:** `TrackTable`, `TransitionDock`, `Inspector`, `Mix`, `DesignPage`.

### `Deck.svelte`
- **Path:** `ui/src/lib/mix/Deck.svelte`
- **Purpose:** Renders a Deck A or Deck B unit. Reused in both the bottom `TransitionDock` (`variant="dock"`) and the `Mix` workspace (`variant="full"`).
- **Props:**
  - `deck: 'A' | 'B'`
  - `variant?: 'dock' | 'full'`
    - `'dock'` (`340 x 87 px` in `TransitionDock`, built on `Card variant="surface" radius="lg"`): `35 x 34 px` deck letter badge (`A` or `B`, `--radius-sm: 6px`, `--color-surface-2`, `--font-display` 600 `--text-lg: 16px`), `60 x 60 px` `Thumb size="md"` (`--radius-md: 8px`), track title (`--font-display` 500 `--text-base: 14px`), artist (`--font-sans` `--text-sm: 12px` `--color-text-2`), mono stats line (`128.0 BPM · A min · 7:02` in `--font-mono` `--text-xs: 11px` `--color-text-2`), and top-right `...` `Button variant="icon" size="sm"`.
    - `'full'` (`Mix.svelte` deck panel): adds `FocusBrackets`, transport controls (`Play`, `Cue`, `Sync`, `Keylock`), pitch slider, and scrolling `Waveform mode="deck"`.
  - `track?: LibraryTrack | null`, `state?: MixDeckState`.
- **Tokens:** `--dock-card-width`, `--dock-card-height`, `--radius-lg`, `--radius-md`, `--radius-sm`, `--thumb-md`, `--color-surface`, `--color-surface-2`, `--color-border`, `--color-text`, `--color-text-2`, `--font-display`, `--font-sans`, `--font-mono`, `--text-base`, `--text-sm`, `--text-xs`.
- **Used in:** `TransitionDock`, `Mix`, `DesignPage`.

### `TrackTable.svelte`
- **Path:** `ui/src/lib/library/TrackTable.svelte`
- **Purpose:** Virtualised 100,000-row track table (`31px` header, `33px` row pitch, `< 16 ms` scroll frames) with container-query responsive column dropping.
- **Props:**
  - `tracks`, `totalCount: number`, `selectedTrackId: number | null`, `checkedIds: Set<number>`, `playingTrackId?: number | null`, `onSelect: (id: number) => void`, `onToggleCheck: (id: number) => void`.
- **Tokens:** `--table-header-height` (`31px`), `--row-height` (`33px`), `--radius-xs` (`3px` checkbox), `--radius-sm` (`6px` selected row frame), `--color-surface`, `--color-raised`, `--color-border`, `--color-border-control`, `--color-text`, `--color-text-2`, `--color-muted`, `--color-accent`, `--font-sans`, `--font-mono`, `--text-sm` (`12px` header), `--text-md` (`13px` cells).
- **Used in:** `Library`.

### `ScanFolder.svelte`
- **Path:** `ui/src/lib/library/ScanFolder.svelte`
- **Purpose:** Folder path input and scan trigger shown in the library empty state and header drawer.
- **Tokens:** `--control-height`, `--radius-sm`, `--color-surface`, `--color-border`, `--font-sans`, `--font-mono`, `--text-sm`.
- **Used in:** `Library`.

---

## Layer 3 — Shell and Workspace Regions (`ui/src/lib/shell/`, `ui/src/lib/library/`)

### `TopBar.svelte`
- **Path:** `ui/src/lib/shell/TopBar.svelte`
- **Purpose:** `50px` tall (`--topbar-height`) full-width window header bar (`y = 0..50` in `concept-dark.png`) with `1px` bottom border (`--color-border`).
- **Composition:** Left `Wordmark` (`Mark` `19 x 21 px` + `RESONANCE` `16px`); right three `Button variant="icon"` (`28 x 28 px` with `18px` `Icon`): audio-engine status (`icon="audio"`), `Activity` job queue (`icon="activity"`), and `Settings` (`icon="settings"`).
- **Tokens:** `--topbar-height` (`50px`), `--color-surface`, `--color-border`, `--space-5` (`20px`).
- **Used in:** `App.svelte`.

### `Sidebar.svelte`
- **Path:** `ui/src/lib/shell/Sidebar.svelte` (replaces `Rail.svelte` in `#51`)
- **Purpose:** `256px` wide (`--sidebar-width`) collapsible left navigation sidebar (`52px` `--sidebar-collapsed-width` when collapsed), spanning `y = 51..752` with a `1px` right border (`--color-border`).
- **Props:**
  - `activeSpace: 'library' | 'prep' | 'sets' | 'mix' | 'replay'`
  - `collapsed: boolean`, `trackCount: number`, `onSelectSpace: (space) => void`, `onToggleCollapse: () => void`
- **Composition:**
  1. Top two-line `MonoLabel` (`MUSIC INTELLIGENCE / FOR DJS`, `11px` mono, `x = 25, y = 75..97`) and collapse toggle button (`[`).
  2. Primary space buttons (`232 x 38 px`, `--control-height-nav: 38px`, `--radius-md: 8px`, `16px` `Icon`, `14px` `--font-sans` label, `12px` `--font-mono` count on `Library`, `2px` left `--color-accent` mark on active item): `Library`, collapsible `Collections` sub-tree (`M2`), `Prep`, `Sets`, `Mix`, `Replay`.
  3. `ModularDivider` (`y = 562`), `MonoLabel` (`ACTIVE SET`, `y = 580`), and active set row (`Fri 23:00 — Warehouse`, `13px` sans, `M10`).
  4. Bottom `Wave variant="sidebar"` canvas (`254 x 78 px` at `y = 655..732`).
- **Tokens:** `--sidebar-width` (`256px`), `--sidebar-collapsed-width` (`52px`), `--sidebar-wave-height` (`78px`), `--control-height-nav` (`38px`), `--radius-md` (`8px`), `--color-surface`, `--color-raised`, `--color-border`, `--color-text`, `--color-text-2`, `--color-accent`, `--font-sans`, `--font-mono`, `--text-xs`, `--text-sm`, `--text-md`, `--text-base`.
- **Used in:** `App.svelte`.

### `CommandBar.svelte`
- **Path:** `ui/src/lib/shell/CommandBar.svelte`
- **Purpose:** Top of the centre workspace column (`y = 51..152` in `concept-dark.png`): houses the `42px` search and command box (`TextInput size="lg"` with `⌘ K` / `Ctrl K` `DataTag variant="keybadge"`, `--radius-lg: 10px`) and the `22px` `SlashChip` strip below it, closed by a `1px` bottom `--color-border` at `y = 152`.
- **Tokens:** `--control-height-lg` (`42px`), `--control-height-xs` (`22px`), `--radius-lg` (`10px`), `--radius-sm` (`6px`), `--color-surface`, `--color-surface-2`, `--color-border`, `--color-border-control`, `--space-3` (`12px`), `--space-5` (`20px`).
- **Used in:** `App.svelte`.

### `Inspector.svelte`
- **Path:** `ui/src/lib/library/Inspector.svelte`
- **Purpose:** `420px` wide (`--inspector-width`) right-hand track inspector column (`x = 1166..1585, y = 51..752`). Has **no top `"INSPECTOR"` header bar**: opens directly into the track header (`96 x 96 px` `Thumb size="lg"` + `20px` title + `14px` artist + `✓ Analysed` `DataTag variant="pill"` + `12px` mono audio specs), followed by five sections separated by `ModularDivider inset="md"`: 2-row metadata grid (`MonoLabel` keys + `13px` values), `TAGS` (`DataTag variant="removable"` + circular `+` `Button`), `AUDIO TRAITS` (five `TraitBar` rows), `Energy` curve (`FilterPill` + `Waveform mode="curve"`), and `TRANSITION SUGGESTIONS` (three rows with `Button variant="icon"`, title/reason, and `95%` `DataTag variant="score"`).
- **Tokens:** `--inspector-width` (`420px`), `--inspector-min-width` (`340px`), `--inspector-max-width` (`440px`), `--thumb-lg` (`96px`), `--radius-lg` (`10px`), `--radius-sm` (`6px`), `--radius-pill`, `--color-surface`, `--color-surface-2`, `--color-border`, `--color-text`, `--color-text-2`, `--color-muted`, `--font-display`, `--font-sans`, `--font-mono`, `--text-xs` (`11px`), `--text-sm` (`12px`), `--text-md` (`13px`), `--text-base` (`14px`), `--text-xl` (`20px`), `--space-4` (`16px`), `--space-5` (`20px`).
- **Used in:** `App.svelte` / `Library.svelte`.

### `TransitionDock.svelte`
- **Path:** `ui/src/lib/shell/TransitionDock.svelte`
- **Purpose:** Bottom `A -> B` transition preview dock (`y = 753..913` in `concept-dark.png`) above `PlayerBar`. Comprises a `52px` header row (`A -> B Transition`, `Mix Point 4:15 (60%) v`, `Blend Crossfade v`) and a three-column body: left `Deck variant="dock" deck="A"` (`340 x 87 px`), centre `Waveform mode="crossfade"` (`844 x 101 px` at reference viewport), and right `Deck variant="dock" deck="B"` (`340 x 87 px`). Collapses to its `52px` header strip when toggled or on viewports shorter than `820px`.
- **Tokens:** `--dock-header-height` (`52px`), `--dock-card-width` (`340px`), `--dock-card-height` (`87px`), `--dock-waveform-height` (`101px`), `--radius-lg` (`10px`), `--color-background`, `--color-surface`, `--color-border`, `--font-display`, `--font-sans`, `--font-mono`, `--text-lg` (`16px`), `--text-md` (`13px`), `--text-sm` (`12px`), `--space-3` (`12px`), `--space-5` (`20px`).
- **Used in:** `App.svelte`.

### `PlayerBar.svelte`
- **Path:** `ui/src/lib/shell/PlayerBar.svelte`
- **Purpose:** `60px` tall (`--player-height`) rounded, bordered transport card (`Card variant="surface" radius="xl"`, `--radius-xl: 12px`) inset `20px` (`--player-inset-x`) from the left and right window edges and `17px` (`--player-inset-bottom`) from the bottom edge (`x = 20..1567, y = 915..974` in `concept-dark.png`).
- **Composition:** `44 x 44 px` circular play `Button variant="circle" size="lg"`, `28 x 28 px` pause `Button variant="icon"`, tabular mono time `4:15 / 7:02`, flexible main `ScrubBar size="md"` (`387px` at reference viewport), `A Vol 100%` `ScrubBar size="sm"` (`86px`), `B Vol 100%` `ScrubBar size="sm"` (`86px`), `Curve` `Button variant="select"`, `EQ Flat v` `Button variant="select"`, `33px` vertical `ModularDivider`, and `36 x 28 px` headphone cue `Button variant="icon"`.
- **Tokens:** `--player-height` (`60px`), `--player-inset-x` (`20px`), `--player-inset-bottom` (`17px`), `--radius-xl` (`12px`), `--radius-sm` (`6px`), `--radius-pill`, `--control-circle-lg` (`44px`), `--control-height-sm` (`26px`), `--control-height` (`28px`), `--color-surface`, `--color-surface-2`, `--color-border`, `--color-border-control`, `--color-text`, `--color-text-2`, `--font-mono`, `--font-sans`, `--text-sm` (`12px`).
- **Used in:** `App.svelte`.

---

## Layer 4 — Screens (`ui/src/`)

| Screen component | File path | Purpose & composed regions | Milestone |
| --- | --- | --- | --- |
| `App.svelte` | `ui/src/App.svelte` | Root responsive CSS Grid shell composing `TopBar`, `Sidebar`, `CommandBar`, active workspace view, `Inspector`, `TransitionDock` and `PlayerBar` | `#51`, `#52` |
| `Library.svelte` | `ui/src/lib/library/Library.svelte` | Library workspace: `27px` `Library` title (`--text-2xl`), track count, filter button, `FilterPill` strip (`All Tracks`, `✓ Analysed`, `○ Unanalysed`, `★ Favourites`, `◷ Recent`), `ScanFolder`, and `TrackTable` | `#52` (shell), `M2`–`M6` (live data) |
| `Mix.svelte` | `ui/src/lib/mix/Mix.svelte` | Two-deck performance view composing two `Deck variant="full"` strips, `Waveform mode="deck"`, central mixer channel controls, and coach strip (`M8`) | `#52` (restyle), `M7`–`M8` (full mixer & coach) |
| `DesignPage.svelte` | `ui/src/design-page/DesignPage.svelte` | `/design.html` living specification page rendering every token swatch and Layer 1–2 component in dark, light and high-contrast modes | `#50` |
