# Prep

The Prep space is the decision queue and batch tagging workspace: `Prep.svelte`, `BatchTagSheet.svelte` and `WriteTagsSheet.svelte` in `ui/src/lib/prep/`, backed by `crates/core/src/prep.rs` and `crates/core/src/staging.rs`. [#116](https://github.com/BradHudsonGrant/RESONANCE/issues/116) builds it to this page. Sizes are at the reference window, 1586 by 992.

`concept-dark.png` places Preparation in the sidebar and sets the shell frame, header type, 33 px row density, 1 px borders and monochrome surfaces. `concept are and design 2.png` supplies the compact 36 px track rows (`[07]` and `[08]`), the data tags, the focus brackets and the light theme ([ADR 0012](../adr/0012-concept-boards-as-design-target.md)). Neither board draws a separate Preparation screen, so the workspace header, filter bar, table density and sheets follow [library.md](library.md).

Nothing in Prep blocks the rest of the app, and accepting a proposal stages the change in the user tag layer without touching files on disk until the user writes tags.

## Layout

```
+-----------------------------------------------------------------------+
| Prep   50 pending · 12 to write   Identify   Batch tools   Write tags |
| [All] [Proposals] [By release] | Threshold 90% [Accept above 90% ...] |
+----------------------------+------------------------------------------+
| queue                      | staging diff                             |
| 36 px thumb, title, artist | 60 px thumb, title, artist, format       |
| provider, fields, 96%      | provider, confidence, audio match        |
|                            |Accept (Enter)  Skip (S)  Edit (E)        |
|                            +------------------------------------------+
|                            | Field   Current   Proposed   Source      |
|                            | TITLE   ...       ...        DISCOGS 96% |
|                            | ARTIST  ...       ...        DISCOGS 96% |
|                            | LABEL   ...       ...        DISCOGS 96% |
+----------------------------+------------------------------------------+
```

In Prep the right-hand library inspector folds away so the queue and the side-by-side field diff share the full workspace width. The command bar stays at the top and the A to B dock and player bar stay at the bottom.

## Header

Matches the Library header: 16 px top padding, 20 px side padding, 12 px bottom padding, with a 16 px gap between the title row and the bar under it.

- On the left, "Prep" in 27 px Space Grotesk 500 (`--text-2xl`), the pending item count in 13 px mono and Text 2, and, while user tags or pictures are waiting to be written to files, the unwritten count (`12 to write`) in 13 px mono and Text 2.
- On the right, 34 px workspace buttons (`--control-height-md`), 8 px apart (`--space-2`):
  - Identify online, which queues online lookup in Activity for tracks in view.
  - Batch tools (`B`), which opens the batch tag sheet for the ticked tracks or the active Prep set.
  - Rename files, which opens the Rename sheet for the active Prep set.
  - Write tags (`W`), inverted as the primary action while any track has unwritten user tags or artwork, and quiet and disabled when nothing is dirty on disk.
- Under the title row, a 26 px toolbar (`--control-height-sm`) that never wraps and scrolls sideways when narrow, separated into groups by 1 px vertical rules (`14 px` tall):
  - Scope pills (`FilterPill`): `All` and `Proposals`, plus a `By release` pill (`G`) that groups tracks belonging to the same release.
  - When the user arrives from Library via "Send these to Prep" on a filter token, a removable 26 px batch token on Surface 2 with a 1 px control border shows the filter summary and track count. Removing it returns to the full Prep queue.
  - The threshold control: a `Threshold` label in 12 px Text 2, a range and percentage readout (`50%` to `100%` in `5%` steps, `90%` by default) in 12 px tabular mono, and the `Accept above X%` button (`Shift+A`). The button names how many tracks and fields it will change before it is pressed, for example `Accept above 90% (48 tracks · 192 fields)`, and turns disabled when the count is zero.
  - After a tag write or batch edit, an inline status and `Undo` button (`Ctrl+Z`) sit at the right end of the bar.

## Queue

The left column, `clamp(280px, 32%, 380px)` wide on Surface with a 1 px border on its right. It is a keyboard-navigable list (`role="listbox"`) of compact track or release items, 4 px apart (`--space-1`), with 12 px padding (`--space-3`).

Each item sits in a 48 px row with 8 px corners (`--radius-md`) and 10 px horizontal padding (`--space-2-5`):

- A 16 px tick box (`--checkbox-size`) with 3 px corners and a control border for multi-select.
- A 36 px artwork `Thumb` (`size="sm"`), showing the track's picture, proposed picture or seeded dot-wave frame.
- A two-line text stack (`min-width: 0`): the title in 13 px sans 500 in Text, cut with an ellipsis, and the artist or folder path under it in 12 px sans in Text 2.
- On the right, in 11 px tabular mono: the field change count (`+4`), the confidence percentage (`96%`) in Text 2 (or Muted below 80 %), and a 13 px check icon when Chromaprint confirmed that the audio matches.

Selection follows the table row rules in [components.md](components.md#primitives):

- The focused item sits on Raised inside a 1 px control border frame with the 2 px accent mark (`--selection-bar-width`) at its left.
- Ticked items without focus sit on Surface 2 with a 2 px mark in the control border colour.
- Under `@media (forced-colors: active)`, the focused item's frame and mark use `Highlight` and ticked items use `CanvasText`.

### Release grouping

When `By release` (`G`) is active, tracks that share a proposed or current release (matching catalogue number, matching album and label or year, or a release-named parent folder) collapse into one release row. The row shows the release title, the label or catalogue number, the track count in mono (`4 tracks`) and the lowest confidence across the group. Pressing `Enter` on a release row accepts the pending proposals for every track in that release in one transaction and moves to the next group.

## Staging diff and inline editor

The right column takes the remaining workspace width (`minmax(0, 1fr)`), framed by `FocusBrackets` while it holds focus.

### Item header

Padded 16 px (`--space-4`) with a 1 px border under it:

- A 60 px `Thumb` (`size="md"`). When identification has staged a release cover for a track that has none, the proposed cover shows beside it with `Use cover` and `Skip cover` buttons.
- The track title in 20 px Space Grotesk 600 (`--text-xl`), the artist in 14 px Text 2 (`--text-base`), and the file path and audio format (`WAV · 44.1 kHz · 16-bit`) in 12 px mono Text 2.
- The evidence row, 10 px apart (`--space-2-5`):
  - The source and confidence drawn by `Provenance.svelte` (`MUSICBRAINZ 96%`, `DISCOGS 92%`, `FILENAME 94%`, `TAGS 95%`).
  - A mono `DataTag` reading `Audio matches` only when Chromaprint compared the audio and it matched. Text or filename matches never show this badge.
  - What accepting will do (`Update 5 fields`), or why step 6 needs a link (`No confident match · paste a Discogs or Bandcamp URL`).
- Action buttons (`--control-height-sm`, 26 px) on the right: `Accept` (`Enter`), `Skip` (`S`), `Edit` (`E`) and `Play` (`Space`).
- When a track needs a release URL (or the user presses `U`), a 28 px `TextInput` (`size="md"`) below the summary takes a Discogs release or Bandcamp album or track URL and runs step 6 on `Enter`.

### Per-field diff table

Under the header sits the 13-row diff table (`Title`, `Artist`, `Mix`, `Album`, `Genre`, `Subgenre`, `Label`, `CatNo`, `Year`, `BPM`, `Key`, `Track number`, `Comment`), with a 31 px header (`--table-header-height`) and 33 px rows (`--row-height`) separated by 1 px hairlines:

| Column | Width | Drawn as |
| --- | --- | --- |
| Field | 120 px | 11 px uppercase mono (`--text-xs`), Text 2 |
| Current | `minmax(140px, 1fr)` | 13 px sans (mono for BPM, Key, Year, CatNo, Track number), with `Provenance` tick when held by the user, or `Empty` in Muted |
| Proposed | `minmax(160px, 1fr)` | 13 px sans or mono on Surface 2 with a 1 px control border when a pending proposal changes the field; Muted when unchanged or skipped |
| Source | 136 px | `Provenance` badge with provider and confidence, or `Locked` in mono when the user already holds the field |
| Action | 96 px | 22 px `Accept` or `Edit` button; replaced by an inline 28 px `TextInput` while editing that field |

Pressing `E` opens the inline editor on the first changing or empty field. While editing:

- `Enter` saves the typed value into the user tag layer (`user_tag`, normalised by `crates/core`), updates the diff in place, and moves focus back to the queue or to the next field with `Tab`.
- `ArrowDown` and `ArrowUp` (or `Tab` and `Shift+Tab`) move between fields in the diff table.
- `Escape` cancels the edit without changing the field and returns focus to the queue.

### One-frame advance and prefetch

When the focused item is accepted (`Enter`) or skipped (`S`), the queue advances to the next item in the same frame (`< 16 ms`) and sends the command in the background. The next item's `staging_diff` and `track_artwork` are prefetched as soon as an item is focused, so stepping through the queue never flashes an empty panel or waits on a round trip.

## Batch tag tools

Pressing `B` in Prep, or clicking `Batch tags` in Library's selection actions when tracks are ticked, opens `BatchTagSheet.svelte` (`Sheet` up to 560 px wide). Everything in the sheet runs from the keyboard (`Tab`, arrow keys on the mode tabs, `Enter` to apply, `Escape` to close).

Four tabs (`SegmentedTabs`) cover the batch jobs from [product.md](../product.md#identify-and-tag), using rules in `crates/core/src/prep.rs`:

1. **Edit fields**: pick a field (or several fields) and type a value to set across every track in the batch, or leave it blank to clear the field.
2. **Find and replace**: pick a specific text field or `All text fields`, enter `Find` and `Replace with`, and toggle `Match case`. Replacing a token collapses leftover double spaces and trims edges.
3. **Fix case**: pick one or more text fields (`Title`, `Artist`, `Mix`, `Album`, `Label`, `Genre`) and choose `Title Case`, `Sentence case` or `UPPER CASE`. Title and Sentence case preserve DJ and format acronyms in uppercase (`UK`, `DJ`, `DJs`, `EP`, `VIP`, `LP`, `MC`, `MCs`, `II`, `III`, `IV`, `ID`, `BPM`, `UKG`, `DNB`) and keep connecting tokens (`feat.`, `ft.`, `vs.`) lowercase.
4. **Release fields**: takes `Album`, `Label`, `Catalogue number` and `Year` from the focused track (including its staged proposals) and applies them to every track in the same release folder or batch.

The sheet shows a live preview count (`Will update 50 tracks`) before `Enter` applies the change to the user tag layer.

## Writing tags to files

User edits and accepted proposals update the library database at once, so search, sort and filters reflect them immediately. Writing those values into the audio files on disk happens through `WriteTagsSheet.svelte`, opened with `W` (or `Write tags` in the header):

- Lists the files with unwritten user tags or artwork from `PreviewTagWrites`, showing the path, each field's `from → to` change, whether a front cover will be written, and any blocked file with its reason.
- Pressing `Enter` on `Write tags to N files` runs `ApplyTagWrites`. Batches up to 200 files complete as one journaled operation and return the report at once; larger batches run as an Activity job with pause and cancel.
- After writing, `Ctrl+Z` (or the `Undo` button in the header bar or Activity) runs `UndoOperation` to restore the files' previous tags and covers.

## States

- **Empty queue**: `GridDots` panel (24 px spacing, 10 px corners, 1 px border) stating "Nothing waiting in Prep" with buttons to `Identify tracks online` or `Go to Library`.
- **Filtered batch with no pending proposals**: shows the tracks sent from Library so the user can run `Batch tools` (`B`), edit fields inline (`E`) or queue online identification.
- **Loading**: skeleton rows in the queue column with `aria-busy="true"`, without shifting the header or toolbar height.
- **Error**: an alert panel naming what failed and offering `Try again`.

## Keyboard

| Key | Does |
| --- | --- |
| `2` or `Ctrl+2` | Open Prep from anywhere in the shell |
| `ArrowDown`, `ArrowUp`, `Home`, `End` | Move through the queue |
| `Shift+ArrowDown`, `Shift+ArrowUp` | Extend the ticked selection |
| `X` | Tick or untick the focused queue item |
| `Ctrl+A` | Tick all items in the current queue or batch |
| `Enter` | Accept pending proposals on the focused track or release group and move to the next item |
| `S` | Skip pending proposals on the focused track or release group and move to the next item |
| `E` | Edit the focused item's fields inline (`Enter` saves, `Escape` cancels) |
| `Shift+A` | Accept all pending proposals at or above the current confidence threshold |
| `[` / `]` (when threshold focused) or `-` / `+` | Lower or raise the confidence threshold by 5 % |
| `G` | Toggle `By release` grouping |
| `B` | Open Batch tools for the ticked tracks or current batch |
| `W` | Open the Write tags preview sheet (and `Enter` inside it writes tags to disk) |
| `U` | Focus the release URL input for step 6 identification |
| `Space` | Play or pause the focused item in the preview player |
| `P` | Turn auto-preview on or off while moving through the queue |
| `Ctrl+Z` | Undo the last journaled tag write or rename |

## Window sizes and themes

Prep follows the shell rules in [ux.md](ux.md#window-sizes) and uses container queries on the workspace:

- **1586 by 992 up to 5120 by 2160**: the 256 px sidebar sits on the left, and the Prep workspace splits into the queue column (`clamp(280px, 32%, 380px)`) and the diff column (`minmax(0, 1fr)`), capped at 2400 px content width so diff columns stay readable on ultrawide displays.
- **1100 by 700**: the sidebar collapses to the 52 px icon rail and the dock collapses to its 52 px header. The workspace is 1048 px wide and keeps the two-column queue-and-diff split with `min-width: 0` on every grid cell so nothing overflows horizontally.
- **760 by 480**: below 860 px workspace width, the container query stacks the queue (180 px tall, scrollable) above the staging diff column so both the queue and the 13-field diff remain usable without horizontal clipping.
- **Dark, light and forced colours**: every surface, hairline and text tone comes from `tokens.css`. Under `@media (forced-colors: active)`, focused rows and active pills use `Highlight` and `HighlightText`, ticked rows use `CanvasText`, and all focus rings remain 2 px solid.
