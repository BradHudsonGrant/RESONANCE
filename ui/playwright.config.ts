import { fileURLToPath } from 'node:url';

import { defineConfig } from '@playwright/test';

// Its own port and core, so a dev server and `just serve` can keep running alongside.
const port = 5180;

export default defineConfig({
  testDir: 'e2e',
  testMatch: '**/*.e2e.ts',
  // Cold browser launch on shared CI runners can take over 20 s on the first test.
  timeout: 60_000,
  globalSetup: './e2e/core.ts',
  // One core and one library, so tests run in order.
  workers: 1,
  forbidOnly: Boolean(process.env.CI),
  reporter: 'list',
  outputDir: '../target/e2e/results',
  use: {
    baseURL: `http://localhost:${port}`,
    // Edge is on every Windows machine and on GitHub's runners, and it is the engine the
    // desktop app's webview uses, so nothing extra is downloaded.
    channel: 'msedge',
  },
  webServer: {
    command: `npm run dev -- --port ${port} --strictPort`,
    url: `http://localhost:${port}`,
    env: {
      RESONANCE_CONNECTION_FILE: fileURLToPath(
        new URL('../target/e2e/connection.json', import.meta.url),
      ),
    },
  },
});
