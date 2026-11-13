import { fileURLToPath } from 'node:url';

import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vite';

import { coreProxy } from './dev/core-proxy.ts';

const connectionFile = fileURLToPath(new URL('../target/dev/connection.json', import.meta.url));

export default defineConfig({
  plugins: [svelte(), coreProxy(connectionFile)],
  // A fixed address, so the page and anything that drives it agree on where it is.
  server: { port: 5173, strictPort: true },
});
