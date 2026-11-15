import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreError } from './client';
import { DesktopCore } from './desktop';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(listen).mockReset();
});

describe('DesktopCore', () => {
  it('sends a command without parameters through the native dispatcher and returns its result', async () => {
    vi.mocked(invoke).mockResolvedValue({ command: 'app_info', result: { version: '0.1.0' } });

    await expect(new DesktopCore().call('app_info')).resolves.toEqual({ version: '0.1.0' });

    expect(invoke).toHaveBeenCalledExactlyOnceWith('dispatch', {
      request: { command: 'app_info' },
    });
  });

  it('preserves the parameters of a native library page request', async () => {
    const page = { total: 100_000, offset: 99_800, tracks: [] };
    vi.mocked(invoke).mockResolvedValue({ command: 'tracks', result: page });

    await expect(new DesktopCore().call('tracks', { offset: 99_800, limit: 200 })).resolves.toEqual(
      page,
    );

    expect(invoke).toHaveBeenCalledExactlyOnceWith('dispatch', {
      request: { command: 'tracks', params: { offset: 99_800, limit: 200 } },
    });
  });

  it('preserves the code and actionable message of an error from the core', async () => {
    const failure = { code: 'scan_failed', message: 'The folder is missing. Check its path.' };
    vi.mocked(invoke).mockRejectedValue(failure);

    const scan = new DesktopCore().call('scan', { folder: 'missing' });

    await expect(scan).rejects.toBeInstanceOf(CoreError);
    await expect(scan).rejects.toMatchObject(failure);
  });

  it.each(['The native command is unavailable.', null, undefined, {}])(
    'says how to recover when the native bridge rejects with %j',
    async (failure: unknown) => {
      vi.mocked(invoke).mockRejectedValue(failure);

      const info = new DesktopCore().call('app_info');

      await expect(info).rejects.toBeInstanceOf(CoreError);
      await expect(info).rejects.toMatchObject({ code: 'disconnected' });
      await expect(info).rejects.toThrow(/restart|reopen|try again/i);
    },
  );

  it('subscribes to native library-changed events and cleans up on unsubscribe', async () => {
    let handler: ((event: { payload: number }) => void) | undefined;
    const unlisten = vi.fn();
    vi.mocked(listen).mockImplementation(async (_name, callback) => {
      handler = callback as (event: { payload: number }) => void;
      return unlisten;
    });

    const seen: number[] = [];
    const stop = new DesktopCore().onLibraryChange((revision) => seen.push(revision));
    await Promise.resolve();
    handler?.({ payload: 5 });
    stop();
    handler?.({ payload: 6 });

    expect(seen).toEqual([5]);
    expect(unlisten).toHaveBeenCalledOnce();
  });
});
