# 0010: Audio engine and the Mix view's real-time channel

Date: 13 November 2026. Status: accepted for the spike; the audio buffer size is open in [#47](https://github.com/BradHudsonGrant/RESONANCE/issues/47).

## Context

The Mix view is the project's performance risk. Two decks have to play real files through the audio device while their waveforms scroll in step with what is heard. The audio callback may not allocate, lock, log or do I/O, and nothing real-time may go through JSON.

## Decision

- A new crate, `crates/engine`, holds the decks, the mixer and the device. `crates/core` holds the rules it uses: meter ballistics (`core::meter`) and the waveform summary (`core::waveform`, 300 bins a second, peak and RMS as a byte each).
- Output is `cpal` on WASAPI in shared mode, at the device's own sample rate and channel count. The stream asks for a 256-frame buffer. cpal's `realtime` feature puts the callback thread in the Windows multimedia scheduler.
- Each deck has a decoder thread. Symphonia decodes the file, the thread resamples it to the device rate by linear interpolation and pushes stereo frames into an `rtrb` ring buffer holding two seconds. A second thread decodes the whole file once for the waveform.
- Commands reach the callback through an `rtrb` queue. A deck's old feed goes back through a second queue of the same size and is freed on the control side, so the callback never frees memory.
- The callback publishes through atomics: the frames each deck has played and the device latency as a sequence lock, the loudest sample per channel with `fetch_max`, and counters for callbacks, underruns, late callbacks, starved callbacks and its own longest run.
- The audio clock is the only clock. A deck's position is the frames it has played, plus the time since that buffer's first frame is due at the device. The page moves the playhead on with its own clock between readings and eases out small differences.
- `resonance serve` reads the decks 60 times a second on one thread, while a page is connected and the device is open. Each connection gets the same 120-byte telemetry frame and, once per load, the deck's waveform, as binary WebSocket frames. The layout is documented in `crates/api/src/mix_frame.rs` and read in `ui/src/lib/mix/frames.ts`. Commands (`mix_load`, `mix_play`, `mix_status`) stay JSON.
- The page draws both lanes and the meters in one WebGL2 canvas. A fragment shader picks the bins under each one-pixel column, so the bars have hard edges and move in whole pixels. Meters are scissored clears. Nothing is allocated per frame, and the loop stops drawing while hidden or when nothing plays.

## Why

cpal is the default named in the architecture and is the only maintained Rust crate that covers WASAPI, ASIO, CoreAudio and ALSA behind one API. Writing WASAPI directly would need `unsafe`, which the workspace denies.

`rtrb` is a single-producer, single-consumer ring buffer that does not allocate or block after it is made, which is the shape both queues need. A sequence lock lets the callback publish several values together without waiting; a reader that lands on a write reads again.

Linear resampling is enough for a spike on 44.1 and 48 kHz material and costs almost nothing. The architecture already names Signalsmith Stretch for keylock and plain resampling otherwise; a better resampler comes with the decks in M6.

## What this does not reach

cpal's WASAPI host opens shared-mode streams only. Its callback runs at the device period whatever buffer is asked for, and it does not use the low-latency shared mode of `IAudioClient3`. On the development laptop that is 480 frames at 48 kHz (10 ms), inside a 1,056-frame device buffer. So the spike measures stability at the device period, not at 256 frames.

Two routes reach 256 frames: ASIO, or WASAPI exclusive mode. The laptop has Realtek ASIO and DJM-450 ASIO drivers. cpal's `asio` feature builds against Steinberg's ASIO SDK and needs LLVM and Clang installed. Since October 2025 the SDK is offered under GPLv3 or under Steinberg's proprietary agreement, which is free but has to be signed. GPLv3 is ruled out by [0005](0005-licence-policy.md), so ASIO means signing that agreement. Exclusive mode needs cpal support or a small amount of Windows code outside the workspace's safe-code rule. Both are tracked in the issue above.

## Dependencies

Checked on 13 November 2026:

- `cpal` 0.18.2, Apache-2.0. Released 16 August 2026; the repository had commits on 2 and 11 November 2026.
- `rtrb` 0.4.1, MIT or Apache-2.0. Released 25 October 2026; last commit 1 November 2026.
- `audio_thread_priority` 0.35.1, MPL-2.0, through cpal's `realtime` feature. Maintained by Mozilla; last commit 17 September 2026.

## What follows from it

The installed app has no binary channel yet, so its Mix view says it needs the development core. The native transport gets one when the decks are built in M6.

The waveform is decoded twice per load, once for playback and once for the summary. Analysis caches waveforms in M4.

The measurements are in [benchmarks/mix-view.md](../benchmarks/mix-view.md).
