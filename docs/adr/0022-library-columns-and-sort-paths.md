# 0022: Library columns and their sort paths

Date: 19 November 2026. Status: accepted.

## Context

The library table shows five columns. DJs want to add others, from the album and label to the bitrate, the file's folder and any tag the library reads, and to sort by any of them. On 100,000 tracks a sort that joins `file_tag` at query time sorts every row and takes hundreds of milliseconds, which is why the M2 sort columns are copied onto `asset` and indexed. The table's layout has to be the same after a restart, and its rules belong in `crates/core`.

## Decision

- `crates/core/src/columns.rs` holds the columns, their groups, default widths and the layout rules: which columns a new table shows, that the title stays, how a header click cycles a sort, that `Shift` adds one secondary sort, and the order picked columns give way on a narrow table. The core answers each change with the whole layout, and the UI draws what it is given.
- The layout lives in three tables in `library.db` (`table_column`, `table_sort`, `table_setting`), so it travels with the library. No rows means the default table.
- Every fixed column sorts from a column on `asset` holding the value the track shows. Composer, remixer, mix, catalogue number and comment join the M2 columns there. A column that can be empty has an index in each direction, so empty values go last both ways without a sort.
- A tag with no column of its own sorts from `asset_tag_value (field, sort_value, asset_id)`, one row per tag a track has, kept with the asset's other columns whenever its tags change. The row carries a copy of the asset's `included` flag, which a trigger keeps in step, because looking each asset up took 200 ms on 100,000 rows. Tracks with the tag are read from that index, then tracks without it from an index of included assets, as two reads: written as one `UNION ALL`, SQLite sorted every row.
- Composer and remixer become tags the library reads and writes, from the composer and remixer frames of each tag format.

## Costs

The sort indexes make a 100,000-track library 56 MB larger, 167 MB instead of 111 MB. A generic tag with a secondary sort still sorts every row, 183 ms on 100,000 tracks; a fixed column with a secondary sort sorts only within runs of equal values.

## What would change it

A library where the extra indexes slow a scan noticeably, or a need for more than two sort keys.
