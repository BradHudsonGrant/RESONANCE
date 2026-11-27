# Analysis

There is no budget for analysis yet. These are the numbers to compare against as the analysers land.

## 26 November 2026, spectral front end ([#186](https://github.com/BradHudsonGrant/RESONANCE/issues/186))

Machine: ROG Zephyrus G16, Core Ultra 9 285H, Windows 11 Pro. Release build, one thread.

Method: six minutes of stereo test signal (a 220 Hz tone with a click every half second) pushed through `FrontEnd` in 8,192-sample pieces: mono mix, resampling to 44.1 kHz where needed, the 2,048-sample STFT and the median-filter split.

| Source rate | Frames  | Time  |
| ----------- | ------- | ----- |
| 44.1 kHz    | 31,008  | 6.7 s |
| 48 kHz      | 31,008  | 7.5 s |

About 50 times faster than real time on one core. Almost all of it is the two medians, 63 million of them for six minutes of audio.

Test signals separate cleanly: a 1 kHz sine is 0.9997 harmonic and a click train every half second is 1.0000 percussive.
