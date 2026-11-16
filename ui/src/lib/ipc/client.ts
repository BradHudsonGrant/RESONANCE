import type { ApiError } from './bindings/ApiError';
import type { Call } from './bindings/Call';
import type { Reply } from './bindings/Reply';
import type { Request } from './bindings/Request';
import type { Response } from './bindings/Response';

export type Command = Request['command'];
export type ParamsOf<C extends Command> =
  Extract<Request, { command: C }> extends { params: infer P } ? [params: P] : [];
export type ResultOf<C extends Command> = Extract<Response, { command: C }>['result'];

export class CoreError extends Error {
  constructor(
    readonly code: ApiError['code'] | 'disconnected',
    message: string,
  ) {
    super(message);
    this.name = 'CoreError';
  }
}

interface Waiting {
  resolve: (result: never) => void;
  reject: (error: CoreError) => void;
}

/** A binary frame and the page time it arrived at, on the clock `performance.now()` reads. */
export type FrameListener = (frame: ArrayBuffer, arrivedAt: number) => void;

/** Fired when a background source watcher finds new or modified files on disk. */
export type LibraryChangeListener = (revision: number) => void;

/** Fired when a background or interactive job starts, advances or finishes. */
export type ActivityChangeListener = (revision: number) => void;

/** Sends typed commands to the core over one socket, opened on the first call and again after a drop. */
export class Core {
  #open: () => WebSocket;
  #socket: Promise<WebSocket> | null = null;
  #waiting = new Map<number, Waiting>();
  #frameListeners = new Set<FrameListener>();
  #libraryListeners = new Set<LibraryChangeListener>();
  #activityListeners = new Set<ActivityChangeListener>();
  #nextId = 1;

  constructor(open: () => WebSocket) {
    this.#open = open;
  }

  async call<C extends Command>(command: C, ...params: ParamsOf<C>): Promise<ResultOf<C>> {
    const socket = await this.#connected();
    const id = this.#nextId++;
    const request = (params.length ? { command, params: params[0] } : { command }) as Request;
    const call: Call = { id, request };
    return new Promise<ResultOf<C>>((resolve, reject) => {
      this.#waiting.set(id, { resolve, reject });
      socket.send(JSON.stringify(call));
    });
  }

  /**
   * Listens for the core's binary frames: meters, playheads and waveforms. They arrive once a
   * call has opened the socket. Returns a function that stops listening.
   */
  onFrame(listener: FrameListener): () => void {
    this.#frameListeners.add(listener);
    return () => this.#frameListeners.delete(listener);
  }

  /**
   * Listens for library revisions pushed when a watched source changes on disk. Returns a function
   * that stops listening.
   */
  onLibraryChange(listener: LibraryChangeListener): () => void {
    this.#libraryListeners.add(listener);
    return () => this.#libraryListeners.delete(listener);
  }

  /**
   * Listens for Activity job queue revisions pushed when jobs start, advance or finish. Returns a
   * function that stops listening.
   */
  onActivityChange(listener: ActivityChangeListener): () => void {
    this.#activityListeners.add(listener);
    return () => this.#activityListeners.delete(listener);
  }

  #connected(): Promise<WebSocket> {
    this.#socket ??= new Promise<WebSocket>((resolve, reject) => {
      const socket = this.#open();
      socket.binaryType = 'arraybuffer';
      socket.addEventListener('open', () => resolve(socket));
      socket.addEventListener('message', (event) => {
        if (event.data instanceof ArrayBuffer) {
          for (const listener of this.#frameListeners) listener(event.data, event.timeStamp);
        } else {
          this.#settle(String(event.data));
        }
      });
      socket.addEventListener('close', () => {
        this.#socket = null;
        const gone = new CoreError(
          'disconnected',
          'The core is not running. Start it with just serve, then try again.',
        );
        reject(gone);
        for (const waiting of this.#waiting.values()) waiting.reject(gone);
        this.#waiting.clear();
      });
    });
    return this.#socket;
  }

  #settle(text: string) {
    const reply = JSON.parse(text) as Reply;
    if (reply.id === null) {
      if (reply.response?.command === 'library_changed') {
        for (const listener of this.#libraryListeners) {
          listener(reply.response.result.revision);
        }
      } else if (reply.response?.command === 'activity_changed') {
        for (const listener of this.#activityListeners) {
          listener(reply.response.result.revision);
        }
      }
      return;
    }
    const waiting = this.#waiting.get(reply.id);
    if (!waiting) return;
    this.#waiting.delete(reply.id);
    if (reply.error) waiting.reject(new CoreError(reply.error.code, reply.error.message));
    else if (reply.response) waiting.resolve(reply.response.result as never);
  }
}
