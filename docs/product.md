# What Resonance is and what it does

Resonance is a desktop app for DJs. It looks after a music library and helps the person using it get better at mixing. It covers the loop a DJ repeats every week:

1. Get new music in, work out what it is, tag it, check the quality.
2. Know the library: search it, see what goes with what.
3. Prepare tracks: grid, cues, phrases.
4. Plan a set.
5. Mix or practise, with coaching if they want it.
6. Review a recorded mix and see what to fix.
7. Send everything to the gear they play on.

Most DJs will keep playing on their own hardware. Resonance is where they prepare, practise and review. They can also mix inside it with a keyboard and mouse, a gamepad or a MIDI controller.

Five rules shape every feature:

1. The files belong to the user. Opening the app changes nothing on disk. Any change to a file (tags, rename, replace, move) is previewed, written to a journal, and can be undone.
2. The app does the slog and the user approves. Proposals come in batches with a confidence figure, and one key accepts them.
3. A guess never looks like a fact. Measured, estimated and user-confirmed values are drawn differently everywhere.
4. Each job has one home. No feature reachable from three menus, no second screen that does nearly the same thing.
5. It works on 3,000 tracks today and 100,000 later, with any folder layout and any mix of formats and sample rates.

## Decisions already made

These are settled. Don't reopen them.

| Topic | Decision |
| --- | --- |
| Project folder | On the development laptop the repo is `D:\Music\RESONANCE`, inside the music library, so the scanner must ignore the app's own folder. |
| GitHub | `BradHudsonGrant/RESONANCE`, private, default branch `master`. The account is on the Free plan. |
| Stack | Picked on merit in [architecture.md](architecture.md). Runtime performance comes first, and day-to-day work must not mean rebuilding everything. |
| Authorship | Commits, PRs and releases carry the author's name only. |
| Old app | DUBPLATE v3 is read-only reference. Nothing is migrated from it. What it taught us is in [v3-notes.md](v3-notes.md). |
| Old app cleanup | Only at the very end, from an approved inventory, and into the Recycle Bin. |
| Platforms | Windows first. Don't do anything that blocks macOS or Linux later. |
| Online services | Free ones by default. Paid ones are optional and use the user's own key. |
| Deleting | To the OS trash. Permanent delete only where no trash exists, after a confirm. |
| File moves | Never on the app's own initiative. Renaming in place and writing tags are fine when the user applies them. An opt-in Organise feature can propose and apply folder changes. |
| Other DJ libraries | Detect what the user has, show a toggle for each in Settings, all on by default. |
| Build order | Library, then analysis, then mixing, then coaching. |
| Look | The dark concept image is the mood: clean, monochrome, pixel-sharp waveforms, 3D dot waves, one orange accent. The layout in [design/ux.md](design/ux.md) takes priority over the layout in the image. |
| Inputs | Keyboard and mouse first, then gamepads (Xbox-style and Steam controllers). The first hardware mapping is the Rane System One, which is on hand for testing. Any other deck after that. |

Anything else in these docs that is marked "default" is a recommendation. It can change when there is evidence, and the reason is written down as an ADR in [adr/](adr/).

## Features

### Sources and library

The user adds one or more folders. Each one has "include subfolders" (on by default), a folder tree where any subfolder can be switched off, and "watch for changes" (on by default).

A scan reads tags and file facts and never writes. Tracks appear in the table within seconds. Deeper analysis carries on in the background.

Ignore the app's own folder, hidden folders, and v3's quarantine folders (`_replaced`, `_wrong_match`, `_trash`, `_converted_originals`). The ignore list is editable.

A track is a recording. An asset is a file. Identity must survive rename, move, retag and replacement, so it comes from a hash of the decoded audio plus a Chromaprint fingerprint. Size and mtime are only for cheap change detection. A path is never identity.

Missing files are flagged and never removed automatically. If the same audio shows up elsewhere, offer to relink.

Support MP3, FLAC, WAV, AIFF and M4A (AAC and ALAC) at any sample rate and bit depth. Resampling happens at the engine and analysis boundary. Nothing on disk is converted unless the user asks.

Collections come in three kinds: manual, smart (a saved filter or a saved natural-language search), and read-only mirrors of playlists from other DJ software.

The table is virtualised, with configurable columns, sort and filter on any field, an inline waveform, full keyboard navigation, and preview playback from any row.

### Prep

Prep is one queue for everything that needs a decision: new tracks, identification proposals, quality flags, duplicates, missing files, Organise plans, pending updates to other libraries.

Each item shows what was found, the proposed action, the confidence and the evidence. Enter accepts, S skips, E edits. "Accept everything above 90%" exists and the threshold is adjustable.

Nothing in Prep blocks the rest of the app.

### Identify and tag

Work down this list and stop as soon as the answer is confident:

1. Existing tags, normalised.
2. Filename and folder parsing (`Artist - Title (Mix)`, catalogue numbers, track numbers).
3. Chromaprint fingerprint, looked up through AcoustID to MusicBrainz.
4. Text search with whatever is known: MusicBrainz, Discogs with the user's token, and Bandcamp, Beatport or SoundCloud pages where their terms allow it.
5. Audio confirmation. If a candidate can be compared by fingerprint, compare it. Say "the audio matches" only when it does.
6. Ask the user for a URL (a Discogs release, a Bandcamp page). Only after steps 1 to 5 have failed. Parse it and treat it as a high-confidence source.

Fingerprint databases are thin on vinyl-only and underground releases, so expect step 4 and step 6 to matter for a library full of them.

Providers sit behind one trait. Each declares whether it is free or paid, its rate limit and what it returns. Paid recognisers appear in Settings only when the user adds a key. Every key or token the app needs, such as an AcoustID application key or a Discogs token, is entered by the user in Settings and kept in the OS credential store. A step whose key is missing is skipped and says so. Cache responses and respect every service's terms.

Staging shows current and proposed values side by side for each field, with the source and confidence per field. Candidates are ranked by duration difference, fingerprint score and agreement on label and catalogue number. A value the user typed is never overwritten by an online one.

Tags are written to the file only when the user applies them. Use the format the file expects (ID3 for MP3, AIFF and WAV; Vorbis comments for FLAC; MP4 atoms for M4A). Default to ID3v2.3 because DJ hardware reads it most reliably, and check that claim before relying on it. Preserve frames you don't understand.

Filename templates use tokens such as `{artist} - {title} ({mix})`, `{bpm}`, `{key}`, `{label}`, `{catno}`, `{year}`. Show a live preview on the user's own tracks. Handle collisions and characters the OS forbids. A rename stays in the same folder.

Tagging has to be fast: multi-select editing, find and replace, case fixes, "apply to the whole release", all from the keyboard.

### Quality and replace

Checks: full decode (errors, truncation, duration mismatch), dropouts and abrupt joins, clipping and true peak, loudness (EBU R128), and spectral cutoff as a sign of a lossy source dressed up as lossless. Every flag is either confirmed (a decode error) or likely (a heuristic), and says why.

Duplicates are sorted into byte-identical, audio-identical, same recording in a different encode, and different versions. Radio edits, extended mixes, dubs and remixes are versions. Never call them duplicates.

Replace, step by step:

1. The user picks "Replace file" on a track, or drags a file onto it.
2. A compare sheet shows old against new: format, bitrate, sample rate, bit depth, duration, loudness, spectral cutoff, fingerprint similarity and the measured alignment offset.
3. The app gives a verdict: same recording and better, same recording and not better, or a different version. For a different version the default is "add as a separate track". Replacing is still possible if the user chooses it.
4. Apply runs as one journaled operation. The new file goes next to the old one under the templated name. Tags carry over and merge. Grid, cues and phrases are mapped onto the new file using the measured offset (and tempo ratio, if there is one) and are marked "carried over, check". Collections and sets keep pointing at the same track. Other DJ libraries are updated through their adapters, or the change is queued until the adapter exists. The old file goes to the trash.
5. Undo puts everything back, including the old file if it is still in the trash.

Encodes of the same master differ by a few milliseconds of padding, and a vinyl rip can differ from the digital release in length, speed and start point. That is why step 2 measures alignment and step 3 can refuse.

Two smaller tools live here too. Convert makes a copy in another format from a named preset that states exactly what it does (for example "MP3 320 kbps CBR, true peak held under -1 dBTP"), and checks the result against the source. Repair offers a remux for container-level faults. Both write a new file and leave the original alone until the result has been verified.

### Organise

Off until the user opens it. It never runs by itself.

It builds a plan from a template (`{genre}/{artist}/`, `{label}/{year}/`, by collection, or custom). It suggests templates from what the library actually contains, with numbers: "by genre gives 14 folders of 40 to 400 tracks; 62% of tracks have a label".

The user can add folders of their own to a plan, drag tracks between folders in it, and pin folders and tracks that must stay put.

A plan is a diff: the tree before and after, counts, conflicts, and which other libraries will be updated. Applying it is a journaled operation with undo. A move on the same volume is a rename. Across volumes it is copy, verify by hash, then trash the source.

### Analysis

| Tier | When | What |
| --- | --- | --- |
| 0 | On scan | Tags and file facts |
| 1 | Background, CPU | Decode check, loudness, peaks, three-band waveform, fingerprint, audio hash |
| 2 | Background, GPU if present | Beats, downbeats, tempo and drift, key, energy curve, vocal activity, phrase boundaries, similarity embedding |
| 3 | On demand | Stems |

Every result records the analyser, its version and its parameters. The cache key is the audio hash plus the analyser version, so a rename or retag never triggers re-analysis.

User corrections are stored separately and always win. Re-analysis never deletes a correction.

Every value has a confidence. A low-confidence grid becomes a Prep item ("check grid") so it isn't trusted silently.

The grid editor has a metronome, shift, halve, double, tap, set-downbeat, and a variable grid for vinyl rips that drift.

Say where the analysis is weak (beatless intros, broken rhythms, tempo changes) in the UI.

Stems are large. Cache them compressed, under a size limit the user sets, evicting the least recently used and regenerating on demand. Keep the stem count flexible in the data model, because it depends on the model in use. Before trusting a set of stems, check that they sum back to the original within tolerance.

### Search and discovery

Text search is instant, over all tags (SQLite FTS).

Filters: BPM range, key (Camelot and musical), energy, year, label, format, quality flags, tags, collection.

"Similar to this" uses embedding neighbours, combined with any active filters.

Natural-language search splits a sentence into hard filters ("around 130 BPM"), soft descriptors matched against a text-audio embedding ("rolling", "hypnotic"), and context ("after this track", which becomes transition compatibility). Show how the query was understood as editable chips, so a wrong reading takes one click to fix. It must work with a rule-based parser alone. If Ollama is running, use the local model for better parsing. No cloud calls.

Suggested collections are clusters described by measured traits ("128 to 132 BPM, minor, sparse vocals, 61 tracks"). The user names them. Don't invent genre names.

A similarity map comes later. If it isn't useful for finding tracks, cut it.

### Sets

A set is a timeline with real overlaps. Total length is computed from play windows, never from full track lengths.

For each gap, suggest next tracks with reasons (tempo difference, key relation, energy move, vocal overlap risk, phrase fit) and let the user hear the transition.

A transition preview is rendered offline: both tracks time-aligned at a chosen pair of phrase points, in a chosen style (long blend, bass swap, cut). Search for good phrase points anywhere in the tracks, at 16, 32 and 64 bars. Once stems exist, add stem-aware styles. Score a rendered transition with the same measurements the coach uses. Don't treat a matching key or BPM as proof that two tracks work together, and let the user's A or B choices between previews be stored for later preference learning.

An energy line runs across the set. The target shape is editable.

Generate takes constraints (length, first and last track, tempo range, must-plays, exclusions, artist repeat limit, variety) and returns several alternatives. A seed makes a result reproducible.

A set can be sent to Mix or exported to other DJ libraries.

### Mix

Two decks to start. Build the engine so that four is a configuration change.

Each deck has a scrolling waveform and an overview, play and cue, tempo range, keylock, nudge, optional sync, eight hot cues, loops, beat jump, three-band EQ, filter, trim and channel fader. There is a crossfader, a master, and headphone cue when the audio device has four outputs.

The two waveforms are stacked so beats line up vertically. The mixer sits in the middle. The library browser is below and collapses. A "Next" panel suggests tracks for whatever is playing and updates as the mix moves on.

Every control is an action. Keyboard, mouse, gamepad and MIDI all go through the same action layer, and a mapping is a data file. `?` shows the map for whatever input is in use.

Build the inputs in this order. Keyboard and mouse come first. Gamepads come second: Xbox-style controllers, and the Steam Controller through whichever input layer supports it. Design a default gamepad layout that makes sense in the hand (sticks to nudge and seek, triggers on the channel faders or filter, bumpers for play and cue, d-pad to browse and load) and show it on screen. MIDI with MIDI learn comes third.

The first hardware mapping is the Rane System One, because that is the unit on hand to test with. It works as a controller for Serato and shows up as a MIDI device. Expect the motorised platters and the screens to use vendor messages that have to be captured from the unit. A DDJ-FLX4 mapping follows, since it is the controller most beginners own.

A session can record the master to WAV or FLAC together with a control log: deck positions, tempo, EQ and fader moves, stamped with the audio clock. The control log is what makes Replay exact.

### Coach

A toggle in Mix: Off, Hints, Full. Off costs nothing and shows nothing.

Live measurements (the same code Replay uses offline):

- Beat alignment. Offset between the decks in milliseconds and as a fraction of a beat, plus drift in ms per bar. It comes from grid positions and playheads, cross-checked by correlating low-band onsets over the last two bars. Show it to 1 ms. Accuracy is limited by the grids, so when grid confidence is low, say that and don't show a precise-looking number.
- Starting thresholds, to be tuned by listening: under 5 ms is tight, 5 to 15 ms is audible on kicks, over 15 ms is loose.
- Levels. Master short-term loudness against the session's running level, each deck's pre-fader loudness, headroom and clipping. Catch a dip or jump of more than about 1.5 LU across a transition.
- Low end. Both decks putting energy below about 150 Hz at once.
- Key. The relation between the two keys given tempo and keylock: compatible, tense or clashing.
- Vocals. Vocal activity on both decks during the overlap.
- Phrase. Bars to the next phrase boundary on each deck, and whether the incoming track's start lands on the outgoing track's boundary.

Guidance rules:

- One prompt at a time, two at most. Priority is timing, then level, then low end, then vocals and key, then phrasing.
- A prompt names the problem, the control, the direction and roughly how much: "Deck B is 2.4 LU quieter than A. Raise B's trim about 2 dB before you bring it in." It goes away when the problem does.
- Every prompt has "ignore for this transition". The DJ may be doing it on purpose.
- Hints shows a prompt only when a threshold is crossed. Full keeps the meters visible and adds a short "why" that expands.
- No praise, no streaks, no confetti.
- After each transition there is a small score card (timing, level, low end, phrase) with one takeaway. After the session there is a summary and a trend against earlier sessions.

The coach strip sits between the waveforms and the mixer: a centre-zero beat-offset meter, a level trace with a target band, a low-end overlap bar and phrase countdowns. The accent colour marks only the thing that needs action.

### Replay

Two sources. Sessions recorded in Resonance are exact because of the control log. Imported recordings are estimates.

For an imported recording: find which library tracks appear and where (matching must tolerate tempo change, and is faster and more reliable if the user supplies the set list), align each track against the recording, find the overlaps, then compute the same measurements using the source tracks as references.

Label exact numbers and estimates differently. If a track can't be identified, still score what can be measured from the mix alone: level, spectral balance, clipping.

The result is a timeline with each transition marked and scored. Clicking one shows both waveforms aligned, the measurement traces and up to three specific notes with timestamps. "Practise this transition" loads both tracks at the right points in Mix with the coach on.

A progress view shows scores over time by category.

### Other DJ libraries

Detect rekordbox 6 and 7, Engine DJ, Serato, Traktor, MusicBee and M3U playlists inside the sources.

Each adapter does six things: detect, read, plan, back up, apply, verify. Reading comes first (playlists as mirrors, then cues, grids and keys). Writing comes later (path updates after a rename, replace or move, then exporting grids, cues and playlists).

Rules for writing:

- Never while the target application is running.
- Always back up first.
- A dry run shows the exact changes.
- Verify after writing. If verification fails, restore the backup.

rekordbox's `master.db` is encrypted with SQLCipher and undocumented. Direct writes to it are the riskiest code in the project. Prefer rekordbox XML wherever it can do the job. Engine DJ uses plain SQLite.

All development and testing runs against copies in a sandbox folder. The real libraries are off limits until a specific adapter is approved.
