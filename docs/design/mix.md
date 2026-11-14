# Mix Workspace

The Mix workspace (`ui/src/lib/mix/Mix.svelte`, `Deck.svelte`, `Waveform.svelte` and `renderer.ts`) is the live two-deck performance screen selected from `Sidebar` (`Mix`, shortcut `4`).

It reuses the same `Deck` and `Waveform` components that power `TransitionDock` in the shell ([components.md](components.md)):

- In `Library`, `Prep`, `Sets` and `Replay`, `TransitionDock` shows `Deck variant="dock"` (`340 x 87 px`) and `Waveform mode="crossfade"` (`844 x 101 px`).
- When `Mix` is active, the centre workspace expands into two full `Deck variant="full"` strips and a central mixer column, while `TransitionDock` collapses so the scrolling waveforms are not duplicated.

Issue [#52](https://github.com/BradHudsonGrant/RESONANCE/issues/52) restyles `Mix.svelte` to the component and token system; `M7` (`v0.8.0`), `M8` (`v0.9.0`) and `M12` (`v0.13.0`) add the full mixer controls, live coaching strip, and stem controls.

## Layout and regions (`Mix.svelte`)

```
+-------------------------------------------------------------------------+
| Mix Header: Audio Output Selector | Latency Readout | Record Indicator  |
+-------------------------------------------------------------------------+
| Deck AScrolling Waveform (Waveform mode="deck", WebGL2 1px bars, 120Hz) |
| Deck BScrolling Waveform (Waveform mode="deck", WebGL2 1px bars, 120Hz) |
+-------------------------------------------------------------------------+
| Coach Strip (M8: drift, phrase alignment, level & key guidance)         |
+-----------------------------------+-------------------------------------+
| Deck A (Deck variant="full")      | Deck B (Deck variant="full")        |
| [A] 60px Thumb | Title / Artist   | [B] 60px Thumb | Title / Artist     |
| 128.0 BPM | 8A | -7.4 LUFS | Time | 126.5 BPM | 7A | -7.8 LUFS | Time   |
| Play | Cue | Sync | Keylock       | Play | Cue | Sync | Keylock         |
| Stem LevelDots (M12)              | Stem LevelDots (M12)                |
+-----------------------------------+-------------------------------------+
```

## 1. Mix header bar (`#52`)

- Height: `--control-height-lg` (`42px`), `padding: 0 var(--space-5)` (`20px`), `border-bottom: 1px solid var(--color-border)`.
- Left: Workspace title `Mix` (`--font-display`, `font-weight: 500`, `--text-xl: 20px`, `--color-text`) and audio device selector (`Button variant="select" size="sm"`, `--radius-sm: 6px`).
- Right: Buffer size and callback telemetry (`DataTag variant="rect"`, `--font-mono`, `--text-xs: 11px`, `tabular-nums`, e.g. `48.0 kHz · 480 frames · 0.34 ms max`), plus the session record indicator (`M7`, using `--color-accent` when recording is armed/active).

## 2. Stacked scrolling waveforms (`Waveform.svelte` `mode="deck"`, `#52`)

- **Container:** `Card variant="surface" radius="lg"` (`--radius-lg: 10px`, `1px solid var(--color-border)`, `--color-background` canvas bed) containing two stacked `110px`-tall WebGL2 waveform lanes separated by a `1px` `--color-border` centre line.
- **1 px bar rendering (`ui/src/lib/mix/renderer.ts`):**
  - Draws `1px` CSS vertical peak bars (`Math.max(1, Math.round(devicePixelRatio))` device pixels wide, `1px` gap) locked to the audio clock (`ui/src/lib/mix/clock.ts`).
  - Unplayed bars ahead of the playhead use `--color-text-2` (`#9A9892` dark / `#595853` light); played bars behind the playhead use `--color-text` (`#F2F1EF` dark / `#141413` light).
  - Beat-grid ticks render as `1px` vertical hairlines (`--color-border` on beats, `--color-border-control` on 4-bar downbeats).
  - Playhead: stationary `1px` vertical line in `--color-accent` (`#E4553D` dark / `#D94F3B` light) topped with a `5 x 5 px` square cap (`--color-accent`), matching the playhead in `TransitionDock`.
- **Performance budget:** Zero DOM writes per frame. Meter and playhead data arrive over the binary channel (`ui/src/lib/mix/frames.ts`) at `<= 60 Hz` and are extrapolated at the display refresh rate (`<= 8 ms` 99th-percentile frame time at `120 Hz`, [#46](https://github.com/BradHudsonGrant/RESONANCE/issues/46)). While `Mix` is playing, background `Wave.svelte` canvases pause automatically.

## 3. Deck strips (`Deck.svelte` `variant="full"`, `#52`, `M7`, `M12`)

Each deck panel is built from `Deck.svelte` (`variant="full"`) inside a `Card variant="surface" radius="lg"` (`--radius-lg: 10px`) wrapped in `FocusBrackets.svelte` (highlighting the keyboard-targeted deck):

- **Header row (shared structure with `Deck variant="dock"`):**
  - Deck badge (`A` or `B`, `35 x 34 px`, `--radius-sm: 6px`, `--color-surface-2`, `1px solid var(--color-border)`, `--font-display` 600 `--text-lg: 16px`).
  - Artwork `Thumb size="md"` (`60 x 60 px`, `--radius-md: 8px`).
  - Track title (`--font-display`, `font-weight: 600`, `--text-xl: 20px`, `--color-text`) and artist (`--font-sans`, `--text-base: 14px`, `--color-text-2`).
- **Technical data tag strip (`DataTag variant="pill"`, Board 2 `[05]` and `[07]`):**
  - Four `22px`-tall `DataTag variant="pill"` badges (`--font-mono`, `--text-xs: 11px`, `tabular-nums`):
    - Tempo: `128.0 BPM` (`+0.0%`)
    - Key: `8A · A min`
    - Loudness: `-7.4 LUFS` (`M5`)
    - Remaining / elapsed time: `-02:47 / 07:02`
- **Transport and pitch row (`#52` / `M7`):**
  - `Play / Pause` (`Button variant="primary" size="md"` when playing, `variant="default"` when paused, `34px` height, `--radius-sm: 6px`), `Cue`, `Sync` and `Keylock` buttons (`Button variant="default" size="md"`).
  - Overview scrub bar (`ScrubBar size="md"`, `4px` track, `12 x 12 px` thumb).
- **Channel mixer and stem controls (`M7` and `M12`):**
  - 3-band EQ (`HI`, `MID`, `LOW`), channel filter, crossfader assignment, and twin vertical level meters (`2px` segments in `--color-text-2` / `--color-text`, with only the `0 dBFS` clip indicator in `--color-accent`) arrive in `M7`.
  - Four `LevelDots.svelte` indicators (`DRUMS`, `BASS`, `VOCALS`, `OTHER`, four `4px` dots each from Board 2 `[07]`) and stem mute toggles arrive in `M12`.
