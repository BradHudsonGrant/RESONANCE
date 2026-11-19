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
| Status   | 22 px                     | 14 px icon: sound lines on the row being heard, play on the loaded or selected row, tick when analysed | never                                    | #52 for play, M5 for tick |
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

The selected row is the one with focus, where the arrow keys move from. When several rows are ticked, the others sit on Surface 2 with a 2 px mark in the control border colour, so the focused row still reads apart from the ticked ones. The row being heard shows sound lines in its status cell in place of the play icon. In forced colours the focused row's frame and mark are Highlight and a ticked row's mark is CanvasText.

## Auto-preview

Auto-preview plays each track as the user moves through the table, so a crate can be heard by holding the down arrow. It is off by default. The Auto-preview button in the player bar or `P` turns it on and off, and the choice is kept in the library database, so it is the same after a restart. Turning it on plays the row the user is on.

With it on, a plain click on a row or a move with the arrow keys, Page Up, Page Down, Home or End swaps the preview to that row through the same 5 ms fade a seek uses. A row that is already playing carries on, and a paused one resumes. A row whose file is missing keeps the last track playing, so arrowing past one does not cut the sound. `Shift` and `Ctrl` clicks build a selection and do not change what plays. A double click plays the row and never pauses it. `Space` still pauses and plays.

Holding a key never queues loads: one is in flight at a time, and only the latest row waits behind it. There is no timer, so the first press is heard at once.

Settings has Previews, for where an auto-preview starts: the beginning, 30 % in, or a fixed time in seconds. 30 % in is the default, past most intros. A fixed time longer than the track plays it from the beginning. The rules live in `crates/core/src/preview.rs`.

## Columns

The table opens with Title, Artist, BPM, Key and Time, the same five a new library has always had. Every other column is one step away and never shown until the user picks it. The rules live in `crates/core/src/columns.rs` and the choice is kept in the library database, so it is the same after a restart.

### Column chooser

A 24 px columns button sits in the header cell above the row actions, and right-clicking anywhere on the header opens the same menu under the pointer. It is a `role="menu"` popover on Surface with a 1 px control border:

- A search field at the top. Typing narrows the list by column name.
- The columns in five groups, each under a mono label: Track (Title, Artist, BPM, Key, Time, Composer, Remixer, Mix, Comment), Release (Album, Genre, Label, Catalogue number, Year, Artwork), Audio (Format, Bitrate, Sample rate, Bit depth, Channels), File (File size, Date added, Date modified, Path, Folder) and Tags. Tags lists the tags the library holds that have no column of their own, as "Tag: Album artist", "Tag: Track number" and so on, and only the ones at least one track has.
- Each column is a `menuitemcheckbox`. Title is always ticked and cannot be cleared.
- Under the list, "Wrap long text" as a checkbox, and "Reset to default", which puts back the five columns, their widths, library order and no wrap.

The menu takes focus on its first item, and `ArrowDown`, `ArrowUp`, `Home` and `End` move through it. `Escape` closes it and gives focus back to the button or the header that opened it. A ticked or cleared column changes the table at once and the menu stays open.

### Order and width

A header can be dragged left or right to move its column, and a 2 px line in Text shows where it will land. With a header focused, `Alt+Left` and `Alt+Right` move it one place.

Every column has a 6 px resize handle on its right edge, a `separator` in the tab order with its width as its value. Dragging it, or `ArrowLeft` and `ArrowRight` while it has focus (8 px a step, 1 px with `Shift`), sets the width between 40 and 640 px. A double click, or `Enter` on the handle, fits the column to its widest value in view. Title and Artist share the room the other columns leave until the user gives them a width.

### Column states

- A header reads in Text 2. Hover and a sort turn it to Text.
- A sorted header shows an 11 px chevron. A secondary sort shows a smaller chevron and a mono "2" beside it, so the two read apart.
- A header being dragged sits on Raised with the control border. The rest keep their place until it lands.
- A resize handle shows a 1 px control border line on hover, on focus and while dragged. At rest it is invisible, so the header stays calm.
- Artwork is a 24 px thumbnail with 4 px corners, the same picture as the inspector. A track without one shows an empty Surface 2 square, so the rows keep one shape.

### Sort

A click on a header, or `Enter` on it, sorts by that column, then reverses it, then goes back to library order. `Shift` with either adds the column as the secondary sort, which orders the tracks that tie on the first, and cycles it the same way without touching the first. `#` reverses library order, and sorting by it clears any other sort. Empty values go last in both directions. Path and Folder both sort by folder and then by file, which is the library order. Artwork does not sort.

### Density and wrap

Wrap is off by default. Off, every row is 33 px, a value too long for its column ends in an ellipsis and its full text shows in a tooltip. On, a row grows to fit its longest value up to three lines, and anything longer is cut on the third line with an ellipsis. Rows that wrap are measured as they are drawn, so scrolling stays smooth and the scroll bar settles as the table is read.

### Narrow tables

The defaults still drop the way the track table above says. Columns the user picked give way first, the last picked first, as soon as they do not fit at their widths. When any have dropped, a "+N" mono button sits beside the columns button and opens the chooser, where each dropped column is marked "Hidden, too narrow". The columns come back in reverse order as the table widens. Nothing moves above the table, and the header keeps its height.

## Selection actions

Selection never moves the table. While any track is ticked, the selection actions take the place of the filter pills, in the same slot: the count ("3 selected"), Select all, Add to collection, Rename, Remove from the open collection, and Clear. When the selection clears, the pills come back. Nothing is inserted above the table, so the header and every row stay where they were and the pointer stays on the row it clicked. No bar, chip row or banner may appear above the table and push it down.

- The pills stay in the layout, hidden, so the slot keeps their height.
- Select all stays in place and turns disabled once every track is ticked. The count uses tabular figures and a fixed minimum width, so going from 9 to 10 does not move the buttons.
- Escape clears the selection unless a menu or sheet is open, and focus goes back to the table. The count is a polite live region.
- In forced colours the actions carry a 2 px Highlight mark at their left, so they read as different from the pills.


## Renaming files

Rename files, in the header beside Scan folder, opens the Rename sheet for the tracks the table shows: the open collection, the filtered view, or the whole library. Rename in the selection actions opens it for the ticked tracks. The sheet names the scope and the file count first, then the template field, its tokens, and a preview of up to 50 files with problems first: old name, new name and a status (Ready, No change, Name taken, No artist, Folder path too long, File missing). A line under it counts ready, unchanged and blocked files. The primary button says what it will do ("Rename 1,240 files") and is off when nothing is ready. After a rename, or once a large batch has finished in Activity, the sheet offers Undo for the last rename.

Settings has the same template field under File names, previewing the first three tracks. It holds the template the sheet starts from. Nothing is renamed from Settings.

## Inspector

The column on the right, 420 px at the reference size, on Surface with a 1 px border on its left. It has no title bar: it opens straight into the track. Sections are split by inset 1 px dividers.

- Track: 96 px artwork with 10 px corners, the title in 20 px Space Grotesk 600 and the artist in 14 px Text 2, star and more buttons on the right, then an Analysed status pill and the audio format (for example `WAV 44.1 kHz 24-bit`) in 12 px mono. The artwork is the user's picture, else the file's front cover, and a track with neither shows a still frame of the dot wave seeded from the track. The artwork is a button: it opens a picker for a JPEG or PNG, and takes one dropped on it, with a dashed control border while a file is over it. A picked picture shows at once and goes into the file with the next tag write ([ADR 0021](../adr/0021-artwork-store-and-layers.md)).
- Cover: only while identification has a release cover waiting. A 60 px thumb of it, a mono Cover label, "Release cover from MusicBrainz" (or Discogs) in 13 px Text 2, and Use cover and Skip buttons. Use cover makes it the track's picture.
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
