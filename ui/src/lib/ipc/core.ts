import { Core } from './client';

/** The app's one connection to the core. In development the dev server passes `/core` through to it. */
export const core = new Core(() => new WebSocket(new URL('/core', location.href)));
