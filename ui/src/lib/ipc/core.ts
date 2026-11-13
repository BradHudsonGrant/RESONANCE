import { isTauri } from '@tauri-apps/api/core';

import { Core } from './client';
import { DesktopCore } from './desktop';

/** The app's one connection to the core. In development the dev server passes `/core` through to it. */
export const core = isTauri()
  ? new DesktopCore()
  : new Core(() => new WebSocket(new URL('/core', location.href)));
