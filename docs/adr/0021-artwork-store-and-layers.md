# 0021: Artwork store and layers

Date: 19 November 2026. Status: accepted.

## Context

The inspector and the player bar show a track's cover, and identification can find one for a track that has none. An album of twenty tracks usually carries the same picture in every file, so keeping a copy per track wastes space, and 100,000 tracks cannot carry their pictures inside the JSON of a track page. Tags already sit in three layers with the user's value over the file's ([ADR 0018](0018-tag-layers-and-precedence.md)), and only an explicit tag write changes a file ([ADR 0019](0019-writing-tags-into-files.md)).

## Decision

- Pictures are kept once, in an `artwork` folder beside `library.db`, named by the BLAKE3 hash of their bytes: `<hash>.jpg` or `<hash>.png`. The `artwork` table records each picture's kind and size. A picture is written beside its final name and renamed into place, so no reader sees half of one.
- Only JPEG and PNG up to 10 MB are kept, and the kind comes from the bytes, not from the tag's MIME type. Those are the formats every tag format and DJ hardware can show.
- The scan reads the front cover, or the first picture when none is typed as the front, and stores its hash on the asset. Files scanned before this change are read for their cover once on the next scan.
- The track shows the user's picture (`user_artwork`) over its primary file's. A picture the user picks or drops in the inspector goes there at once.
- Identification proposes a picture only for a track that shows none. MusicBrainz releases name the Cover Art Archive's 500 px front cover when the archive holds one, and Discogs releases name their primary image, which Discogs lists only with a token. The picture is fetched at the provider's pace, stored, and staged in `proposed_artwork`. A release's tracks share one fetch. Accepting copies it into the user layer.
- A user picture reaches the file with the next tag write. It replaces the front cover alone (the first `covr` image in an M4A, the front cover block in a FLAC) and leaves other pictures as they are. The cover it replaced is stored first, so undo puts it back.
- The page loads a picture by hash: `/artwork/<hash>` on the local socket, behind the same token as the WebSocket, and `artwork://<hash>` in the desktop app. A picture never changes under its hash, so the browser may cache it for good. Anything that is not a 64-character lowercase hex name is refused before it reaches the file system.

## What would change it

A format the hardware gains, such as WebP, or a library large enough that unused pictures need cleaning up. Pictures nothing points to are kept for now, so undo always has the old cover.
