import { readFileSync } from 'node:fs';
import type { IncomingMessage } from 'node:http';
import { connect } from 'node:net';
import type { Duplex } from 'node:stream';
import type { Plugin } from 'vite';

interface Connection {
  port: number;
  token: string;
}

function readConnection(file: string): Connection | null {
  try {
    return JSON.parse(readFileSync(file, 'utf8')) as Connection;
  } catch {
    return null;
  }
}

const loopbackNames = new Set(['localhost', '127.0.0.1', '[::1]']);

/**
 * Whether an upgrade request comes from a page this dev server served.
 *
 * Any site can open a WebSocket to localhost, and the proxy adds the token, so without this check
 * a page in another tab could reach the core through it. The loopback name is checked as well
 * because a site that points its own hostname at 127.0.0.1 would otherwise match its own origin.
 */
export function isOwnPage(origin: string | undefined, host: string | undefined): boolean {
  if (!origin || !host) return false;
  let page: URL;
  try {
    page = new URL(origin);
  } catch {
    return false;
  }
  return page.host === host && loopbackNames.has(page.hostname);
}

/**
 * Passes the page's WebSocket at `/core` through to `resonance serve`.
 *
 * The connection file is read on every connection, so a restarted core is picked up without
 * restarting the dev server. The token is added here and never reaches the page.
 */
export function coreProxy(connectionFile: string): Plugin {
  return {
    name: 'resonance-core-proxy',
    configureServer(server) {
      server.httpServer?.on('upgrade', (request: IncomingMessage, socket: Duplex, head: Buffer) => {
        if (request.url !== '/core') return;
        if (!isOwnPage(request.headers.origin, request.headers.host)) {
          socket.end('HTTP/1.1 403 Forbidden\r\nConnection: close\r\n\r\n');
          return;
        }
        const connection = readConnection(connectionFile);
        if (!connection) {
          socket.end('HTTP/1.1 503 Service Unavailable\r\nConnection: close\r\n\r\n');
          return;
        }
        const core = connect(connection.port, '127.0.0.1', () => {
          const headers = Object.entries(request.headers)
            .filter(([name]) => name !== 'host' && name !== 'authorization')
            .flatMap(([name, value]) => [value].flat().map((one) => `${name}: ${one}`));
          core.write(
            [
              'GET / HTTP/1.1',
              `host: 127.0.0.1:${connection.port}`,
              `authorization: Bearer ${connection.token}`,
              ...headers,
              '\r\n',
            ].join('\r\n'),
          );
          core.write(head);
          core.pipe(socket);
          socket.pipe(core);
        });
        core.on('error', () => socket.destroy());
        socket.on('error', () => core.destroy());
      });
    },
  };
}
