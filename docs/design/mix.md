# Mix

The Mix space is the live two-deck screen: `Mix.svelte`, `Waveform.svelte` and `renderer.ts` in `ui/src/lib/mix/`, with `Deck.svelte` from [#51](https://github.com/BradHudsonGrant/RESONANCE/issues/51). [#52](https://github.com/BradHudsonGrant/RESONANCE/issues/52) restyles the M0 spike to the tokens and components. M7 adds the full decks and mixer, M8 the coach and M12 the stems.

It uses the same `Deck` and `Waveform` as the dock. In the other spaces the dock shows the two decks as cards with the crossfade between them. In Mix the workspace holds the full decks and the dock folds away, so the scrolling waveforms aren't shown twice.

## Layout

```
+-----------------------------------------------------------------------+
| Mix   audio device                      buffer and timing   recording |
+-----------------------------------------------------------------------+
| deck A scrolling waveform                                             |
| deck B scrolling waveform                                             |
+-----------------------------------------------------------------------+
| coach strip (M8)                                                      |
+-----------------------------------+-----------------------------------+
| deck A                            | deck B                            |
| letter, artwork, title, artist    | letter, artwork, title, artist    |
| BPM, key, loudness, time          | BPM, key, loudness, time          |
| play, cue, sync, keylock          | play, cue, sync, keylock          |
| stems (M12)                       | stems (M12)                       |
+-----------------------------------+-----------------------------------+
```

## Header

42 px tall, 20 px in from the sides, with a 1 px border under it. On the left, "Mix" in 20 px Space Grotesk 500 and the audio device as a select. On the right, the buffer and callback figures as a mono tag, such as `48.0 kHz · 480 frames · 0.34 ms max`, and the recording indicator, which turns to the accent while recording (M7).

## Waveforms

Two lanes, each 110 px tall, stacked in a card with 10 px corners on Background and split by a 1 px line. They follow the waveform rules in [visual-language.md](visual-language.md#waveforms) and are locked to the audio clock (`clock.ts`).

- Played bars are in Text and bars still to play in Text 2.
- Beats are 1 px lines in Border, and every fourth bar's downbeat in control border.
- The playhead stands still: a 1 px line in the accent with a 5 px square cap, as in the dock.

Nothing in the page changes per frame. Playhead and meter data arrive over the binary channel (`frames.ts`) up to 60 times a second and are extrapolated to the display rate. The budget is a 99th-percentile frame time of 8 ms or less at 120 Hz ([#46](https://github.com/BradHudsonGrant/RESONANCE/issues/46)). The dot wave pauses while Mix is playing.

## Decks

Each deck is `Deck variant="full"` in a card with 10 px corners, framed by focus brackets when it has the keyboard.

- The header matches the dock card: a 35 by 34 letter badge, 60 px artwork, the title in 20 px Space Grotesk 600 and the artist in 14 px Text 2.
- Four 22 px mono pills: tempo with its pitch change (`128.0 BPM`, `+0.0%`), key (`8A · A min`), loudness (`-7.4 LUFS`, from M5) and time left over total time.
- Play, Cue, Sync and Keylock buttons, 34 px tall. Play is inverted while playing. Under them, an overview scrub bar.
- From M7, a three-band EQ, a filter, crossfader assignment and two level meters made of 2 px segments in Text 2 and Text, with only the clip light in the accent.
- From M12, four stem rows (Drums, Bass, Vocals, Other) as level dots with mute buttons.
