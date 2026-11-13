# 0008: The library table reads pages

Date: 13 November 2026. Status: accepted as the default.

## Context

The track table has to scroll 100,000 rows at the display's refresh rate, and the library should open as fast at that size as at fifty tracks. Rows come from the core over the same command channel as everything else.

## Decision

- One command, `tracks`, returns a page of rows in library order with the size of the library. The table asks for 200 rows at a time.
- Library order is source folder, then path. The query is a `CROSS JOIN` from `source` to `asset`, which holds SQLite to reading both through their indexes in that order with no sort.
- The table draws only the rows in view plus eight above and below, in one block moved with a transform. The cells stay in place as it scrolls and only their text changes.
- The UI keeps about 60 pages, letting go of the ones farthest from the view. It reads half a page past each edge of the view, has at most two reads in flight, and drops reads for a part of the table that has scrolled away before they were sent.
- A row is built for display in the core: the title falls back to the file name, and the BPM and key tags are parsed there.

## Why

Sending the whole library at once would be about 47 MB of JSON at 100,000 rows. A page of 200 is about 94 kB and comes back in 6 to 14 ms.

Left to choose its own plan, SQLite sorted every row for every page. The last page of 100,000 took 585 ms. With the join order fixed it takes 13 ms, and the first page under 2 ms.

The core answers one command at a time on a connection. Dragging the scrollbar asked for hundreds of pages, and the page in view waited behind all of them. Two reads in flight keep the core busy without building a queue.

Rebuilding the cells when a row waited for its page made a scrollbar drag cost 9.6 ms a frame at the 99th percentile. Changing only the text brought that down to 1.4 ms.

## What follows from it

- An offset into the library means something only until the library changes. Tracks added by a scan while the table is open can make pages overlap or skip rows until the next reload.
- Search and sort in M1 change what defines the order. They will most likely hand the table a list of track ids, and the table will read rows by id the same way it reads pages now.
- The walk to a deep offset grows with the library: 13 ms at 100,000 rows. A list of ids removes that cost too.

## What would change it

A command channel that can answer several reads at once would let more than two be in flight. Sort and search needing the order held somewhere other than SQLite would move the paging there with it.
