# Mix and later spaces

Issue [#52](https://github.com/BradHudsonGrant/RESONANCE/issues/52) rebuilds the Mix view (`ui/src/lib/mix/Mix.svelte`) to match the tokens, rounded deck pods and micro elements of both concept boards using today's two-deck audio spike. Elements from the boards that belong to `Sets`, `Coach` and `Replay` wait for their milestones.

## Mix view in M1 (`#52`)

The view fills the shell's workspace area as a CSS grid (`grid-template-rows: auto auto 1fr auto`, `height: 100%`, `--color-background`).

### Deck A and Deck B header pods

A two-column bar (`grid-template-columns: 1fr 1fr`, `gap: 16px`, `padding: 12px 16px`, `--color-surface`, `1px solid var(--color-border)` bottom border) holds one framed pod for each deck, matching the Deck A / B card anatomy at the bottom of `concept-dark.png`:

- **Pod container:** `--color-surface-2` (`#1E1E1F` dark) or `--color-background` (`#F8F7F5` light), `1px solid var(--color-border)`, `--radius-panel` (`8px`), `padding: 10px 12px`, `display: flex`, `align-items: center`, `gap: 12px`.
- **Deck badge (`A` / `B`):** `28 x 28 px` square (`--radius-control: 6px`, `--color-raised`, `1px solid var(--color-border)`, `Space Grotesk` `14px` weight `600`, `--color-text`).
- **Deck thumbnail (`40 x 40 px` `WaveThumb`):** `--radius-control` (`6px`), `1px solid var(--color-border)`, seeded from the loaded track's facts (or showing the Grid Dots motif when empty).
- **Track selector and facts:** `<select>` (`height: 28px`, `--radius-control: 6px`, `--color-surface`, `1px solid var(--color-border-control)`, `Inter` `13px` `--color-text`) above a mono subline (`BPM · Key`, `JetBrains Mono` `11px` `--color-text-2`, `tabular-nums`) when a track is loaded.
- **Transport and clock:** Play/Stop `Button` (`28px` height, `--radius-control: 6px`; active playing deck takes a `1px solid var(--color-accent)` left or border highlight without moving layout) and tabular time readout (`0:00 / 0:00`, `JetBrains Mono` `12px` `--color-text`).

### Stacked waveform viewport (`#52`)

- **Canvas:** WebGL2 `<canvas>` filling `1fr` on `--color-background` (`#0B0B0C` dark / `#F8F7F5` light), drawing Deck A (top) and Deck B (bottom) with 1 px whole-pixel bars (`--color-text-2` peak, `--color-text` RMS, no anti-aliasing, no gradient fills), playheads in `--color-accent`, and stereo meters on the right (`--color-surface-2` track, `--color-text-2` level, `--color-text` hot).
- **Focus brackets:** Passive `6 x 6 px` corner brackets (`--color-border-control`, `1.5px` stroke) frame the canvas viewport as a CSS overlay outside the per-frame render loop, with an `--color-accent` playhead tick at the alignment line.
- **Budget:** Re-measured in `#52` against [#46](https://github.com/BradHudsonGrant/RESONANCE/issues/46) (99th-percentile frame under 8 ms at 120 Hz) with zero DOM updates per frame.

### Status bar and states (`#52`)

- **Footer (`32px` height, `padding: 0 16px`, `--color-surface`, `1px solid var(--color-border)` top border):** `JetBrains Mono` `12px` `--color-text-2` displaying the active device (`device.name`, sample rate in kHz, period frames and requested buffer frames), or the prompt `"Choose a track for a deck. The audio device opens with the first one."`.
- **Empty / notice / error states:**
  - When opened without the binary WebSocket channel (for example in the packaged app before M7 adds native binary transport), a flat bordered notice above the canvas states `"The Mix view runs against the development core for now. Start it with just serve and open the page from just ui."`
  - When WebGL2 is absent or a command fails, a `role="alert"` banner names what failed in `--color-text` inside `--color-surface-2` with a `1px solid var(--color-accent)` border.

## Later deck, coach, replay and set views

Both concept boards contain components designed for later milestones. None appear in `#52` with made-up values; each waits for the milestone that brings its data.

| Board element | Design and tokens | Where it goes in Resonance | Milestone |
| --- | --- | --- | --- |
| Full deck controls, 3-band EQ, filter, faders, cues, loops | Dense `--color-surface` mixer strips, `28px` controls (`--radius-control: 6px`), tabular mono readouts | `Mix` view central mixer and deck strips | **M7** Decks |
| `Next in Mix` track rows (`[08]`/`[09]`) | `36 x 36 px` `WaveThumb`, title/artist, 1 px inline mini waveform, mono duration, load/preview buttons on `--color-surface` | Collapsible `Next` panel in `Mix`, plus `Matches` in `Inspector` | **M6** Discovery / **M7** Decks |
| Coach strip & bracketed score cards (`[07]` `Taste Profile`) | `--color-surface` strip between waveforms and mixer; bracketed `StatCard` (`--radius-panel: 8px`, `--color-accent` corner brackets, 1 px metric bars, `Space Grotesk` `20px` score) | `Mix` coach strip (Off / Hints / Full) and post-transition score cards | **M8** Coach |
| `Listening Insight` card (`[08]`/`[09]`) | `--color-surface` panel (`--radius-panel: 8px`, `1px solid var(--color-border)`), bar glyph, `Inter` `13px` takeaway note | `Replay` transition notes (up to 3 timestamped notes per transition) and session summary | **M9** Review |
| Hero dot-wave header, 5-column mono strip (`[08]`/`[09]`) & `WaveCard` grid (`concept-dark.png`, `[07]`) | `Wave` canvas behind `Space Grotesk` `24px` title, `--color-accent` primary `Generate` button, `default` `Refine` button, 5-column mono stats (`SET INDEX`, `TRACKS`, `DURATION`, `BPM RANGE`, `ENERGY` 5-dot meter), and `--radius-lg` (`10px`) `WaveCard` tiles | `Sets` workspace (saved and generated sets above the real-overlap timeline) | **M10** Sets and transitions |
| Index marks (`[05]`) & A to B crossfade waveform (`concept-dark.png`) | `001 \| 002 \| 003` bar/phrase ruler and aligned dual-waveform crossfade preview in the bottom dock | `Sets` transition previewer and `Replay` aligned mix timeline | **M9** Review / **M10** Sets |
