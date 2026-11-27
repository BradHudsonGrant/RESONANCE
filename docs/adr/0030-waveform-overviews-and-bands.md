# 0030: Waveform overviews and bands

Date: 26 November 2026. Status: accepted.

## Context

Table rows, the dock and the inspector draw a small waveform for each track, and the Mix lanes draw a loaded track at full detail. A plain peak waveform doesn't show where the kick drops out or where the hats come in, and the design rules allow brightness but no colour. Every file is already decoded once by the identity pass, which hashes it, fingerprints it and checks its quality. Waveform data has to reach the page as binary, never inside the JSON of a track page.

## Decision

- Every waveform bin holds four bytes: the peak of all channels, then the RMS level of the mono mix below 150 Hz, from 150 Hz to 2.5 kHz and above 2.5 kHz, split with fourth-order Linkwitz-Riley filters. The summariser lives in `core`, so the identity pass and the deck loader share it.
- Bars are drawn in layers of one tone: the peak faint, then the high, mid and low bands, each brighter than the last.
- The identity pass draws a 140-column overview as it decodes and keeps it in `waveform_overview`, keyed by audio hash. A moved, renamed or retagged file keeps it, and files identified before this change are read once more to draw theirs. Each column keeps the loudest of the bins it covers, so a short hit still shows.
- Deck waveforms are not stored. At 300 bins a second a six-minute track is 430 KB, which across 100,000 tracks is tens of gigabytes. The deck loader draws them on its own thread while the track loads, as before.
- The page reads the overviews of the rows on screen in one binary body: `GET /waveforms?tracks=<id>,...` on the local socket, behind the same token as the WebSocket, and the `waveforms` scheme in the desktop app. At most 500 tracks a request. One read is in flight at a time and only the newest view waits behind it.
- The spectral front end for the analysers (44.1 kHz mono, a 2,048-sample STFT and the median-filter harmonic and percussive split) streams frames to its caller and never holds a whole spectrogram. It runs when an analyser needs it, not in the identity pass, until an analyser stores what it reads.

## What would change it

An analyser that reads the front end in the identity pass, a zoomable overview that needs more than 140 columns, or a table that draws the column fast enough only from one shared canvas ([#187](https://github.com/BradHudsonGrant/RESONANCE/issues/187)).
