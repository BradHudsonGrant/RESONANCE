import { fileURLToPath } from 'node:url';

import { svelte } from '@sveltejs/vite-plugin-svelte';
import { svelteTesting } from '@testing-library/svelte/vite';
import { defineConfig } from 'vite';

import { coreProxy } from './dev/core-proxy.ts';
import { bundleNotices } from './dev/bundle-notices.ts';

const connectionFile = fileURLToPath(new URL('../target/dev/connection.json', import.meta.url));

export default defineConfig({
  // `svelteTesting` only acts under Vitest, where it makes Svelte load its browser build.
  plugins: [svelte(), svelteTesting(), coreProxy(connectionFile), bundleNotices()],
  // The desktop CSP permits fonts from the bundled origin, so they must stay as files.
  build: { assetsInlineLimit: 0 },
  // A fixed address, so the page and anything that drives it agree on where it is.
  server: { port: 5173, strictPort: true },
});
