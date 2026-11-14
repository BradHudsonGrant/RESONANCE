# Structure, layout and interaction

The two concept boards in `handoff/` are the design target, with `concept-dark.png` leading ([ADR 0012](../adr/0012-concept-boards-as-design-target.md)). This page says where each job lives, how the window is laid out at every size and how the app behaves. The look is in [visual-language.md](visual-language.md), the components in [components.md](components.md), and each region in [shell.md](shell.md), [library.md](library.md) and [mix.md](mix.md).

## Structure

Five spaces in the sidebar: Library, Prep, Sets, Mix and Replay. Activity and Settings sit at the top right of the top bar.

Each space is a job with a beginning and an end. Analysis, history and "intelligence" are properties of tracks and sessions, so they are shown where the track or session is shown and have no page of their own.

| Space | Key | What it is for | Built in |
| --- | --- | --- | --- |
| Library | 1 | Every track, search and filters, with collections listed under it in the sidebar | #51 and #52 for the shell and table, M2 for sources, search, collections and preview |
| Prep | 2 | Getting tracks ready to play: new tracks, identification, tags, quality checks, duplicates and missing files | M3 and M4 |
| Sets | 3 | Building and generating sets | M10 |
| Mix | 4 | The decks, the coach and recording | #52 restyles the spike, then M7, M8 and M12 |
| Replay | 5 | Recorded mixes, their scores and progress over time | M9 |

Until its milestone, a space is disabled in the sidebar, with a tooltip naming the milestone that brings it.

| Job | Where it lives |
| --- | --- |
| Add folders, choose subfolders | Settings > Sources, and the Library sidebar |
| Browse, search, filter, collections | Library |
| New tracks, identify, fix tags, quality flags, duplicates, missing files | Prep (the same tools are also on any track in Library) |
| Track details, grid and cue editing, replace | Inspector, expandable to a full track view |
| Similar tracks, what mixes next | Inspector > Transition suggestions, and the Next panel in Mix |
| Build and generate sets | Sets |
| Decks, coach, recording | Mix |
| Recorded mixes, scores, progress | Replay |
| Background work and undo history | Activity |
| Organise folders | Library > Organise (a tool that opens a plan) |
| Other DJ libraries, controllers, models, audio device | Settings |

Always present: the command bar (Ctrl+K: search, commands, natural language), the inspector on the right (I toggles it), the A to B dock and the player bar at the bottom, and Activity in the top bar. Activity and Settings open as sheets over the current space.

Selecting one track fills the inspector. Selecting two offers the A to B preview in the dock. Selecting many shows batch actions.

## Layout

```
+--------------------------------------------------------------------------+
| mark  RESONANCE                                 audio  Activity  Settings |
+-------------+----------------------------------------+-------------------+
| sidebar     | command bar and slash chips            | inspector         |
|             +----------------------------------------+                   |
| Library     |                                        | artwork, title    |
|  collections|  workspace                             | details, tags     |
| Prep        |  table / set timeline / decks / replay | audio traits      |
| Sets        |                                        | energy            |
| Mix         |                                        | suggestions       |
| Replay      |                                        |                   |
| dot wave    |                                        |                   |
+-------------+----------------------------------------+-------------------+
| A to B dock: deck A | crossfade waveform | deck B                         |
|  player bar                                                              |
+--------------------------------------------------------------------------+
```

At the reference size the top bar is 50 px, the sidebar 256 px, the inspector 420 px and the dock 160 px, and the player bar is a 60 px card inset 20 px at the sides and 17 px at the bottom. The tokens are in [visual-language.md](visual-language.md#sizes).

## Window sizes

The reference is `concept-dark.png`: 1586 by 992 CSS pixels at a device pixel ratio of 1. The full layout is designed from 1280 by 720 up to 5120 by 2160, at ratios 1, 1.25, 1.5 and 2. A 1920 by 1080 laptop at 150 % scaling gives exactly 1280 by 720. Below that, two waveforms, a table with title, artist, BPM, key and time, and the transport can't all fit without clipping. The desktop window can still be made as small as 760 by 480 (`app/src/main.rs`), and the narrow rules below cover it.

At every size:

- Text never scales with the window. Font sizes stay at their tokens.
- Table rows stay 33 px. A taller window shows more rows.
- Layout works in CSS pixels, and a hairline stays 1 CSS pixel at every ratio.
- A canvas sizes its drawing buffer to its CSS size times `devicePixelRatio`, rounded, and puts waveform bars on whole device pixels so they don't shimmer at 1.25 or 1.5.

The layout responds with CSS media queries and container queries. Script only holds the user's own choices: `[` for the sidebar, I for the inspector and D for the dock.

| Region | Rule |
| --- | --- |
| Sidebar | 256 px. Below 1360 px wide it collapses to a 52 px icon rail. `[` collapses or expands it at any width. It never grows past 256 px. |
| Inspector | `clamp(340px, 26.5vw, 440px)`: 340 px at 1280 wide, 420 at 1586, 440 from 1660. Below 1280 px wide it becomes a sheet over the workspace, closed until a track is selected or I is pressed. |
| Dock | 160 px: a 52 px header and the deck cards and crossfade waveform under it. Below 820 px high it collapses to its 52 px header. D or a click on the header opens it. The deck cards are `clamp(260px, 21.5vw, 340px)` and the waveform takes the rest. |
| Command bar | A 42 px search box as wide as the workspace, up to 1120 px. Slash chips hide from the right when the bar is narrower than 760 px, keeping the overflow button. |
| Workspace | Takes every pixel the other regions leave, so it is the part that grows on large screens. The table drops columns with container queries ([library.md](library.md#track-table)) and stops growing at 2400 px. |
| Player bar | The scrub bar flexes. When the bar narrows, the A and B volume sliders shrink from 86 to 64 px and hide their percentage, which shows in a tooltip while dragging. |

What that gives at common sizes:

| Window | Sidebar | Inspector | Dock |
| --- | --- | --- | --- |
| 1100 by 700 | 52 | sheet, closed | header only |
| 1280 by 720 | 52 | 340 | header only |
| 1400 by 900 | 256 | 371 | open |
| 1586 by 992 | 256 | 420 | open |
| 1920 by 1080 | 256 | 440 | open |
| 2560 by 1440 and wider | 256 | 440 | open |

## Keyboard

Everything is reachable from the keyboard. Shortcuts appear in menus and tooltips.

| Key | Does |
| --- | --- |
| Ctrl+K, or / outside a text field | Focus the command bar |
| 1 to 5, or Ctrl+1 to Ctrl+5 | Switch space |
| `[` | Collapse or expand the sidebar |
| I | Show or hide the inspector |
| D | Collapse or expand the dock |
| Space | Play or pause the preview in Library, or the focused deck in Mix |
| Up, Down, Page Up, Page Down, Home, End | Move through the table |
| Shift with Up or Down | Extend the selection |
| X | Tick or untick the row |
| Enter | Load the row into the preview or the active deck |

Focus shows as a 2 px outline in the control border colour, and in the system text colour under Windows high contrast. Active decks and drop targets also get focus brackets.

## Interaction rules

- No chains of modals. Anything that changes files uses one confirm sheet that shows the diff.
- Ctrl+Z works for every journaled operation. Activity lists past operations with an undo on each.
- Background work never blocks. A job that needs the user becomes a Prep item.
- Every view has designed empty, loading and error states. An error says what happened and what to do next.
- Dense by default: 33 px rows and tabular numerals. Don't swap tables for decorative cards.
- Honour the OS settings for reduced motion, more contrast and Windows high contrast.

## Showing where a value came from

- Measured: plain.
- Estimated by an algorithm or model: the value with a small hollow dot. Hover shows the source, version and confidence.
- Confirmed by the user: a small tick.
- Low confidence: a muted value, plus a Prep item.

One component does this, `Provenance.svelte`. Every data field uses it.

## Names

Use plain words: Library, Prep, Sets, Mix, Replay, Activity, Settings, Collections, Coach, Replace, Organise, Identify.

Prep replaces the brief's Inbox and Replay replaces its Review ([#49](https://github.com/BradHudsonGrant/RESONANCE/issues/49)). The boards' own navigation names, such as Discover, Transitions, Set Builder, Practice, Listen and Explore, are not used.

Inbox and Review are retired as names. So are the names from the old chats: Job Orchestrator, Disk Steward, Sonic Constellations, Anchor, Catalyst, Intelligence.

Interface text uses British spelling, as v3 did. Code uses whatever the language ecosystem expects.
