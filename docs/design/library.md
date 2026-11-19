# Library

The Library space is the track table and the inspector beside it: `Library.svelte`, `TrackTable.svelte` and `Inspector.svelte` in `ui/src/lib/library/`. [#52](https://github.com/BradHudsonGrant/RESONANCE/issues/52) builds them to this page. Sizes are at the reference window, 1586 by 992.

The table is the library. It starts right under the filter pills, with no collection cards above it and no switch to a card view.

## Header

- The title "Library" in 27 px Space Grotesk 500, and the track count beside it in 13 px mono and Text 2.
- On the right, a 34 px filter button with a sliders icon (the filter drawer arrives in M2), and the scan form while the library is empty.
- 16 px under the title, the filter pills: All tracks, Analysed, Unanalysed, Favourites and Recent, 24 px apart. The active pill is inverted. The others are Text 2 with a 13 px icon. #52 makes them toggle. The filters behind them arrive in M2, and Analysed and Unanalysed need M5.

## Track table

A virtualised table: a 31 px header and 33 px rows, with 1 px lines between rows. Only the visible rows and a small margin are drawn, so 100,000 rows scroll within the 16 ms frame budget ([#40](https://github.com/BradHudsonGrant/RESONANCE/issues/40)). Column widths and drop rules live in `ui/src/lib/library/columns.ts`.

| Column   | Width                     | Drawn as                                                            | Drops                                    | Built in                  |
| -------- | ------------------------- | ------------------------------------------------------------------- | ---------------------------------------- | ------------------------- |
| Tick box | 30 px                     | 16 px box, 3 px corners, control border                             | never                                    | #52                       |
| Status   | 22 px                     | 14 px icon: play on the playing or selected row, tick when analysed | never                                    | #52 for play, M5 for tick |
| #        | 36 px                     | 12 px mono, right aligned, Muted                                    | never                                    | #52                       |
| Title    | `minmax(150px, 1.8fr)`    | 13 px sans 500, Text, cut with an ellipsis                          | never                                    | #52                       |
| Waveform | 140 px, 88 px when narrow | `Waveform mode="inline"`                                            | narrows below 740 px, hides below 640 px | #52, peaks from M5        |
| Artist   | `minmax(120px, 1.3fr)`    | 13 px sans, Text 2                                                  | never                                    | #52                       |
| BPM      | 58 px                     | 13 px mono, right aligned                                           | never                                    | #52, analysed in M5       |
| Key      | 52 px                     | 13 px mono, in the notation chosen in Settings                      | never                                    | #52, analysed in M5       |
| Energy   | 54 px                     | `MeterBar`                                                          | hides below 820 px                       | #52, analysed in M5       |
| Time     | 56 px                     | 13 px mono, right aligned, Text 2                                   | never                                    | #52                       |
| Actions  | 56 px                     | 24 px favourite button and 24 px more button                        | never                                    | #52 for more, M2 for star |

The table is a container (`container-type: inline-size`), so it drops columns by its own width, whatever narrowed it: the window, the sidebar or the inspector. Below 820 px Energy goes first, and energy stays in the inspector. Below 740 px the waveform narrows to 88 px. Below 640 px it goes, and the track's waveform stays in the dock. Every other column stays at every size.

A selected row sits on Raised inside a 1 px control border frame with 6 px corners. Its tick box is filled in Text with a dark tick, its status cell shows the play icon, and the 2 px accent mark sits at its left. The accent goes nowhere else on the row.

## Selection actions

Selection never moves the table. While any track is ticked, the selection actions take the place of the filter pills, in the same slot: the count ("3 selected"), Select all, Add to collection, Remove from the open collection, and Clear. When the selection clears, the pills come back. Nothing is inserted above the table, so the header and every row stay where they were and the pointer stays on the row it clicked. No bar, chip row or banner may appear above the table and push it down.

- The pills stay in the layout, hidden, so the slot keeps their height.
- Select all stays in place and turns disabled once every track is ticked. The count uses tabular figures and a fixed minimum width, so going from 9 to 10 does not move the buttons.
- Escape clears the selection unless a menu or sheet is open, and focus goes back to the table. The count is a polite live region.
- In forced colours the actions carry a 2 px Highlight mark at their left, so they read as different from the pills.

## Inspector

The column on the right, 420 px at the reference size, on Surface with a 1 px border on its left. It has no title bar: it opens straight into the track. Sections are split by inset 1 px dividers.

- Track: 96 px artwork with 10 px corners, the title in 20 px Space Grotesk 600 and the artist in 14 px Text 2, star and more buttons on the right, then an Analysed status pill and the audio format (for example `WAV 44.1 kHz 24-bit`) in 12 px mono. Artwork arrives in M3. Until then the artwork is a still frame of the dot wave seeded from the track.
- Details: two rows of mono labels over 13 px values. BPM, Key, Time and Release, then Genre, Subgenre and Label. File facts show from #52, tags from M3, BPM and key from M5.
- Tags: removable 26 px tags and a round add button. Editing arrives in M3.
- Audio traits: five `TraitBar` rows (Danceability, Energy, Warmth, Vocal presence, Groove), filled by M5 and M6.
- Energy: an Energy pill over the energy curve (`Waveform mode="curve"`) with five time labels under it. The curve arrives with M5.
- Transition suggestions: three rows, each with a small play button, title and artist, a mono line saying why it fits, a score badge and a more button, and a "View all" link in the header. Matches arrive in M6 and transition scoring in M10.

Below 1280 px wide the inspector is a sheet, closed until a track is selected or I is pressed ([ux.md](ux.md#window-sizes)).

## Left out from the board

`concept-dark.png` shows three things the library doesn't have:

- The Table and Card view switch. The library is always a table.
- The Tension column, and the Tension tab beside Energy in the inspector. Tension is not a planned analysis output.
- The four collection cards and the New collection card above the table. They move to Sets (M10), and in Library collections are listed in the sidebar.

[ADR 0012](../adr/0012-concept-boards-as-design-target.md) records why.
