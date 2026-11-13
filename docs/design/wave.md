# The wave

[wave-reference.html](wave-reference.html) is a working reference and a tuning tool. The component is in `ui/src/lib/wave/`, with the same maths. Open the reference in a browser, play the test signal or a real track, move the sliders, and copy the settings box. Saved values become the component's defaults. Until some are saved, the defaults are the Balanced preset.

How it moves:

- Idle, it drifts slowly. On the Still preset it is a static image.
- With music, the flow follows the tempo: one full cycle every 32 bars by default, with a small surge on each beat.
- The shape follows the tone. The ridge slides toward where the spectral energy sits and widens as the spectrum spreads, smoothed over more than a second.
- Bass lifts the swell, mids stir the cross-waves, highs raise the fine ripple. Each has its own amount.
- A beat sends one ring outward from the crest.
- In a breakdown, when the low end drops out and the music carries on, the surface settles. It opens up again when the kick returns.
- Every input goes through attack and release smoothing, so nothing snaps.

The reference estimates tempo and beats from the audio because it has nothing else. The app knows better: take tempo, beat phase and phrase boundaries from the grid and the analysis, band levels from the engine's metering tap, and breakdowns from the energy curve. Don't use a Web Audio analyser.

It draws the surface as points in one draw call, displaces them in the vertex shader, allocates nothing per frame, pauses when hidden and has a frame cap. Port it into a component. Keep the maths unless you improve it and can show the difference side by side.

Settings > Appearance offers the presets (Still, Calm, Balanced, Lively), idle drift and reactivity, with the remaining controls under Advanced. With reduced motion set in the OS it starts on Still.

Use it in empty states, the now-playing header, collection headers, and as artwork when a track has none (a still frame seeded from that track's own band energy, so each track gets a consistent tile). Never behind a table. The default preset is Balanced.

Budget: under 1 ms of UI-thread time per frame at 60 fps. Drop to 30 fps or pause while GPU analysis is running. Measure it and record the numbers. The numbers are in [benchmarks/wave.md](../benchmarks/wave.md).
