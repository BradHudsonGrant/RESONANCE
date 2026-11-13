# Structure, layout and interaction

## Structure

Five spaces in a left rail: Library, Inbox, Sets, Mix, Review. Activity and Settings sit at the bottom of the rail.

Each space is a job with a beginning and an end. Analysis, history and "intelligence" are properties of tracks and sessions, so they are shown where the track or session is shown and have no page of their own.

| Job | Where it lives |
| --- | --- |
| Add folders, choose subfolders | Settings > Sources, and the Library sidebar |
| Browse, search, filter, collections | Library |
| New tracks, identify, fix tags, quality flags, duplicates, missing files | Inbox (the same tools are also on any track in Library) |
| Track details, grid and cue editing, replace | Inspector, expandable to a full track view |
| Similar tracks, what mixes next | Inspector > Matches, and the Next panel in Mix |
| Build and generate sets | Sets |
| Decks, coach, recording | Mix |
| Recorded mixes, scores, progress | Review |
| Background work and undo history | Activity |
| Organise folders | Library > Organise (a tool that opens a plan) |
| Other DJ libraries, controllers, models, audio device | Settings |

Always present: the command bar (Ctrl+K: search, commands, natural language), the inspector on the right (I toggles it), the player bar at the bottom, and the Activity indicator.

## Layout

```
+------+-----------------------------------------------+---------------+
| rail | command bar                                   |               |
|      +-----------------------------------------------+   inspector   |
| Lib  |                                               |   (track,     |
| Inbx |   workspace                                   |   transition  |
| Sets |   table / set timeline / decks / review       |   or set item)|
| Mix  |                                               |               |
| Rev  +-----------------------------------------------+               |
|      |   dock: preview deck, expands to A > B        |               |
| Act  +-----------------------------------------------+---------------+
| Set  |   player bar                                                  |
+------+---------------------------------------------------------------+
```

This sketch is a guide. The real prototype is milestone 0, in the real stack.

Selecting one track fills the inspector. Selecting two offers the A > B preview in the dock. Selecting many shows batch actions.

## Interaction rules

- Everything is reachable from the keyboard. Shortcuts appear in menus and tooltips.
- No chains of modals. Anything that changes files uses one confirm sheet that shows the diff.
- Ctrl+Z works for every journaled operation. Activity lists past operations with an undo on each.
- Background work never blocks. A job that needs the user becomes an Inbox item.
- Every view has designed empty, loading and error states. An error says what happened and what to do next.
- Dense by default: 28 to 32 px rows and tabular numerals. Don't swap tables for decorative cards.
- Honour the OS settings for reduced motion and high contrast.

## Showing where a value came from

- Measured: plain.
- Estimated by an algorithm or model: the value with a small hollow dot. Hover shows the source, version and confidence.
- Confirmed by the user: a small tick.
- Low confidence: a muted value, plus an Inbox item.

One component does this. Every data field uses it.

## Names

Use plain words: Library, Inbox, Sets, Mix, Review, Activity, Settings, Collections, Coach, Replace, Organise, Identify.

Don't bring back the names from the old chats: Job Orchestrator, Disk Steward, Sonic Constellations, Anchor, Catalyst, Intelligence.

Interface text uses British spelling, as v3 did. Code uses whatever the language ecosystem expects.
