# Shell

The shell is the frame around every space: the top bar, the sidebar, the command bar, the A to B dock and the player bar. It lives in `ui/src/App.svelte` and `ui/src/lib/shell/`, and [#51](https://github.com/BradHudsonGrant/RESONANCE/issues/51) builds it. Sizes below are at the reference window, 1586 by 992. How each region changes with the window is in [ux.md](ux.md#window-sizes). Where a part needs data the app doesn't have yet, the milestone that brings the data is named next to it.

## Top bar

50 px tall, full width, on Surface with a 1 px border under it.

- Left, 20 px from the edge: the mark (19 by 21) and the RESONANCE wordmark (16 px), 18 px apart.
- Right, 20 px from the edge: three 28 px icon buttons with 18 px icons in Text 2, 30 px apart centre to centre.
  - Audio status. Opens the audio device and latency popover. Live in M7.
  - Activity. Opens the job queue and undo history as a sheet. Live in M2. Each active job has Pause (Resume once paused) and Cancel. A paused job stops at its next file or batch, keeps its progress and stays paused after a restart.
  - Settings. Opens settings as a sheet. #51 gives it the theme switch (Dark, Light, System). Sources arrive in M2, provider keys and the key notation (Camelot, Open Key, Musical; #126) in M3, and audio and MIDI devices in M7.

Activity and Settings sit here, as `concept-dark.png` shows, and not at the foot of the sidebar ([ADR 0012](../adr/0012-concept-boards-as-design-target.md)).

## Sidebar

256 px wide on Surface, with a 1 px border on its right. From the top:

- The two-line mono label MUSIC INTELLIGENCE / FOR DJS (11 px, Text 2, 15 px line height) and a quiet collapse button (`[`).
- The five spaces as 232 by 38 buttons with 8 px corners, a 16 px icon and a 14 px label. The active space sits on Raised with a 1 px border and the 2 px accent mark at its left. Library shows the track count in 12 px mono. Inactive spaces are clear, in Text 2, and turn Text on hover.
- Under Library, the collections: a Collections row with an add button, then one 27 px row per collection with its count in 12 px mono and Muted. Live in M2.
- An Active set block under the spaces: a divider, the ACTIVE SET label and the current set with a 6 px status dot in Text 2. It arrives with sets in M10.
- The dot wave at the foot, full width and 78 px tall, fading upward into the surface.

Collapsed, the sidebar is 52 px wide and shows only the five space icons as 38 px buttons, each with a tooltip. The label, collections, active set and wave are hidden.

## Command bar

At the top of the workspace, closed by a 1 px border under it.

- The search box: 42 px tall, 12 px below the top bar, on Surface with a 1 px control border and 10 px corners. A 16 px search icon, a 14 px placeholder ("Type a command…" until library search arrives in M2, then "Search your library, or type a command…"), and a Ctrl K badge on the right (⌘ K on macOS). #51 builds the box and its focus. Text search arrives in M2 and natural-language search in M6.
- Slash chips, 12 px below the box and 12 px apart, with an overflow button at the end. Clicking one puts its command in the box and focuses it.

| Chip | Description | Works from |
| --- | --- | --- |
| `/ bpm 125-132` | Filter by tempo | M2 |
| `/ key 8A` | Compatible with A minor | M5 |
| `/ similar` | Find matching tracks | M6 |
| `/ build set` | Generate 90m progression | M10 |

## A to B dock

160 px tall above the player bar, with a 1 px border above it.

- A 52 px header, 20 px in from each side: "A → B" in 16 px Space Grotesk 600, "Transition" and an info icon, then the Mix point (for example "4:15 (60%)") and Blend (for example "Crossfade") as quiet selects. The controls work from M10.
- Under it, three columns 12 px apart: deck A's card, the crossfade waveform and deck B's card. The cards are `Deck variant="dock"`: 340 by 87 with 10 px corners on Surface. The waveform is `Waveform mode="crossfade"`, 101 px tall, and takes the remaining width.

Live decks and the crossfade arrive in M7 and M10. In Mix the dock folds away, so the scrolling waveforms aren't shown twice ([mix.md](mix.md)).

## Player bar

A card, not a strip: 60 px tall, 12 px corners, on Surface with a 1 px border, inset 20 px at the sides and 17 px at the bottom. From left to right:

- A 44 px round play button on Surface 2 with a control border, and a 28 px pause button.
- While a track is loaded, its 36 px artwork `Thumb` before the title and artist.
- Elapsed and total time in 12 px mono, the elapsed part in Text and the total in Text 2.
- The scrub bar, which takes the free width.
- A Vol and B Vol, each a 12 px mono readout and an 86 px slider.
- Curve and EQ selects, 26 px tall.
- A 33 px vertical divider and the headphone cue button.

Preview playback, the scrub bar and volume arrive in M2. The A and B volumes, curve, EQ and cue arrive in M7 and M10.
