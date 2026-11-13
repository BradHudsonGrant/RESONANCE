<script lang="ts">
  import Button from './lib/design/Button.svelte';
  import Wordmark from './lib/design/Wordmark.svelte';
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
  <h1><Wordmark /></h1>
  {#if state.kind === 'loading'}
    <p class="status">Connecting to the core…</p>
  {:else if state.kind === 'ready'}
    <p class="status">
      Core <span class="numeric" data-testid="core-version">{state.version}</span>
    </p>
  {:else}
    <p role="alert">{state.message}</p>
    <Button onclick={load}>Try again</Button>
  {/if}
</main>

<style>
  main {
    display: grid;
    justify-items: start;
    gap: var(--space-3);
    padding: var(--space-6);
  }

  h1 {
    font-size: var(--text-lg);
  }

  p {
    margin: 0;
  }

  .status {
    color: var(--color-text-2);
  }
</style>
