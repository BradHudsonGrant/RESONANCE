import type { PreviewState } from '../ipc/bindings/PreviewState';
import type { FrameListener } from '../ipc/client';
import { core } from '../ipc/core';
import { emptyTelemetry, frameKind, readTelemetry, TELEMETRY } from '../mix/frames';

export interface PreviewTransport {
  call: typeof core.call;
  onFrame?: (listener: FrameListener) => () => void;
}

/**
 * Coordinates single-track preview playback with the backend audio engine, staying in sync via
 * 60 Hz binary telemetry frames when available and rAF extrapolation otherwise.
 */
export class PreviewPlayer {
  #transport: PreviewTransport;
  #telemetry = emptyTelemetry();
  #stopFrames: (() => void) | undefined;
  #rafId: number | null = null;
  #opSeq = 0;
  #loadSeq = 0;
  #savedVolume = 1;
  #syncPositionMs = 0;
  #syncTimestamp = 0;
  #lastFrameAt = 0;
  #eofRetryAfter = 0;
  #scrubHoldUntil = 0;
  #seekInFlight = false;
  #queuedSeekMs: number | null = null;
  #volumeInFlight = false;
  #queuedVolume: number | null = null;
  #destroyed = false;

  trackId = $state<number | null>(null);
  title = $state<string | null>(null);
  artist = $state<string | null>(null);
  artwork = $state<string | null>(null);
  playing = $state(false);
  positionMs = $state(0);
  durationMs = $state(0);
  volume = $state(1);
  muted = $state(false);
  generation = $state(0);
  error = $state<string | null>(null);

  constructor(transport: PreviewTransport = core) {
    this.#transport = transport;
    if (typeof transport.onFrame === 'function') {
      this.#stopFrames = transport.onFrame((frame) => {
        if (frameKind(frame) !== TELEMETRY) return;
        readTelemetry(frame, this.#telemetry);
        this.#onTelemetry();
      });
    }
  }

  destroy() {
    this.#destroyed = true;
    this.#stopFrames?.();
    this.#stopFrames = undefined;
    this.#stopTicker();
  }

  async refresh(): Promise<void> {
    const seq = ++this.#opSeq;
    const loadAtStart = this.#loadSeq;
    try {
      const state = await this.#transport.call('preview_status');
      if (this.#destroyed || seq < this.#opSeq || loadAtStart !== this.#loadSeq) return;
      this.#applyState(state);
    } catch {
      // Passive status refresh during boot or offline unit tests stays quiet.
    }
  }

  async loadAndPlay(trackId: number, startMs?: number): Promise<void> {
    const seq = ++this.#opSeq;
    const loadSeq = ++this.#loadSeq;
    this.#queuedSeekMs = null;
    this.#scrubHoldUntil = 0;
    this.#eofRetryAfter = 0;
    this.error = null;
    try {
      const state = await this.#transport.call(
        'preview_load_and_play',
        startMs === undefined ? { track_id: trackId } : { track_id: trackId, start_ms: startMs },
      );
      if (this.#destroyed || loadSeq !== this.#loadSeq) return;
      this.#applyState(state, { forcePosition: seq >= this.#opSeq });
    } catch (cause) {
      if (this.#destroyed || loadSeq !== this.#loadSeq) return;
      this.error = messageOf(cause);
    }
  }

  async toggleOrPlayTrack(trackId: number): Promise<void> {
    if (this.trackId === trackId) {
      await this.togglePlay();
    } else {
      await this.loadAndPlay(trackId);
    }
  }

  async togglePlay(): Promise<void> {
    if (this.trackId === null) return;
    const seq = ++this.#opSeq;
    const loadAtStart = this.#loadSeq;
    this.#eofRetryAfter = 0;
    this.error = null;
    try {
      const state = await this.#transport.call('preview_toggle_play');
      if (this.#destroyed || loadAtStart !== this.#loadSeq || seq < this.#opSeq) return;
      this.#applyState(state);
    } catch (cause) {
      if (this.#destroyed || loadAtStart !== this.#loadSeq) return;
      this.error = messageOf(cause);
    }
  }

  async pause(): Promise<void> {
    const seq = ++this.#opSeq;
    const loadAtStart = this.#loadSeq;
    this.error = null;
    try {
      const state = await this.#transport.call('preview_pause');
      if (this.#destroyed || loadAtStart !== this.#loadSeq || seq < this.#opSeq) return;
      this.#applyState(state);
    } catch (cause) {
      if (this.#destroyed || loadAtStart !== this.#loadSeq) return;
      this.error = messageOf(cause);
    }
  }

  async seek(positionMs: number): Promise<void> {
    if (this.trackId === null) return;
    const clamped = this.#clampPosition(positionMs);
    this.positionMs = clamped;
    this.#syncPositionMs = clamped;
    this.#syncTimestamp = nowMs();
    this.#scrubHoldUntil = this.#syncTimestamp + 200;
    this.#eofRetryAfter = 0;

    if (this.#seekInFlight) {
      this.#queuedSeekMs = clamped;
      return;
    }

    await this.#flushSeek(clamped);
  }

  async #flushSeek(targetMs: number): Promise<void> {
    this.#seekInFlight = true;
    const loadAtStart = this.#loadSeq;
    const seq = ++this.#opSeq;
    this.error = null;
    try {
      const state = await this.#transport.call('preview_seek', { position_ms: targetMs });
      if (!this.#destroyed && loadAtStart === this.#loadSeq && seq >= this.#opSeq) {
        this.#applyState(state, { preserveScrub: this.#queuedSeekMs !== null });
      }
    } catch (cause) {
      if (!this.#destroyed && loadAtStart === this.#loadSeq) {
        this.error = messageOf(cause);
      }
    } finally {
      this.#seekInFlight = false;
      const queued = this.#queuedSeekMs;
      this.#queuedSeekMs = null;
      if (!this.#destroyed && queued !== null && loadAtStart === this.#loadSeq) {
        await this.#flushSeek(queued);
      }
    }
  }

  async setVolume(gainLinear: number): Promise<void> {
    const clamped = clampGain(gainLinear);
    this.volume = clamped;
    if (clamped > 0) {
      this.muted = false;
      this.#savedVolume = clamped;
    } else {
      this.muted = true;
    }

    if (this.#volumeInFlight) {
      this.#queuedVolume = clamped;
      return;
    }
    await this.#flushVolume(clamped);
  }

  async #flushVolume(gainLinear: number): Promise<void> {
    this.#volumeInFlight = true;
    const loadAtStart = this.#loadSeq;
    this.error = null;
    try {
      const state = await this.#transport.call('preview_set_volume', { volume: gainLinear });
      if (!this.#destroyed && loadAtStart === this.#loadSeq && this.#queuedVolume === null) {
        this.#applyVolumeOnly(state.volume);
      }
    } catch (cause) {
      if (!this.#destroyed) this.error = messageOf(cause);
    } finally {
      this.#volumeInFlight = false;
      const queued = this.#queuedVolume;
      this.#queuedVolume = null;
      if (!this.#destroyed && queued !== null) {
        await this.#flushVolume(queued);
      }
    }
  }

  async toggleMute(): Promise<void> {
    if (this.muted || this.volume <= 0) {
      const restore = this.#savedVolume > 0 ? this.#savedVolume : 1;
      await this.setVolume(restore);
    } else {
      this.#savedVolume = this.volume;
      await this.setVolume(0);
    }
  }

  #applyVolumeOnly(nextVolume: number) {
    const clamped = clampGain(nextVolume);
    this.volume = clamped;
    if (clamped > 0) {
      this.muted = false;
      this.#savedVolume = clamped;
    } else {
      this.muted = true;
    }
  }

  #applyState(
    state: PreviewState,
    options: { preserveScrub?: boolean; forcePosition?: boolean } = {},
  ) {
    this.trackId = state.track_id;
    this.title = state.title;
    this.artist = state.artist;
    this.artwork = state.artwork;
    this.durationMs = state.duration_ms;
    this.generation = state.generation;
    this.playing = state.playing;
    this.#applyVolumeOnly(state.volume);

    const holdActive =
      !options.forcePosition &&
      (options.preserveScrub ||
        this.#seekInFlight ||
        this.#queuedSeekMs !== null ||
        nowMs() < this.#scrubHoldUntil);
    if (!holdActive) {
      const pos = this.#clampPosition(state.position_ms);
      this.positionMs = pos;
      this.#syncPositionMs = pos;
      this.#syncTimestamp = nowMs();
    }

    if (this.playing) {
      this.#ensureTicker();
    } else {
      this.#stopTicker();
    }
  }

  #onTelemetry() {
    const reading = this.#telemetry.preview;
    if (reading.generation === 0 || reading.generation !== this.generation) return;
    if (this.#seekInFlight || this.#queuedSeekMs !== null || nowMs() < this.#scrubHoldUntil) {
      return;
    }

    const now = nowMs();
    this.#lastFrameAt = now;
    if (reading.durationSeconds > 0) {
      const nextDuration = Math.round(reading.durationSeconds * 1000);
      if (nextDuration > 0) this.durationMs = nextDuration;
    }
    const nextPos = this.#clampPosition(Math.round(reading.positionSeconds * 1000));
    this.positionMs = nextPos;
    this.#syncPositionMs = nextPos;
    this.#syncTimestamp = now;

    if (this.playing !== reading.playing) {
      this.playing = reading.playing;
      if (this.playing) this.#ensureTicker();
      else this.#stopTicker();
    }
  }

  #ensureTicker() {
    if (this.#rafId !== null || typeof requestAnimationFrame !== 'function') return;
    const step = () => {
      this.#rafId = null;
      if (this.#destroyed || !this.playing) return;
      const now = nowMs();
      // Only extrapolate locally when binary telemetry is absent (e.g. DesktopCore) or quiet.
      if (now - this.#lastFrameAt > 32 && !this.#seekInFlight && now >= this.#scrubHoldUntil) {
        const projected = this.#clampPosition(this.#syncPositionMs + (now - this.#syncTimestamp));
        this.positionMs = projected;
        if (
          this.durationMs > 0 &&
          projected >= this.durationMs &&
          !this.#stopFrames &&
          now >= this.#eofRetryAfter
        ) {
          this.#eofRetryAfter = now + 100;
          void this.refresh();
        }
      }
      this.#rafId = requestAnimationFrame(step);
    };
    this.#rafId = requestAnimationFrame(step);
  }

  #stopTicker() {
    if (this.#rafId !== null && typeof cancelAnimationFrame === 'function') {
      cancelAnimationFrame(this.#rafId);
    }
    this.#rafId = null;
  }

  #clampPosition(ms: number): number {
    const floored = Math.max(0, Math.round(ms));
    return this.durationMs > 0 ? Math.min(floored, this.durationMs) : floored;
  }
}

function clampGain(value: number): number {
  if (!Number.isFinite(value)) return 1;
  return Math.min(1, Math.max(0, value));
}

function nowMs(): number {
  return typeof performance !== 'undefined' ? performance.now() : Date.now();
}

function messageOf(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}
