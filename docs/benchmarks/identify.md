# Identification hit rates

On the 50 local fixtures, identification matched 35 tracks and was confident about 31, at a confidence of 0.92 or more. The audio confirmed 26 of the matches, by fingerprint or by a store preview. Most confident results came from the AcoustID fingerprint (22), then the name searches (8) and the preview check (1).

## Method

On 25 November 2026, `resonance scan` read the 50 files in `fixtures-local/` (20 MP3, 10 FLAC, 8 AIFF, 7 WAV and 5 M4A) into an empty library, and `resonance identify --online` looked every track up with an empty provider cache. AcoustID and Discogs keys were configured, AudD was not, and no release links were supplied. The build was `master` at `bf6c89d`, in release mode.

The counts come from the step reports stored for each track in `track_identification.steps_json`. A step that read the file counts as a hit when it read at least one field. A provider counts as a hit when it returned at least one candidate. A step skipped because an earlier one was already confident, or for missing input, stays out of the "ran for" column.

## Results

| Step                                   |       Ran for | Found something | First made the track confident |
| -------------------------------------- | ------------: | --------------: | -----------------------------: |
| 1. The file's own tags                 |            50 |              48 |                              0 |
| 2. The filename and folders            |            50 |              50 |                              0 |
| 3. AcoustID fingerprint                |            49 |              23 |                             22 |
| 4. Name searches                       |            25 |              17 |                              8 |
| 5. Check by ear against store previews |             8 |               4 |                              1 |
| 6. A pasted release link               | none supplied |                 |                                |

| Name search                           | Asked for | Found something |
| ------------------------------------- | --------: | --------------: |
| Discogs                               |        25 |              14 |
| Deezer                                |        24 |              10 |
| iTunes                                |        17 |               3 |
| MusicBrainz                           |        24 |               2 |
| Bandcamp pages guessed from the names |        16 |               0 |

| Outcome                       |   Tracks |
| ----------------------------- | -------: |
| Matched                       | 35 of 50 |
| Confident                     | 31 of 50 |
| Confirmed by audio            | 26 of 50 |
| Left to a pasted release link | 19 of 50 |

The run took 289 s, with 396 requests and 27 answers from the cache built during the run.

## Conclusion

The fingerprint does most of the work on commercially released tracks. The name searches pick up a third of what it misses, mostly on Discogs and Deezer. None of the fixtures is a Bandcamp-only release, so the guessed Bandcamp pages found nothing here. They were measured by hand on a separate sandbox of club edits during [#150](https://github.com/BradHudsonGrant/RESONANCE/pull/150), where pasted and learned sellers found the tracks the other providers missed.

The same fixtures with batched AcoustID lookups found no fingerprint matches at all, which is how [#151](https://github.com/BradHudsonGrant/RESONANCE/issues/151) was found and fixed in [#152](https://github.com/BradHudsonGrant/RESONANCE/pull/152).
