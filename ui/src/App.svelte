<script lang="ts">
  import { core } from './lib/ipc/core';
  import { trackCount } from './lib/library/format';
  import Inspector from './lib/library/Inspector.svelte';
  import Library from './lib/library/Library.svelte';
  import ScanFolder from './lib/library/ScanFolder.svelte';
  import { TrackPages } from './lib/library/track-pages.svelte';
  import CommandBar, { type Command } from './lib/shell/CommandBar.svelte';
  import PlayerBar from './lib/shell/PlayerBar.svelte';
  import Rail from './lib/shell/Rail.svelte';

  const commandInput = 'command-bar';
  const pages = new TrackPages((offset, limit) => core.call('tracks', { offset, limit }));

  let inspectorOpen = $state(true);
  let scanOpen = $state(false);
  const track = $derived(pages.selectedTrack);

  const commands: Command[] = $derived([
    { name: 'Scan a music folder', run: () => (scanOpen = true) },
    {
      name: inspectorOpen ? 'Hide the inspector' : 'Show the inspector',
      shortcut: 'I',
      run: () => (inspectorOpen = !inspectorOpen),
    },
    { name: 'Go to the first track', shortcut: 'Home', run: () => goTo(0) },
    { name: 'Go to the last track', shortcut: 'End', run: () => goTo(pages.total - 1) },
    { name: 'Reload the library', run: reload },
  ]);

  function goTo(index: number) {
    if (pages.total > 0) pages.selected = index;
  }

  function reload() {
    pages.load();
  }

  function typing(target: EventTarget | null): boolean {
    return (
      target instanceof HTMLElement &&
      (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName))
    );
  }

  function onkeydown(event: KeyboardEvent) {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
      event.preventDefault();
      document.getElementById(commandInput)?.focus();
    } else if (
      event.key.toLowerCase() === 'i' &&
      !event.ctrlKey &&
      !event.metaKey &&
      !event.altKey &&
      !typing(event.target)
    ) {
      event.preventDefault();
      inspectorOpen = !inspectorOpen;
    }
  }

  pages.load();
</script>

<svelte:window {onkeydown} />

<div class="shell">
  <div class="rail"><Rail /></div>
  <div class="bar">
    <CommandBar
      space="Library"
      summary={pages.state.kind === 'ready' ? trackCount(pages.total) : ''}
      {commands}
      inputId={commandInput}
    />
  </div>
  <main class="workspace">
    <Library
      {pages}
      bind:selected={pages.selected}
      onopen={() => (inspectorOpen = true)}
      onscan={() => (scanOpen = true)}
    />
  </main>
  {#if inspectorOpen}
    <div class="inspector">
      <Inspector {track} onclose={() => (inspectorOpen = false)} />
    </div>
  {/if}
  <div class="player"><PlayerBar {track} /></div>
</div>

<ScanFolder
  bind:open={scanOpen}
  scan={(folder) => core.call('scan', { folder })}
  onscanned={reload}
/>

<style>
  .shell {
    display: grid;
    grid-template:
      'rail bar inspector' auto
      'rail workspace inspector' 1fr
      'rail player player' auto
      / auto 1fr auto;
    height: 100vh;
    overflow: hidden;
  }

  .rail {
    display: grid;
    grid-area: rail;
  }

  .bar {
    grid-area: bar;
  }

  .workspace {
    grid-area: workspace;
    min-width: 0;
    min-height: 0;
    background: var(--color-background);
  }

  .inspector {
    display: grid;
    grid-area: inspector;
    min-height: 0;
  }

  .player {
    grid-area: player;
  }
</style>
