import { describe, expect, it, vi } from 'vitest';

import type { PreviewState } from '../ipc/bindings/PreviewState';
import type { FrameListener } from '../ipc/client';
import { TELEMETRY } from '../mix/frames';
import { PreviewPlayer, type PreviewTransport } from './preview.svelte';

function makeTelemetryFrame(
  generation: number,
  playing: boolean,
  positionSeconds: number,
  durationSeconds: number,
): ArrayBuffer {
  const frame = new ArrayBuffer(144);
  const view = new DataView(frame);
  view.setUint8(0, TELEMETRY);
  view.setFloat64(120, positionSeconds, true);
  view.setFloat64(128, durationSeconds, true);
  view.setUint32(136, generation, true);
  view.setUint8(140, playing ? 1 : 0);
  return frame;
}

function previewState(overrides: Partial<PreviewState> = {}): PreviewState {
  return {
    track_id: 1,
    title: 'Warm Up',
    artist: 'Ada Marsh',
    artwork: null,
    playing: true,
    position_ms: 0,
    duration_ms: 180_000,
    volume: 1,
    generation: 1,
    ...overrides,
  };
}

describe('PreviewPlayer', () => {
  it('loads a track, ignores stale frames and follows matching binary telemetry through EOF', async () => {
    let listener: FrameListener | undefined;
    const call = vi.fn().mockResolvedValueOnce(previewState({ track_id: 7, generation: 2 }));
    const transport: PreviewTransport = {
      call: call as PreviewTransport['call'],
      onFrame: (fn) => {
        listener = fn;
        return () => {
          listener = undefined;
        };
      },
    };
    const player = new PreviewPlayer(transport);

    await player.loadAndPlay(7);
    expect(call).toHaveBeenCalledWith('preview_load_and_play', { track_id: 7 });
    expect(player.trackId).toBe(7);
    expect(player.title).toBe('Warm Up');
    expect(player.playing).toBe(true);
    expect(player.durationMs).toBe(180_000);

    // Frame from an older generation is ignored.
    listener?.(makeTelemetryFrame(1, false, 99, 100), 10);
    expect(player.positionMs).toBe(0);
    expect(player.playing).toBe(true);

    // Frame matching generation updates position and EOF state.
    listener?.(makeTelemetryFrame(2, true, 12.345, 180), 20);
    expect(player.positionMs).toBe(12_345);
    expect(player.playing).toBe(true);

    listener?.(makeTelemetryFrame(2, false, 180, 180), 30);
    expect(player.positionMs).toBe(180_000);
    expect(player.playing).toBe(false);

    player.destroy();
  });

  it('coalesces rapid seeks and preserves saved volume across mute/unmute', async () => {
    let resolveFirstSeek!: (state: PreviewState) => void;
    const call = vi.fn((cmd: string, params?: Record<string, unknown>): Promise<PreviewState> => {
      if (cmd === 'preview_load_and_play') {
        return Promise.resolve(previewState({ track_id: 5, generation: 1 }));
      }
      if (cmd === 'preview_seek') {
        if (params?.position_ms === 10_000) {
          return new Promise((resolve) => {
            resolveFirstSeek = resolve;
          });
        }
        return Promise.resolve(
          previewState({
            track_id: 5,
            generation: 2,
            position_ms: Number(params?.position_ms ?? 0),
          }),
        );
      }
      if (cmd === 'preview_set_volume') {
        return Promise.resolve(
          previewState({
            track_id: 5,
            generation: 2,
            volume: Number(params?.volume ?? 1),
          }),
        );
      }
      return Promise.resolve(previewState());
    });

    const player = new PreviewPlayer({ call: call as PreviewTransport['call'] });
    await player.loadAndPlay(5);

    const first = player.seek(10_000);
    await player.seek(25_000);
    await player.seek(40_000);
    expect(player.positionMs).toBe(40_000);

    resolveFirstSeek(previewState({ track_id: 5, generation: 1, position_ms: 10_000 }));
    await first;

    const seeks = call.mock.calls.filter(([cmd]) => cmd === 'preview_seek');
    expect(seeks.map(([, args]) => args)).toEqual([
      { position_ms: 10_000 },
      { position_ms: 40_000 },
    ]);

    await player.setVolume(0.65);
    expect(player.muted).toBe(false);
    await player.toggleMute();
    expect(player.muted).toBe(true);
    expect(player.volume).toBe(0);
    await player.toggleMute();
    expect(player.muted).toBe(false);
    expect(player.volume).toBe(0.65);

    player.destroy();
  });

  it('ignores an older load resolution when a newer track is requested', async () => {
    let resolveSlowLoad!: (state: PreviewState) => void;
    const call = vi.fn((cmd: string, params?: { track_id?: number }): Promise<PreviewState> => {
      if (cmd === 'preview_load_and_play' && params?.track_id === 1) {
        return new Promise((resolve) => {
          resolveSlowLoad = resolve;
        });
      }
      return Promise.resolve(previewState({ track_id: 2, title: 'Second Track', generation: 2 }));
    });

    const player = new PreviewPlayer({ call: call as PreviewTransport['call'] });
    const first = player.loadAndPlay(1);
    await player.loadAndPlay(2);
    resolveSlowLoad(previewState({ track_id: 1, title: 'First Track', generation: 1 }));
    await first;

    expect(player.trackId).toBe(2);
    expect(player.title).toBe('Second Track');
    player.destroy();
  });

  it('restores position on refresh when idle and records playback errors cleanly', async () => {
    const call = vi.fn((cmd: string): Promise<PreviewState> => {
      if (cmd === 'preview_status') {
        return Promise.resolve(
          previewState({ track_id: 9, playing: false, position_ms: 92_000, generation: 4 }),
        );
      }
      return Promise.reject(new Error('The file could not be opened.'));
    });

    const player = new PreviewPlayer({ call: call as PreviewTransport['call'] });
    await player.refresh();
    expect(player.trackId).toBe(9);
    expect(player.positionMs).toBe(92_000);
    expect(player.playing).toBe(false);

    await player.loadAndPlay(10);
    expect(player.error).toBe('The file could not be opened.');
    player.destroy();
  });

  it('sends nothing on focus while auto-preview is off', async () => {
    const call = vi.fn();
    const player = new PreviewPlayer({ call: call as PreviewTransport['call'] });
    await player.focus(3);
    expect(call).not.toHaveBeenCalled();
    player.destroy();
  });

  it('keeps one focus call in flight and only the latest row waiting', async () => {
    const replies: Array<(state: PreviewState) => void> = [];
    const call = vi.fn((command: string, params?: { track_id?: number }) => {
      if (command === 'set_preview_settings') {
        return Promise.resolve({
          auto_preview: true,
          start: { kind: 'thirty_percent' },
          max_start_seconds: 600,
        });
      }
      return new Promise<PreviewState>((resolve) => {
        replies.push(() =>
          resolve(previewState({ track_id: params?.track_id ?? null, generation: replies.length })),
        );
      });
    });
    const player = new PreviewPlayer({ call: call as unknown as PreviewTransport['call'] });
    await player.changeSettings({ auto_preview: true });

    // A held arrow key: ten rows before the first load answers.
    const first = player.focus(1);
    for (let row = 2; row <= 10; row += 1) void player.focus(row);
    const focusCalls = () => call.mock.calls.filter(([command]) => command === 'preview_focus');
    expect(focusCalls()).toHaveLength(1);

    replies[0]?.(previewState());
    await vi.waitFor(() => expect(focusCalls()).toHaveLength(2));
    expect(focusCalls()[1]).toEqual(['preview_focus', { track_id: 10 }]);

    replies[1]?.(previewState());
    await first;
    expect(focusCalls()).toHaveLength(2);
    expect(player.trackId).toBe(10);
    player.destroy();
  });
});
