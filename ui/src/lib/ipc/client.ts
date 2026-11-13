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

/** Sends typed commands to the core over one socket, opened on the first call and again after a drop. */
export class Core {
  #open: () => WebSocket;
  #socket: Promise<WebSocket> | null = null;
  #waiting = new Map<number, Waiting>();
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

  #connected(): Promise<WebSocket> {
    this.#socket ??= new Promise<WebSocket>((resolve, reject) => {
      const socket = this.#open();
      socket.addEventListener('open', () => resolve(socket));
      socket.addEventListener('message', (event) => this.#settle(String(event.data)));
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
    if (reply.id === null) return;
    const waiting = this.#waiting.get(reply.id);
    if (!waiting) return;
    this.#waiting.delete(reply.id);
    if (reply.error) waiting.reject(new CoreError(reply.error.code, reply.error.message));
    else if (reply.response) waiting.resolve(reply.response.result as never);
  }
}
