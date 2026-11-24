# Prep

Prep is where new tracks get their tags. Every track in the view sits in one list, and each one has a status.

## Status

| Status       | What it means                                                                          |
| ------------ | -------------------------------------------------------------------------------------- |
| To review    | An online match's suggestions are waiting for a decision                               |
| Not searched | No online search has run for the track. Fixes from its own tags or name may be waiting |
| No match     | An online search ran and found nothing usable. Fixes from the file may be waiting      |
| Done         | The track was matched and its suggestions have all been saved or skipped               |

The rule that gives a track its status lives in `crates/core/src/prep.rs` (`PrepStage`). The toolbar sums the statuses up in a line, such as "36 to review · 395 not searched". Sorting by the Status column lists them in the order above, then the other way round.

**Find tracks** narrows the list by name, and the filter beside it narrows to tracks missing one tag. A selection sent from Library shows as a button that clears it.

## Finding matches

**Find matches** searches every track not yet searched, or only the ticked tracks when any are ticked. The button names how many tracks it will search. While a search runs, a strip under the toolbar shows how many tracks are done, the rows being searched say Searching, and the rest of Prep stays usable. When it ends, the strip says how many tracks are ready to review and how many found nothing, and offers to list them first.

A track the catalogues do not know, such as an edit sold only on Bandcamp, is looked up on the Bandcamp page its names point at: first the Bandcamp sellers that other tracks sharing its artist, remixer or mix name were matched to, then the remixer or the name in the mix, the artist, the label and the release named in the title, with the title as Bandcamp writes it in the address. An artist who sells under another name is found once one of their tracks is matched, by search or by a pasted link. Bandcamp's own search is not used: its rules for automated visitors exclude it.

A title that ends in the release it came from, such as `Track [Label Vol.4]`, is searched without it, and the release becomes the album hint.

A track counts as searched once a search has run, even if one provider failed. The next search tries it again.

The checkboxes are for working on many tracks at once. Sorting keeps the ticks. Changing the filter, the find text or the Library selection clears them.

## The list

Each row shows the cover (the suggested one if there is one), title and artist, then Genre, Label and Year, then its status in two short lines. A match says whether the audio or only the names matched, then its confidence and how many suggestions wait, such as `100% · 3 to accept`. A track with only fixes read from its own tags or name says Not searched, then `2 fixes from filename`, so a tidy-up is never mistaken for a match. An empty column shows a dash, and a missing artist, title or cover is named under the artist. Clicking a column header sorts by it, A to Z, then Z to A, then back to Prep's order. Status sorts by stage. As the list narrows, Label goes first, then Genre and Year, and the missing tags move into the line under the artist. The order is fixed when the list loads or is sorted. Saving, skipping or searching changes a track's status in place, so nothing moves while the user works; sorting again brings the new order. There are no pages. The list is as tall as every track in the view from the start, and only the rows in sight are drawn and fetched, so the scrollbar stays put and a long library stays quick.

## Review

The review pane answers one question first: is this the right release?

- The cover sits at the top, larger than anything else, beside the title, artist and release.
- Under them, the confidence and the evidence in plain words: whether the audio fingerprint matches, how the length compares and which providers agree. A match on names alone says to listen before saving.
- **Tags** lists every tag a file can hold, the file's value beside the suggested one, with Missing where the file has none. Each suggestion has its own **Accept**, and **Accept all** takes every one. The mix is part of the title, as stores and DJ software write it, so a remix or edit never leaves the title. A suggestion that differs from the file only in letter case is not offered. Fields the user typed are never offered and are listed as kept.
- **Matches** lists every release the search found, numbered, each with its cover. The one in use says so and the others say Use. Choosing another one, or pressing its number, updates the suggestions.
- **Cover** shows the file's cover and each match's cover at a size worth judging. Choosing a match's cover suggests it; choosing the file's keeps it. Match covers are fetched for viewing when the track opens and are not suggested until chosen.
- **Not the right release?** takes a pasted release page and looks the track up from it.
- **Search details** shows what each provider returned, and links to provider settings when a key is missing.

A track that has not been searched offers **Find a match**. A track with no match offers **Search again** and the link box.

The footer holds the actions. **Accept N changes** writes every suggestion, so the track leaves To review. **Skip** sets every suggestion aside. **Edit tags**, **Write edits** (when library edits are waiting for the file) and **Rename file** apply to the ticked tracks, or to this track when none are ticked. After a save, **Undo** restores the files. Activity keeps the history.

If a file has changed or cannot be written, Prep lists it with the reason and keeps its suggestions waiting.

## Keyboard

| Key                    | Action                                                                |
| ---------------------- | --------------------------------------------------------------------- |
| `2` or `Ctrl+2`        | Open Prep                                                             |
| `↑` or `↓`             | Move between tracks                                                   |
| `Shift+↑` or `Shift+↓` | Tick tracks while moving                                              |
| `X`                    | Tick or untick this track                                             |
| `Ctrl+A`               | Tick or untick every track in the list                                |
| `Enter`                | Accept every suggestion, or search a track that has not been searched |
| `S`                    | Skip this track's suggestions or fixes                                |
| `1` to `9`             | Choose that match                                                     |
| `F`                    | Find matches for the ticked tracks, or this track                     |
| `Shift+A`              | Save every suggestion for the ticked tracks                           |
| `E` or `B`             | Edit tags                                                             |
| `W`                    | Preview edits before writing them to the files                        |
| `Space` on a track     | Preview it                                                            |
| `Ctrl+Z`               | Undo the last save                                                    |

## Layout

The list and the review pane share the workspace equally. At 800 px of workspace or less the list takes the full width, a track opens its review, and Back to tracks or Escape returns to the list. Below 520 px of review width the cover stacks above the title and the tags show only the suggested value. The shell supplies the dark, light and Windows high-contrast themes.
