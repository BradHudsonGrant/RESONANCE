<script lang="ts">
  import { core } from './lib/ipc/core';

  type State =
    { kind: 'loading' } | { kind: 'ready'; version: string } | { kind: 'error'; message: string };

  let state = $state<State>({ kind: 'loading' });

  async function load() {
    state = { kind: 'loading' };
    try {
      const info = await core.call('app_info');
      state = { kind: 'ready', version: info.version };
    } catch (error) {
      state = { kind: 'error', message: error instanceof Error ? error.message : String(error) };
    }
  }

  load();
</script>

<main>
  <h1>Resonance</h1>
  {#if state.kind === 'loading'}
    <p>Connecting to the core…</p>
  {:else if state.kind === 'ready'}
    <p>Core <span data-testid="core-version">{state.version}</span></p>
  {:else}
    <p role="alert">{state.message}</p>
    <button onclick={load}>Try again</button>
  {/if}
</main>
