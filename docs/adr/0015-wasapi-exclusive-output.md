# 0015: WASAPI opened directly for exclusive and low-latency output

Date: 18 November 2026. Status: accepted. Settles the buffer size left open by [0010](0010-audio-engine-spike.md).

## Context

The audio budget in [architecture.md](../architecture.md#performance-budgets) asks for stable playback at a 256-sample buffer or smaller, on WASAPI exclusive or ASIO. cpal 0.18.2, the latest release, opens WASAPI streams in shared mode only. Its callback runs at the mixer's period whatever buffer is asked for, which is 480 frames (10 ms) at 48 kHz on the development laptop.

Resonance has to work out of the box on Windows, macOS and Linux, for people who are new to DJing as well as those who are not. ASIO means driver installs and a signed agreement with Steinberg, so it cannot be the default anywhere.

## Decision

- cpal stays the output on every platform, and the fallback on Windows.
- On Windows, the engine opens WASAPI itself through the `wasapi` crate, in one of two modes, `OutputMode::Shared` or `OutputMode::Exclusive`.
- Shared is the default. It asks the system mixer for its shortest period through `IAudioClient3`, at or under 256 frames where the driver allows that. Other programs keep playing.
- Exclusive takes the device for Resonance alone, at a 256-frame period, in the first sample format the device accepts: 32-bit float, then 24-bit in 32, packed 24-bit and 16-bit. If the device refuses, the stream opens shared instead, and `Device::mode` says which it got.
- Each exclusive open plays half a second of silence first, before any deck can play, and counts late callbacks. More than one in ten means the device came up asking only every other period, which is heard as constant crackle. The engine then closes it, tries once more, and opens shared if the second try is no better.
- If neither opens through WASAPI, the stream goes through cpal as before.
- The render loop runs on the engine's output thread, promoted once to the multimedia scheduler through `audio_thread_priority`. Its buffers are made before it starts, so it never allocates, locks, logs or does I/O.
- An exclusive device is always asked for exactly one period, so the callback's frame count cannot show lateness. The loop judges it by the time between callbacks instead: more than one and a half periods is late, and more than two has let the device run dry.
- `resonance audio` plays two tracks for as long as needed, in either mode, and prints the counters. `resonance bench` asks for exclusive, since that is what the budget names.

## Why

`wasapi` (HEnquist/wasapi-rs) wraps the WASAPI calls cpal leaves out, and its `unsafe` stays inside it, so the workspace's safe-code rule holds. It is the crate CamillaDSP is built on, and it uses the same `windows` 0.62 crate as cpal, so it adds no second copy of the Windows bindings.

Exclusive is opt-in because it silences every other program while Resonance plays, and fails while something else holds the device. A newcomer should never find their other apps gone quiet without having asked for it.

## What this does not reach

The development laptop's USB speakers allow 480 frames and nothing shorter in shared mode, so on this machine only exclusive mode meets the budget. Drivers that allow a shorter shared period get it without any setting.

Exclusive mode held 256 frames for 20 minutes with no dropouts, but two opens came up asking every other period, and why is not known. The settling check covers that case; the clean hour stays open in [#47](https://github.com/BradHudsonGrant/RESONANCE/issues/47).

Settings has no switch for the mode yet, so the app opens shared until [#104](https://github.com/BradHudsonGrant/RESONANCE/issues/104) adds one.

macOS and Linux keep cpal, which asks CoreAudio and ALSA for a 256-frame buffer directly. Neither has been measured yet.

## Dependencies

Checked on 18 November 2026:

- `wasapi` 0.27.0, MIT. The repository's last push was on 16 November 2026, and it is not archived.
- `audio_thread_priority` 0.35.1, MPL-2.0, already in the tree through cpal's `realtime` feature and listed in [0010](0010-audio-engine-spike.md). It is now a direct dependency on Windows.

## What follows from it

The measurements are in [benchmarks/mix-view.md](../benchmarks/mix-view.md).
