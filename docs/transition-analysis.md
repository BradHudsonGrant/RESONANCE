# Bar-synchronous analysis and transition scoring

Design notes for the analysis, transition-scoring, coaching and set-generation work in M2, M4, M5, M6, M7, M8 and M10. See [architecture.md](architecture.md), [product.md](product.md), [v3-notes.md](v3-notes.md), [ADR 0002](adr/0002-model-runtime.md), [ADR 0003](adr/0003-storage.md) and [ADR 0004](adr/0004-identity-model.md).

## Why track-level scalars fall short

Comparing one track's overall BPM, Camelot code and single embedding against another's misses how a DJ actually mixes. Two tracks never overlap from start to finish. An 8, 16, 32 or 64-bar exit window of track A overlaps an entry window of track B under a chosen mixer move: a 150 Hz bass swap on the downbeat, a long progressive blend, a buildup-to-drop cut, or a beatless breakdown transition.

General-purpose audio embeddings also measure the wrong thing for transitions. Contrastive encoders such as CLAP and MERT put tracks with the same density and instrumentation near each other. If track A has upfront vocals and dense midrange chords, its nearest neighbours in CLAP or MERT space have upfront vocals and dense midrange chords too, so playing them together clashes. In Delabaere et al.'s mashup evaluation against COCOLA perceptual accompaniment scores, cosine similarity correlated at `r = 0.053` for CLAP and `r = -0.010` for MERT. Vibe similarity is symmetric; mix compatibility is asymmetric and complementary.

Resonance has to model that bar-level interaction inside its standing limits: rules in `core`, shared live and offline measurements in `metrics`, permissive licences enforced by `cargo-deny`, no Python in the shipped app, search under 50 ms, and idle memory under 400 MB at 100,000 tracks.

## Risks in the starting plan

Five traps in the initial milestone notes need addressing as M2 to M6 are built:

| Area | Trap | Fix |
| --- | --- | --- |
| Phrases (M5) | `allin1` (`mir-aidj/all-in-one`) takes four-stem HTDemucs spectrograms (`[4, steps, 24]`), relies on custom NATTEN C++/CUDA kernels that resist plain ONNX export, and outputs pop labels (`verse`, `chorus`, `bridge`). Running it in Tier 2 would force Tier 3 stem separation on every track. | Keep `allin1` in `tools/models/` as an offline Python benchmark only. In Tier 2, segment over bar-synchronous harmonic, percussive and sub-band features with dance labels (`intro`, `buildup`, `drop`, `breakdown`, `cooldown`, `outro`). |
| Embeddings (M6) | CLAP and MERT cosine similarity do not predict blend compatibility, and CLAP checkpoints vary in licence even though the code is MIT/CC0. | Use CLAP in M6 for symmetric vibe and text search ("similar to this"), and verify the chosen checkpoint's commercial licence in the model registry (for example `music_audioset_epoch_15_esc_90.14.pt` under MIT, or an Apache-2.0 encoder such as `dasheng`). Rank "what mixes next" with asymmetric phrase-window scoring. |
| Vocal activity (M5) | [product.md](product.md) puts vocal activity in Tier 2 for the coach and set builder, while [architecture.md](architecture.md) assigns no algorithm to it. Waiting for Tier 3 vocal stems would leave un-stemmed tracks without vocal clash checks. | Add a Tier 2 vocal presence detector on the harmonic mid-channel (`M = L + R`, 250 Hz to 4 kHz) using pitch-stability and vibrato features or a small mel CNN. |
| Decode offset (M2, M4) | Lossy decoders (MP3, AAC, Symphonia vs FFmpeg) strip encoder delay and end padding differently. A few milliseconds of padding drift breaks M4's acceptance check (alignment within 1 ms, carried-over cues within 5 ms). | Strip gapless encoder delay and padding canonically at decode time before BLAKE3 audio hashing and Beat This! grid extraction. In M4 Replace, refine `offset_samples` by cross-correlating decoded onset envelopes rather than trusting container headers alone. |
| Global key and BPM (M6–M10) | Global key penalises percussive outros that have no pitched content, misses modulations and non-440 Hz masters, and ignores pitch shift when keylock is off. Global BPM misses swing and syncopation clashes. | Score transitions over bar-synchronous feature windows in `core` and `metrics`. |

## Bar-synchronous feature matrix

Scalar columns (`bpm`, `key`, `loudness`) stay in SQLite for table views and fast `WHERE` filters. Frame-level spectrograms (such as 100 frames a second) are far too large to hold across 100,000 tracks. Between the two sits a fixed 48-byte binary summary per bar (`#[repr(C)]` in Rust), aligned to the Beat This! downbeat grid and stored as one SQLite BLOB per track.

| Bytes | Field | Size | Contents |
| --- | --- | --- | --- |
| `0..12` | `hpcp_chroma` | 12 B | L1-normalised 12-pitch-class harmonic profile (`u8`, `0..=255`) from the whitened harmonic spectrogram above 200 Hz. |
| `12..24` | `drum_grid_16` | 12 B | 16 sixteenth-note steps across 3 percussive bands (kick `< 150 Hz`, snare/clap `150 Hz–2.5 kHz`, hi-hat `> 2.5 kHz`), quantised to 2 bits per band per step (`16 * 6 = 96` bits). |
| `24..30` | `band_energy_db` | 6 B | Loudness (`u8`, 0.5 dB steps) in 6 bands: sub (`< 60 Hz`), punch (`60–150 Hz`), low-mid (`150–400 Hz`), mid (`400 Hz–1 kHz`), presence (`1–4 kHz`), air (`4–16 kHz`). |
| `30..34` | `hpss_and_stems` | 4 B | `harmonic_ratio` (`u8`), `percussive_ratio` (`u8`), `vocal_activity` (`u8`) and `sub_bass_duty` (`u8`, separating short kicks from sustained sub-bass). |
| `34..42` | `groove_and_pitch` | 8 B | `sub_bass_f0_cents` (`u16`), `kick_crest_db` (`u8`), `kick_decay_ms` (`u8`), `swing_ratio` (`u8`), `lhl_syncopation` (`u8`), `stereo_width` (`u8`), `reverb_tail_ratio` (`u8`). |
| `42..48` | `structure_meta` | 6 B | `tension_score` (`u8`), `novelty_score` (`u8`), `section_role` (`u8`: intro, buildup, drop, breakdown, cooldown, outro), `section_confidence` (`u8`), `local_bpm_x100` (`u16`, for drifting vinyl grids). |

A six-minute track at 128 BPM has 768 beats (192 bars in 4/4), which takes `192 * 48 = 9,216` bytes (`9.00 KiB`). Across 3,000 tracks that is `26.37 MiB`; across 100,000 tracks it is `878.91 MiB` on disk. Transitions happen in entry and exit phrases, so keeping 48 boundary bars per track in memory for Next-panel and set-builder searches takes `219.73 MiB` at 100,000 tracks (`6.59 MiB` at 3,000), well inside the 400 MB idle budget and the 50 ms search budget.

## Pitch, tuning offset and bar-level harmony

Global Camelot rules break in four common cases:

- Drum-only intros and outros (`harmonic_ratio < 0.15`) have no pitched content to clash, even when the two tracks carry incompatible global keys.
- Two tracks in 8A (A minor) can still beat in the low-mids if track A's outro holds an F major pedal while track B's intro centres on E Phrygian or G major.
- Overlapping two phrases that both change chords rapidly sounds cluttered; blends hold together when one track anchors a root pedal or both follow the same loop (Lee et al.).
- Vinyl rips, analogue masters and decks played with keylock off sit away from `A4 = 440 Hz`.

### Continuous cent offset and keylock transposition

Before binning spectral peaks into the 12-bin chroma vector in Tier 2, estimate the track's global tuning offset `tau_track` in `[-50, +50]` cents relative to 440 Hz (Harte and Sandler) and store `tuning_cents: f32` on the key table. At mix time `core` works out each deck's effective pitch offset in cents:

```text
delta_cents(deck) = tau_track + (if keylock { 0.0 } else { 1200.0 * log2(bpm_play / bpm_native) })
```

Three behaviours follow directly from `delta_cents`:

- Quarter-tone clash detection. Let `diff_cents` be `delta_cents(B) - delta_cents(A)` wrapped to `[-50, +50]` cents. When `abs(diff_cents) > 25` cents while both decks carry harmonic energy—for instance `+51.2` cents when pushing a track `+3.0%` with keylock off, or mixing a `+30`-cent rip into a `-20`-cent rip with keylock on—`coach` names the cause: turn keylock on or use a percussive bass swap.
- Semitone transposition. At `+5.8%` to `+6.0%` tempo with keylock off, pitch rises by `+97.6` to `+100.9` cents (one semitone). Rotating deck B's chroma vector by `round((delta_cents(B) - delta_cents(A)) / 100.0)` semitones lets `core` see that a 1A (G#m) track played `+5.8%` faster now sounds in 8A (Am).
- Keylock advice. If track A plays at `126.0` BPM (`tau = 0` cents) and track B is a flat vinyl rip at `124.2` BPM (`tau = -25` cents), pitching track B up `+1.45%` to `126.0` BPM adds `+24.9` cents and brings it onto 440 Hz without engaging the time-stretcher.

### Bernardes tonal interval vectors

Raw chroma cosine similarity treats all 12 pitch classes as orthogonal, so C major and G major look little closer than C major and C# major. Instead, `core` maps a bar's L1-normalised 12-bin chroma `c[0..12]` (rotated by any integer semitone shift) into Bernardes, Davies and Guedes' six-component complex Tonal Interval Vector:

```text
T(k) = w_a[k] * sum_{n=0..11} c[n] * exp(-i * 2 * pi * k * n / 12),   k in 1..=6
w_a  = [3.0, 8.0, 11.5, 15.0, 14.5, 7.5]
```

`w_a` are Bernardes' empirical audio consonance weights. From `T_A` and `T_B` on two overlapping bars, `core` gets two numbers:

- Harmonic relatedness, from the Euclidean distance `||T_A - T_B||_2` in tonal interval space, capturing fifth (`k = 5`) and third (`k = 3, 4`) proximity alongside chord overlap.
- Combined interval dissonance, from normalising the summed chroma `c_AB = (c_A + c_B) / ||c_A + c_B||_1` and computing `D_AB = 1 - ||T(c_AB)||_2 / ||w_a||_2`. When two sustained chords form a dissonant cluster together, `||T(c_AB)||_2` shrinks towards zero and `D_AB` rises even if both tracks share a global key label.

### Sensory roughness on sustained partials

When rendering offline transition previews in `sets` or checking tonal blends in `metrics`, Plomp-Levelt and Sethares pairwise roughness supplements TIV dissonance. From the top spectral partials `(f_i, a_i)` of deck A and `(f_j, a_j)` of deck B:

```text
s = 0.24 / (0.0207 * min(f_1, f_2) + 18.96)
d(f_1, f_2, a_1, a_2) = min(a_1, a_2) * (exp(-3.5 * s * |f_2 - f_1|) - exp(-5.75 * s * |f_2 - f_1|))
```

Weighting by `min(a_1, a_2)` ignores quiet overtones and flags loud partials separated by roughly a quarter of a critical bandwidth.

## Groove, microtiming and low-end envelope

Two tracks at the same BPM with zero downbeat offset still fight if their subdivisions, syncopation or sub-150 Hz envelopes disagree.

### Swing ratio

From the percussive spectrogram, map each onset `t_m` between beat `b_n` and `b_{n+1}` to its intra-beat phase `phi_m = (t_m - b_n) / (b_{n+1} - b_n)` in `[0, 1)` and estimate the sixteenth-note swing ratio (Peeters). At 128 BPM a beat is `468.75` ms and a straight first sixteenth lands at `117.2` ms (`25%`). Against a swung groove whose off-beat sixteenth lands near `134.5` ms (`28.7%`), every off-beat hat sits `17.3` ms apart—past the 15 ms threshold where percussion sounds loose, even with kicks locked within 1 ms. When `swing_ratio` differs across an overlap, `coach` can suggest trimming highs above 2 kHz on one deck.

### Three-band step grid and syncopation

Each bar's 16 sixteenth-note steps are quantised across low (`< 150 Hz`), mid (`150 Hz–2.5 kHz`) and high (`> 2.5 kHz`) percussive bands, alongside Fitch and Rosenfeld's Longuet-Higgins and Lee (LHL) syncopation score using the 4/4 metrical weights:

```text
w_16[0..16] = [0, -4, -3, -4, -2, -4, -3, -4, -1, -4, -3, -4, -2, -4, -3, -4]
```

Each onset at a weak step `N` followed by a rest or tie at a stronger step `R` adds `w_16[R] - w_16[N]`. Pairing a straight low-syncopation rhythm with a syncopated loop locks cleanly because the straight pulse anchors the beat; pairing two high-syncopation bars whose 16-step grids have low cosine similarity produces a polyrhythmic clash.

### Sub-150 Hz envelope

Swapping the low band near 150 Hz on the downbeat avoids kick mud ([v3-notes.md](v3-notes.md)), and three per-bar descriptors tell `sets` and `coach` how much low-end overlap a pair tolerates:

- `kick_crest_db`: separates transient kicks from compressed, continuous bass.
- `sub_bass_duty`: fraction of frames in a beat where 30–150 Hz energy stays high. A short punchy kick leaves space between beats for an incoming off-beat bassline; two high-duty rumble, Reese or 808 subs overload headroom unless swapped cleanly on the downbeat.
- `sub_bass_f0_cents`: dominant fundamental below 150 Hz, showing whether two basslines sit on the same bin (phase cancellation risk) or at a clean interval.

## Sub-millisecond beat grids, bar phase and functional cues

A neural beat tracker gets the pulse right to the nearest spectrogram frame, which is still rough enough for two synced kicks to flam or for a hot cue to click.

### Kick-transient and zero-crossing grid snapping

`Beat This!` emits beat and downbeat activations at 50 frames a second (20 ms per frame). A 10 to 20 ms offset causes low-end comb filtering or a double attack when two decks play in sync, and slices mid-cycle when a cue or loop fires. After `Beat This!` sets the coarse beat positions in Tier 2, a deterministic Rust pass refines every beat marker inside a `+-15 ms` search window:

1. On percussive bars (`percussive_ratio >= 0.15`), pick the steepest positive rise on the `< 150 Hz` percussive envelope so the marker sits at the start of the kick transient rather than the body peak behind it. On beatless bars (`percussive_ratio < 0.15`), fall back to broadband onset flux (`150 Hz–8 kHz`), or keep the interpolated tempo grid when no transient is present.
2. Walk backwards up to `0.5 ms` (`22` samples at `44.1 kHz`) to the nearest zero-crossing of the summed mono signal (`L + R`) so hot cues and loops jump cleanly without a click.

### Downbeat bar phase and pickup bars

Many club tracks open with a 1, 2 or 4-beat vocal phrase, snare fill or synth note before the first bar-1 kick. Frame-level downbeat trackers often latch onto that opening sound and put beat 1 on beat 3 or beat 4 for the rest of the track.

Instead of trusting early bars in isolation, Tier 2 scores all four global bar phases `p in {0, 1, 2, 3}` across the whole track at once, weighting:

- `Beat This!` downbeat probabilities across full-drum sections;
- the sharpness of the first full drop impact and `8`, `16` and `32`-bar checkerboard novelty peaks;
- `< 150 Hz` step-0 weight paired with snare and clap energy (`150 Hz–2.5 kHz`) on beats 2 and 4 (`steps 4 and 12`).

Locking bar phase to the main drops keeps the 4-bar and 8-bar phrase count right from start to finish, and any leading beats ahead of the first true bar 1 are marked as a pickup bar (`bar 0`) rather than shifting the whole grid.

### Proposed functional hot cues and memory cues

DJs set cues at mix decision points rather than verse and chorus labels. From the snapped beat grid, bar summary and phrase roles, Tier 2 generates a `proposed` cue set stored separately from user-placed cues:

| Cue | Where it lands | Why a DJ uses it |
| --- | --- | --- |
| `Mix-in` | First clean downbeat (`bar 1 beat 1`, skipping any pickup beats), carrying cold-vocal or beatless-intro flags. | Starting point on the incoming deck without losing bar phase. |
| `Groove / Bass in` | First phrase downbeat where `< 150 Hz` kick and bass reach full level. | Marks the latest bar where the outgoing deck's low EQ should swap out. |
| `Breakdown` | Downbeat where `< 150 Hz` drops out into a mid/high or vocal section. | Safe landing spot for a beatless cut or vocal tease. |
| `Buildup` | Downbeat where `tension_score` begins its climb ahead of a drop. | Aligns two risers for a drop swap or double drop. |
| `Drop 1` / `Drop 2` | First and second main impact downbeats. | Instant jump for high-energy cuts and phrase checks. |
| `Mix-out` + loop candidate | `16` or `32` bars before the outro or final energy drop, plus a `4` or `8`-bar clean drum loop candidate when the outro is under `16` bars or ends cold. | Warns before the track runs out and gives an emergency loop on short edits. |

Every proposed cue snaps to the upward zero-crossing just ahead of its beat marker. Re-running analysis updates only unaccepted proposals and never moves or deletes a cue the user placed, edited or locked; M11 exports accepted cues to rekordbox, Engine DJ, Serato and Traktor.

## Complementarity without Tier 3 stems

HTDemucs stems stay in Tier 3 (on demand) so scanning 100,000 tracks needs neither terabytes of stem cache nor hours of GPU time. Tier 1 and Tier 2 get most of the benefit through three cheaper steps:

- Median-filtering HPSS in Rust. Running Fitzgerald's 1D median filter across time frames of the STFT magnitude `S(t, f)` isolates the harmonic component `H(t, f)`, and filtering across frequency bins isolates the percussive component `P(t, f)`, with no model weights. Key, tuning offset and TIVs read `H(t, f)` so kicks and hats do not skew chroma; drum grids and swing read `P(t, f)` so synth sweeps do not trigger false onsets; `harmonic_ratio` distinguishes drum tools, beatless pads and full-mix bars.
- Six-band spectral slot balance. Summing `band_energy_db` from both phrases after applying the transition style's simulated EQ curve (for example cutting deck B below 150 Hz before a bass swap), normalising the six bands to sum to 1, and taking `1 - std_dev` rewards pairs where track B fills gaps in track A (Davies et al.) while penalising simultaneous harmonic peaks across `150 Hz–4 kHz`.
- Optional stem scoring on shortlisted pairs. Once Tier 3 stems exist for a pair, stem-level accompaniment compatibility (COCOLA, or phase-aware CQT scoring as in PHALAR) can refine vocal-over-instrumental blends.

## Phrase roles and conditional transition styles

Club and electronic tracks organise around energy function rather than pop verse-chorus form: `intro`, `buildup`, `drop`, `breakdown`, `cooldown` and `outro` (as in the Raveform dataset). Single-band energy novelty misses boundaries in techno and progressive tracks where parts enter gradually at steady loudness. Tier 2 therefore runs a Foote checkerboard novelty kernel jointly across four bar channels—sub-150 Hz energy and duty cycle, TIV self-similarity, three-band 16-step drum self-similarity, and vocal activity—followed by dynamic programming with 8, 16 and 32-bar priors anchored to the first full drop and final kick exit.

Compatibility between exit phrase `P_A` and entry phrase `P_B` depends on the transition style `S`:

| Style | Typical window | Simulated mixer move in `metrics` | Main weights in `core` |
| --- | --- | --- | --- |
| Long blend | 32 or 64 bars | Gradual crossfade; deck B `< 150 Hz` cut until the midpoint downbeat, then swapped; mids and highs overlap throughout. | TIV consonance, cent alignment, swing and groove lock, spectral slot balance, no vocal overlap. |
| Downbeat bass swap | 16 or 32 bars | Deck B enters with `< 150 Hz` cut; on the phrase downbeat deck A `< 150 Hz` cuts and deck B `< 150 Hz` moves to unity. | Discounts `< 150 Hz` pitch clash during overlap; weights downbeat lock, mid/high groove lock and post-swap bass continuity. |
| Drop swap / double drop | 8 or 16 bars | Aligns buildup A with buildup B; cuts deck A on the last buildup bar as drop B lands. | Pre-drop tension slope match, key relation at drop impact (unison, fifth or `+2` Camelot hours), loudness step `>= 0 LU`. |
| Breakdown cut | 4 or 8 bars | Overlaps a beatless breakdown or outro of A (`percussive_ratio < 0.15`) with intro B. | Relaxes BPM and groove weights, allowing wide tempo jumps or rhythm changes. |
| Stem vocal blend (Tier 3) | 16 or 32 bars | Places track B's vocal stem over track A's instrumental stems (or the reverse) when stems exist. | Vocal chroma against instrumental TIV, with zero competing vocal activity on the instrumental deck. |

## Additional signals for `metrics`, `coach` and `sets`

- Internal bar tension vs loudness. In a buildup with the kick high-passed out, an accelerating snare roll and an upward riser, RMS loudness drops while perceived tension peaks. `tension_score` combines spectral centroid rise, sub-150 Hz drop-out paired with 1–16 kHz rise, onset density acceleration and TIV dissonance so buildups are not mislabelled as cooldowns. Per [ADR 0012](adr/0012-concept-boards-as-design-target.md), Tension is kept out of the Library table and Inspector tabs as a separate UI surface; it lives inside the bar summary for segmentation and drop-swap scoring.
- Stereo width and tail continuity. Cutting from a wide modern stereo master to a narrow mono vinyl rip collapses the image even at matched LUFS. Mid-to-side energy (`stereo_width`) and `reverb_tail_ratio` flag abrupt spatial drops.
- Mastering crest factor and midrange density. Integrated LUFS alone treats a dynamic `-14 LUFS` vinyl rip or 90s pressing the same as a heavily limited `-6 LUFS` modern club master turned down by `8 dB`. At matched LUFS the denser master packs far more sustained `400 Hz–4 kHz` RMS energy and buries the dynamic track's snare and vocals, while its clipped peaks carry less transient punch. Storing short-term crest factor (peak-to-RMS in dB across `< 150 Hz` and `400 Hz–4 kHz`) alongside LUFS lets `phi_8` and `coach` flag density mismatches and suggest gentler channel gain with a slight mid trim on the denser deck.
- Edge-of-track flags. Inspecting the first and last 16 bars of the bar summary yields deterministic flags for cold vocal intros (vocal activity on bar 1 beat 1), beatless intros or outros, and sustained-bass outros ([v3-notes.md](v3-notes.md)).
- Control-log calibration in Replay. Because Mix logs sample-accurate fader, EQ, filter and tempo moves (`session_event`), Replay (M9) can compare the predicted compatibility of a pair against the measured score the DJ achieved live.

## Pairwise preference learning in `core`

[product.md](product.md) stores the user's A or B choices between rendered transition previews for preference learning. An eight-parameter L2-regularised Bradley-Terry model fits inside `crates/core` with no ML runtime and does not overfit small choice counts.

For a candidate transition `i = (track_A, track_B, P_A, P_B, style_S)`, `metrics` produces `K = 8` sub-scores `phi_i` in `[0, 1]^8`:

1. `phi_1`: TIV harmonic relatedness and consonance at effective playback pitch.
2. `phi_2`: fractional cent alignment, `1 - abs(diff_cents) / 50`.
3. `phi_3`: tempo closeness within the `+-10%` bound.
4. `phi_4`: three-band groove, swing and LHL syncopation lock.
5. `phi_5`: sub-150 Hz envelope fit under style `S`'s EQ curve.
6. `phi_6`: six-band spectral slot balance and midrange masking margin.
7. `phi_7`: vocal clash clearance.
8. `phi_8`: energy, tension, mastering density and stereo continuity against the set's target line.

The linear score is `U(phi_i) = w^T phi_i`, starting at fixed prior weights `w_0`. When the user prefers preview `i` over preview `j`, the Bradley-Terry likelihood is `sigma(w^T (phi_i - phi_j)) = 1 / (1 + exp(-w^T (phi_i - phi_j)))`. Over `M` stored choices `(i_m, j_m)` in SQLite, `core` updates `w` by minimising the strictly convex objective:

```text
L(w) = -sum_{m=1..M} ln sigma(w^T (phi_{i_m} - phi_{j_m})) + (lambda / 2) * ||w - w_0||_2^2
```

With eight parameters and `lambda > 0`, projected gradient descent in Rust converges in microseconds. At `M = 0`, `w = w_0`; as choices accumulate, `w` adapts to the DJ's taste while every suggestion remains a sum of eight named reasons.

## Where each piece lands

| Milestone | Scope | Check |
| --- | --- | --- |
| M2 Library (`v0.3.0`) & M4 Replace (`v0.5.0`) | Canonical gapless encoder-delay and padding stripping in Symphonia before BLAKE3 hashing; onset-envelope cross-correlation for `offset_samples` in Replace. | MP3 and FLAC encodes of the same master align within 1 ms and carry cues across within 5 ms. |
| M5 Analysis (`v0.6.0`) | Rust median-filtering HPSS on the STFT front end; `< 150 Hz` transient and zero-crossing beat-grid snapping plus four-phase downbeat and pickup-bar locking; `tuning_cents` and Bernardes TIVs; three-band 16-step drum grids, swing, LHL syncopation, sub-150 Hz duty cycle and mastering crest factor; Tier 2 vocal presence; 48-byte/bar SQLite BLOB; multi-feature checkerboard phrases with dance role labels and non-destructive `proposed` functional cues (`allin1` kept offline in `tools/models/`). | Bar summary stays at `9.00 KiB` per 192-bar track; beat markers land within `1 ms` of synthetic and fixture kick onsets without mid-cycle clicks; key accuracy benchmarked with and without HPSS and tuning correction against `fixtures-local/` and rekordbox keys; vocal detector false-positive rate measured on fixtures. |
| M6 Discovery (`v0.7.0`) | Commercial licence check on the default CLAP checkpoint; symmetric vibe search (CLAP) separated from asymmetric Next-track phrase scoring. | "After this track" queries return phrase-compatible matches inside the 50 ms search budget. |
| M7 Decks (`v0.8.0`) & M8 Coach (`v0.9.0`) | Continuous `delta_cents` keylock maths, TIV bar dissonance, swing-flam checks and sub-150 Hz duty-cycle overlap rules in `core`, `metrics` and `coach`. | Synthetic pairs with `+3.0%` keylock-off detuning (`+51.2` cents) and known swing offsets trigger the expected prompts within tolerance. |
| M10 Sets (`v0.11.0`) | 2D `(P_A, P_B)` phrase-pair search conditional on transition style, plus the L2-regularised Bradley-Terry weight update in `core`. | Previews score across 16, 32 and 64-bar windows; seeded synthetic A/B choices move `w` deterministically towards the simulated preference. |

## References

- Davies, M. E. P., Hamasaki, M., & Goto, M. (2014). *AutoMashUpper: Automatic creation of multi-song music mashups*. IEEE/ACM TASLP. https://repositorio.inesctec.pt/bitstreams/97188d38-9bf1-46f9-99a3-08d3b656ca4f/download
- Bernardes, G., Davies, M. E. P., & Guedes, C. (2016). *A multi-level tonal interval space for modelling pitch relatedness and musical consonance*. JNMR. https://www.researchgate.net/publication/303595116_A_multi-level_tonal_interval_space_for_modelling_pitch_relatedness_and_musical_consonance
- Bernardes, G., Davies, M. E. P., & Guedes, C. (2017). *A hierarchical harmonic mixing method*. CMMR. https://www.researchgate.net/publication/329158672_A_Hierarchical_Harmonic_Mixing_Method
- Delabaere, L., et al. (2025). *AutoMashup: Automatic music mashups creation*. https://arxiv.org/pdf/2508.06516
- Ciranni, M., et al. (2025). *COCOLA: Coherence-oriented contrastive learning of musical audio representations*. ICASSP.
- *PHALAR: Phasors for learned musical audio representations* (2026). https://arxiv.org/html/2605.03929v2
- *Raveform: A dataset of metrical and functional structure annotations for EDM tracks in DJ mixes* (2025). https://mir-aidj.github.io/raveform/
- *All-In-One metrical and functional structure analysis with neighborhood attentions on demixed audio* (2023). WASPAA. https://github.com/mir-aidj/all-in-one
- Fitzgerald, D. (2010). *Harmonic/percussive separation using median filtering*. DAFx.
- Harte, C., & Sandler, M. (2005). *Automatic chord identification using a quantised chromagram* (tuning estimation). AES.
- Peeters, G. (2020). *How to improve tempo, beat, and downbeat estimation by estimating swing*. ISMIR. https://program.ismir2020.net/static/final_papers/223.pdf
- Fitch, W. T., & Rosenfeld, A. J. (2007). *Perception and production of syncopated rhythms*. Music Perception.
- Lee, J., et al. (2020). *Neural network models for assessing the compatibility of loops*. https://arxiv.org/html/2008.02011v2
- Sethares, W. A. (2005). *Tuning, Timbre, Spectrum, Scale* (Plomp-Levelt roughness parametrisation).
- Zhang, M. M., et al. *Automatic DJ transitions*. https://michaelmzhang.com/papers/djtransitionpaper.pdf
