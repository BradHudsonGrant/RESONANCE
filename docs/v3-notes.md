# Notes from DUBPLATE v3

Read on 12 November 2026 from `D:\Music\ZMusic Manager` and a scratch copy of `%APPDATA%\com.bradd.v3\library.db`. Nothing there was changed and no code was copied. v3 is reference only. Nothing is migrated from it.

## What v3 was

A Tauri 2 desktop app with a Svelte 5 UI and one Rust crate behind it. About 40,000 lines of Rust and about 100 Svelte and TypeScript files. It had no git repository.

Main dependencies: rusqlite, cpal, rtrb, rustfft, Symphonia, lofty, the `beat-this` crate on the rten backend, Signalsmith Stretch, reqwest, quick-xml, trash and walkdir. It shipped `ffmpeg.exe` and `ffprobe.exe` (about 100 MB each) and two ONNX files for beat tracking.

What it did:

- Library. Top-level folders under one music root were "crates", with kinds crate, inbox and archive. A scan walked a crate and read tags. Files that vanished were flagged missing and their rows kept.
- Analysis. Tempo, key, beats and downbeats from Beat This!, a 1 to 10 energy figure, intro and outro length, first drop, vocal and bass flags at the edges, loudness and true peak, a three-band waveform, an energy curve and sections.
- Health. A four-stage integrity scan built on ffmpeg and ffprobe, a transcode check by spectral cutoff, and a duplicate finder.
- Identify. Discogs, MusicBrainz, Bandcamp and SoundCloud lookups with a review queue, apply and revert.
- Tags. Writing tags and cover art for every format through lofty, and renaming files from a pattern.
- Convert and repair. Lossless to 320 kbps MP3 with a true-peak ceiling, and a re-encode repair for damaged MP3s.
- Intake. An inbox folder (`Unsorted`) with identify, check, convert, file into a crate, flatten and delete.
- Organise. Two classifiers (style and role in a set), a review board, move plans, apply and undo.
- Sets and mixing. A set generator, a two-deck engine with EQ, filter, loops, hot cues, sync and beat-locked crossfades, and playlist export.
- rekordbox. Import of beat grids and hot cues from `collection.xml`, into the database only.

## What was awkward

Each of these is a reason for a choice in [architecture.md](architecture.md) or [product.md](product.md).

1. Every Rust change rebuilt everything. All the Rust was one crate, and two files held half of it: `organize.rs` at 11,622 lines and `api_routes.rs` at 8,130. Incremental compilation was switched off in the dev profile to dodge an MSVC link failure, and the build script deleted `target/` after each build. `target/` is 8.2 GB today.
2. The UI talked to Rust through one command that took a method, a path and a JSON body, matched in one long function. Nothing was typed on either side, and the hand-written client was 1,572 lines. An unknown GET returned `{}` so a missing endpoint never failed loudly.
3. Rules lived in the UI. The set generator, transition scoring and two different Camelot modules were TypeScript. The two Camelot modules disagreed and both had bugs. The Rust `/sets/generate` route was a stub.
4. Things looked finished that weren't. `AUDIT_REPORT.md` lists them: eleven analysis columns that were always NULL but shown in the UI, a compatibility percentage that painted one number across four axes, random jitter in set scoring, and a grid described as per-beat that was one tempo and one anchor. In the database today, 2,805 tracks have analysis, 723 of them have loudness and 431 have a per-beat grid. Results from five analyser versions sit side by side, because nothing re-ran analysis when the analyser changed.
5. A path was the identity. `track.path` was unique, every rename or move had to repoint the row, and a file renamed outside the app became one missing track and one new one. Duplicate detection was limited to a single crate so the same track in two folders would not be flagged.
6. `D:/Music` was hard-coded as the default music root in three places and `D:/Downloads/DUBPLATE-exports` as an export folder.
7. Quarantine folders sat inside the library. The scanner skipped `_replaced` only, so files in `_converted_originals` and `_trash` could be scanned back in as tracks.
8. The journal could not account for the disk. One table, `export_action_journal`, served every file operation and carried Organise-only columns that other callers filled with placeholders. It holds two rows today, both renames, while 236 quarantined files sit on disk from earlier builds. Tag edits updated the database first and did not roll back if the file write failed.
9. API tokens were stored as plain text in the `app_kv` table.
10. Each deck held the whole decoded track in memory and stretched it offline to the set tempo. A tempo change re-stretched the whole buffer. Timing was accurate to an audio block, about 11 ms. The UI polled for the playhead.
11. Organise was built around one folder. Its taxonomy was locked to seven house playlists with invented role names, and it had been run on one crate of 444 tracks.
12. Several design documents described behaviour the code did not have, and the version numbers and names disagreed across files (`v3`, `DUBPLATE`, `dubplate-ui` 2.0.0, crate 0.1.0).

## Rules worth keeping

These are ideas to carry over, to be re-implemented and tested in Resonance. Figures are v3's starting values.

File operations:

- Write the journal row before touching the file. Check again that the source exists and the target is free immediately before the rename. Refuse to overwrite.
- One batch id per user action, so one undo reverses the whole action.
- Build new files as a temp file in the destination folder and rename at the end, so the last step is atomic on the same volume.
- If the final step fails, put the original back. Never leave a hole at the library path.
- A missing file is flagged with the date it went missing and the date it was last seen. Removing the row is a separate action the user confirms.

Duplicates and versions:

- Group by a normalised identity first: artist order ignored, featuring credits stripped, "Original Mix" treated as no version at all. Then let the audio decide.
- Same name with different audio is never resolved automatically.
- Keep-ranking was lossless over lossy, then bitrate, then size.
- v3 treated "Extended Mix" and "Radio Edit" as the same track. Resonance treats them as versions and never calls them duplicates.
- Similarity thresholds were 0.85 for the same audio, 0.72 for likely, and 0.90 before calling two differently labelled versions identical. Re-tune these on Chromaprint.

Integrity:

- Only a structural decode failure, or a proven break when compared with a trusted source, marks a file corrupt. Heuristics can only ask for a review.
- A file with no trusted original can be healthy but never verified.
- The abrupt-join detector is useless on dense percussive music and the repeated-block detector is useless on loop-based music. v3 suppressed both when they fired too often, and later downgraded three finding kinds from warning to information because they flagged too much.
- Truncation slack was 1 second or 1% of the declared duration.

Convert:

- Encode a candidate from the source, decode it and measure true peak. If it is over the ceiling (-1 dBTP), encode again from the original with a constant attenuation plus 0.1 dB. The measurement encode is never the shipped file.
- Never amplify, normalise, limit or EQ.
- Validate before publishing: full decode, duration within 0.25 s plus 1%, bitrate, sample rate, channels, tags and art, true peak, hash.
- An existing MP3 is reported on and never re-encoded.

Identify:

- Providers do no database work. Each one fails soft to zero candidates.
- One scorer ranks every source: token-sorted similarity on artist and title, a bonus for matching duration, and a small prior per source.
- Rate limits are a minimum interval per provider. MusicBrainz is one request a second and Discogs 60 a minute.
- Applying a match stores the previous values so it can be reverted.

Keys and grids:

- The 34-entry enharmonic table mapping musical keys to Camelot codes, and its parser rules: a bare `m` is minor, a bare `M` is major, and sharp and flat glyphs are accepted.
- Relations: same code, one hour either way on the same ring, and the relative major or minor are compatible. Two hours up is an energy lift. One semitone apart, which is seven hours on the wheel, is a clash.
- A grid is a list of beat and downbeat times. One tempo plus one anchor drifts: 0.1 BPM of error is about 0.28 s over six minutes.
- Kick energy cannot find the downbeat in four-on-the-floor music, because the kick is on every beat.
- Folding tempo into 90 to 180 BPM mishandles drum and bass and half-time.
- Stock Krumhansl-Schmuckler key profiles do badly on dance music. The audit recommends dance-music profiles, log magnitude, whitening, a high-pass near 200 Hz and a tuning estimate. Check this against real keys in M4 before relying on it.
- Cues are stored in the track's own time, never in stretched time.

rekordbox XML:

- `TEMPO` nodes (`Inizio`, `Bpm`, `Battito`) are grid segments and can be expanded to per-beat times.
- `POSITION_MARK` nodes numbered 0 to 7 are hot cues.
- `Location` is a percent-encoded `file://localhost/` URL.

Mixing:

- An equal-power crossfade of two kicks is muddy. Swap the low band at about 150 Hz on the downbeat.
- A crossfade asked for after its mix point should wait for the next bar.
- A UI timer is throttled when the window is hidden, so anything that must be on time runs off the engine clock.
- Signalsmith Stretch was chosen over the home-made WSOLA, which smeared kicks and collapsed stereo, and over Rubber Band, which is GPL.

Scoring:

- A missing input lowers confidence and can ask for a review. It never becomes a confident default.
- An energy curve normalised per track shows shape. Intensity comes from loudness.
- For set generation: filter to a candidate pool first, scale each cost term to 0 to 1 before weighting, make results reproducible from a seed, and space out repeats of an artist.

## Every place v3 wrote files

In its own folders:

| Where | What |
| --- | --- |
| `%APPDATA%\com.bradd.v3\library.db`, `-wal`, `-shm` | The database |
| `%LOCALAPPDATA%\com.bradd.v3\EBWebView\` | WebView2 profile and caches |
| `D:\Music\ZMusic Manager\v3\src-tauri\target\`, `v3\dist\`, `DUBPLATE\` | Build output and the portable app folder |
| `%TEMP%\dubplate_*` | Database copies made by its export tools, removed afterwards |

In the music library:

| Where | What |
| --- | --- |
| The audio files themselves | Tags and cover art, written in place on edit or when a match was applied |
| The audio files' own folder | Renames from the filename pattern |
| Next to a source file | The converted MP3, built as a hidden temp file then renamed |
| `<music root>\_replaced\restore\` | The old file before a restore. The new bytes went to the original path. |
| `<music root>\_replaced\remux\` | The damaged MP3 before a repair |
| `<crate>\_replaced\dupes\` | Duplicate copies that lost |
| `<music root>\_converted_originals\library\` | Originals after a library convert |
| `<music root>\_converted_originals\intake\` | Originals after an inbox convert |
| `<music root>\_trash\intake\` | Files deleted from the inbox |
| `<music root>\_wrong_match\` | Counted and emptied by Tidy. I found no code that writes to it. |
| Crate folders | Files moved in from the inbox, and new subfolders created for them |
| The inbox folder | Flatten moved nested files to its root and removed the empty folders |
| Style folders inside a crate | Organise apply created them and moved files in. Undo moved them back. |

Elsewhere:

| Where | What |
| --- | --- |
| The Downloads folder | Analysis, evidence, review, cluster, plan and audit exports as CSV or JSON, and set playlists as `.m3u8` with a `.m3u` beside it |
| `D:\Downloads\DUBPLATE-exports\<plan id>\` | Move plans and export packages. The folder does not exist today. |
| The Recycle Bin | Tidy sent the contents of a quarantine folder there |

## What is on disk now

For the clean-up at 1.0. Sizes measured on 12 November 2026.

| Path | Files | Size |
| --- | --- | --- |
| `D:\Music\ZMusic Manager` | | 8.7 GB, of which `v3\src-tauri\target` is 8.2 GB |
| `%APPDATA%\com.bradd.v3` | 5 | 106 MB, including `library.db.bak-20260628-173134` and its `-wal`, which no code I found creates |
| `%LOCALAPPDATA%\com.bradd.v3` | | 121 MB |
| `D:\Music\Unsorted\_converted_originals` | 197 | 6.4 GB |
| `D:\Music\New\_converted_originals` | 23 | 1.3 GB |
| `D:\Music\House\_replaced` | 15 | 120 MB |
| `D:\Music\Unsorted\_trash` | 1 | 4.7 MB |
| `D:\Music\_converted_originals\intake` | 0 | empty |

The quarantine folders are inside source folders, so Resonance's ignore rule has to match those four names at any depth.

## The library, as measured

Outside the quarantine folders, `RESONANCE` and `ZMusic Manager`, `D:\Music` holds 7,268 audio files in 113 GB: 6,126 MP3, 584 AIFF, 370 WAV, 130 FLAC and 58 M4A. The biggest folders are `Albums` (3,104 files), `Unsorted` (1,147) and `Old Music` (999). `D:\Music` also contains `rekordbox`, `PioneerDJ` and `MusicBee` folders, which the scanner and the library adapters will meet.
