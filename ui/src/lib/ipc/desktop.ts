import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import type { ApiError } from './bindings/ApiError';
import type { Request } from './bindings/Request';
import type { Response } from './bindings/Response';
import {
  type ActivityChangeListener,
  type Command,
  CoreError,
  type LibraryChangeListener,
  type ParamsOf,
  type ResultOf,
} from './client';

/** Uses the same commands in the installed app, without opening a local socket. */
export class DesktopCore {
  async call<C extends Command>(command: C, ...params: ParamsOf<C>): Promise<ResultOf<C>> {
    const request = (params.length ? { command, params: params[0] } : { command }) as Request;
    try {
      const response = await invoke<Response>('dispatch', { request });
      return response.result as ResultOf<C>;
    } catch (error) {
      if (
        typeof error === 'object' &&
        error !== null &&
        'code' in error &&
        'message' in error &&
        typeof error.code === 'string' &&
        typeof error.message === 'string'
      ) {
        const failure = error as ApiError;
        throw new CoreError(failure.code, failure.message);
      }
      throw new CoreError(
        'disconnected',
        'The desktop core did not answer. Close and reopen Resonance, then try again.',
      );
    }
  }

  onLibraryChange(listener: LibraryChangeListener): () => void {
    let active = true;
    let unlisten: (() => void) | undefined;
    void listen<number>('resonance://library-changed', (event) => {
      if (active) listener(event.payload);
    }).then((stop) => {
      if (active) unlisten = stop;
      else stop();
    });
    return () => {
      active = false;
      unlisten?.();
    };
  }

  onActivityChange(listener: ActivityChangeListener): () => void {
    let active = true;
    let unlisten: (() => void) | undefined;
    void listen<number>('resonance://activity-changed', (event) => {
      if (active) listener(event.payload);
    }).then((stop) => {
      if (active) unlisten = stop;
      else stop();
    });
    return () => {
      active = false;
      unlisten?.();
    };
  }
}
