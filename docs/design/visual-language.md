# Visual language

## Colour, type and spacing

| Token | Dark | Light |
| --- | --- | --- |
| Background | `#0B0B0C` | `#F8F7F5` |
| Surface | `#141415` | `#FFFFFF` |
| Surface 2 | `#1E1E1F` | `#ECEAE7` |
| Raised | `#252527` | `#FFFFFF` |
| Border | `#343437` | `#D8D4CF` |
| Text | `#F2F1EF` | `#171717` |
| Text 2 | `#A9A8A6` | `#666461` |
| Muted | `#777777` | `#8A8783` |
| Accent | `#E4553D` | `#D94F3B` |

Check contrast for every text and background pair and adjust the muted tones if any fail WCAG AA.

Type: Space Grotesk for headings and the wordmark, Inter for the interface, JetBrains Mono for BPM, time, key and other numbers. All three are bundled with the app. Nothing loads from a CDN.

Spacing on a 4 px scale. Corner radius 4 px on controls and 6 px on panels, no larger. Borders, not shadows. No blur, no gradients, no glass.

The accent is for the playhead, the active selection, the one thing that needs action, and one primary action per view.

One outline icon set with a 1.5 px stroke.

Motion lasts 120 to 180 ms, eases out, and exists only to explain a change of state.

## Waveforms

Bars are one device pixel wide, on whole pixel positions, with no anti-aliasing. Two tones show peak and RMS. Band energy is shown as brightness. No rainbow colouring. The accent is only for the playhead and cue markers.

Draw them in WebGL2 from precomputed multi-resolution peak data, passed as binary. The overview and the scrolling view share one renderer.

## Logo

The mark is seven bars tracing a resonance peak, with the centre bar in the accent colour. The mark's SVG uses `currentColor` for the other six, and the app icon is a separate SVG. Both join the repo with the design tokens in M0. The wordmark is RESONANCE in Space Grotesk Medium, uppercase, tracked out to about 0.32 em. Treat these as a starting point that may change.
