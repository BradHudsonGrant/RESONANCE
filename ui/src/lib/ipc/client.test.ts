import { describe, expect, it } from 'vitest';

import { Core, CoreError } from './client';

class FakeSocket extends EventTarget {
  sent: string[] = [];

  send(text: string) {
    this.sent.push(text);
  }

  opens() {
    this.dispatchEvent(new Event('open'));
  }

  receives(reply: object) {
    this.dispatchEvent(new MessageEvent('message', { data: JSON.stringify(reply) }));
  }

  receivesFrame(frame: ArrayBuffer) {
    this.dispatchEvent(new MessageEvent('message', { data: frame }));
  }

  closes() {
    this.dispatchEvent(new Event('close'));
  }
}

function coreWithSockets() {
  const sockets: FakeSocket[] = [];
  const core = new Core(() => {
    const socket = new FakeSocket();
    sockets.push(socket);
    return socket as unknown as WebSocket;
  });
  return { core, sockets };
}

const settled = () => new Promise((resolve) => setTimeout(resolve));

describe('Core', () => {
  it('sends a call and resolves with the result of the matching reply', async () => {
    const { core, sockets } = coreWithSockets();

    const info = core.call('app_info');
    sockets[0]!.opens();
    await settled();
    expect(JSON.parse(sockets[0]!.sent[0]!)).toEqual({ id: 1, request: { command: 'app_info' } });
    sockets[0]!.receives({
      id: 1,
      response: { command: 'app_info', result: { version: '0.1.0' } },
      error: null,
    });

    await expect(info).resolves.toEqual({ version: '0.1.0' });
  });

  it('matches replies to calls by id when they arrive out of order', async () => {
    const { core, sockets } = coreWithSockets();

    const first = core.call('app_info');
    const second = core.call('app_info');
    sockets[0]!.opens();
    await settled();
    const reply = (id: number, version: string) => ({
      id,
      response: { command: 'app_info', result: { version } },
      error: null,
    });
    sockets[0]!.receives(reply(2, 'second'));
    sockets[0]!.receives(reply(1, 'first'));

    await expect(first).resolves.toEqual({ version: 'first' });
    await expect(second).resolves.toEqual({ version: 'second' });
    expect(sockets).toHaveLength(1);
  });

  it('rejects with the message the core wrote when a reply carries an error', async () => {
    const { core, sockets } = coreWithSockets();

    const info = core.call('app_info');
    sockets[0]!.opens();
    await settled();
    sockets[0]!.receives({
      id: 1,
      response: null,
      error: { code: 'bad_request', message: 'The core could not read that.' },
    });

    await expect(info).rejects.toMatchObject({
      code: 'bad_request',
      message: 'The core could not read that.',
    });
  });

  it('rejects every waiting call when the socket closes, and says how to start the core', async () => {
    const { core, sockets } = coreWithSockets();

    const info = core.call('app_info');
    sockets[0]!.opens();
    await settled();
    sockets[0]!.closes();

    await expect(info).rejects.toBeInstanceOf(CoreError);
    await expect(info).rejects.toMatchObject({ code: 'disconnected' });
    await expect(info).rejects.toThrow('just serve');
  });

  it('rejects a call when the socket never opens', async () => {
    const { core, sockets } = coreWithSockets();

    const info = core.call('app_info');
    sockets[0]!.closes();

    await expect(info).rejects.toMatchObject({ code: 'disconnected' });
  });

  it('hands binary frames to frame listeners and leaves waiting calls alone', async () => {
    const { core, sockets } = coreWithSockets();
    const frames: number[] = [];
    const stop = core.onFrame((frame) => frames.push(new Uint8Array(frame)[0]!));

    const info = core.call('app_info');
    sockets[0]!.opens();
    await settled();
    sockets[0]!.receivesFrame(new Uint8Array([1, 0, 0, 0]).buffer);
    stop();
    sockets[0]!.receivesFrame(new Uint8Array([2, 0, 0, 0]).buffer);
    sockets[0]!.receives({
      id: 1,
      response: { command: 'app_info', result: { version: '0.1.0' } },
      error: null,
    });

    expect(frames).toEqual([1]);
    await expect(info).resolves.toEqual({ version: '0.1.0' });
  });

  it('opens a new socket for the next call after a drop', async () => {
    const { core, sockets } = coreWithSockets();

    const lost = core.call('app_info');
    sockets[0]!.closes();
    await expect(lost).rejects.toBeInstanceOf(CoreError);

    const info = core.call('app_info');
    sockets[1]!.opens();
    await settled();
    sockets[1]!.receives({
      id: 1,
      response: { command: 'app_info', result: { version: '0.1.0' } },
      error: null,
    });

    await expect(info).resolves.toEqual({ version: '0.1.0' });
  });

  it('delivers unsolicited library_changed notifications to listeners until unsubscribed', async () => {
    const { core, sockets } = coreWithSockets();
    const revisions: number[] = [];
    const stop = core.onLibraryChange((revision) => revisions.push(revision));

    const info = core.call('app_info');
    sockets[0]!.opens();
    await settled();
    sockets[0]!.receives({
      id: null,
      response: { command: 'library_changed', result: { revision: 3 } },
      error: null,
    });
    stop();
    sockets[0]!.receives({
      id: null,
      response: { command: 'library_changed', result: { revision: 4 } },
      error: null,
    });
    sockets[0]!.receives({
      id: 1,
      response: { command: 'app_info', result: { version: '0.2.0' } },
      error: null,
    });

    expect(revisions).toEqual([3]);
    await expect(info).resolves.toEqual({ version: '0.2.0' });
  });
});
