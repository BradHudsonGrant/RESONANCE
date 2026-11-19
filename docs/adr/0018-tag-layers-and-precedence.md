# 0018: Tag layers and which one wins

Date: 18 November 2026. Status: accepted.

## Context

[architecture.md](../architecture.md#data-model) keeps tags in three layers, and [product.md](../product.md) promises that a value the user typed is never overwritten. Identification now finds artist, title, mix, album, label, catalogue number and year online, and M5 analysis will find key and BPM. Each needs a place to go that never hides or replaces what the user chose, and the library has to show the result straight away, not after a write to the file.

## Decision

- `file_tag` holds what is in the file. Only a scan writes it.
- `proposed_tag` holds one proposal per track and field: value, provider, confidence, whether Chromaprint matched the audio, and a status of pending, accepted or skipped. Only identification writes it. A proposal that matches what the track already shows is not stored, because there is nothing to decide.
- `user_tag` holds what the user typed or accepted, with `dirty_on_disk` set until the value is in the file. Only a user action writes it. An empty value blanks the file's value.
- What a track shows is the user's value, then analysis (from M5), then the file's tag. `effective_tags` in `crates/core::staging` is the one place that rule lives. The asset's sort, filter and `FTS5` columns are refreshed from it in the same transaction as the edit, and a rescan lays the user's values back over the new file tags.
- A run of identification never writes `user_tag`. A proposal for a field the user holds is shown as locked. "Accept everything above" skips it. Accepting that one field is still allowed, because the user picked it.
- A new run keeps a decision when it proposes the same value again, and makes a different value pending.
- Keys are stored as Camelot codes. "Am", "8A" and "1m" (Open Key) all become `8A`. BPM is stored as a number and a year as four digits; anything else is refused with a message saying what fits.
- `mix`, `subgenre` and `catno` join the tag fields. `catno` is read from files that carry a catalogue number. No tag format has a common frame for mix or subgenre, so those live in the library until tag writing decides how to store them.

## Why

Three tables rather than a column per layer keep each writer to its own table, so "identification never touches a user value" holds by construction rather than by care in every query. Refreshing the denormalised sort and search columns on each edit keeps sort and search on their existing indexes, inside the 50 ms text search budget, instead of joining the user layer into every query of a 100,000-row library.

## What follows from it

- Writing tags reads `user_tag` where `dirty_on_disk = 1`, writes the file, re-reads `file_tag` and clears the flag.
- When two tracks merge after a relink, the user's values follow the surviving track, as favourites and collections already do.
- Analysis results get their own tables in M5 and slot in between the user and the file.

## What would change it

A provider that returns several plausible values for one field that the user needs to pick between, which would need more than one proposal per field.
