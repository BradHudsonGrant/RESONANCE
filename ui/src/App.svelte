<script lang="ts">
  import { core } from './lib/ipc/core';
  import Inspector from './lib/library/Inspector.svelte';
  import Library from './lib/library/Library.svelte';
  import ScanFolder from './lib/library/ScanFolder.svelte';
  import { TrackPages } from './lib/library/track-pages.svelte';
  import Mix from './lib/mix/Mix.svelte';
  import CommandBar, { type Command } from './lib/shell/CommandBar.svelte';
  import PlayerBar from './lib/shell/PlayerBar.svelte';
  import SettingsSheet from './lib/shell/SettingsSheet.svelte';
  import Sidebar, { type SpaceName } from './lib/shell/Sidebar.svelte';
  import TopBar from './lib/shell/TopBar.svelte';
  import TransitionDock from './lib/shell/TransitionDock.svelte';
  import Sheet from './lib/design/Sheet.svelte';

  const commandInput = 'command-bar';
  const pages = new TrackPages((offset, limit) => core.call('tracks', { offset, limit }));

  let space = $state<SpaceName>('Library');
  let innerWidth = $state(1586);
  let innerHeight = $state(992);

  let userSidebarCollapsed = $state<boolean | null>(null);
  let userDockCollapsed = $state<boolean | null>(null);
  let inspectorOpen = $state(true);
  let settingsOpen = $state(false);
  let scanOpen = $state(false);

  const isNarrow = $derived(innerWidth < 1280);
  const sidebarCollapsed = $derived(
    userSidebarCollapsed !== null ? userSidebarCollapsed : innerWidth < 1360,
  );
  const dockCollapsed = $derived(
    userDockCollapsed !== null ? userDockCollapsed : innerHeight < 820,
  );

  const track = $derived(pages.selectedTrack);

  function toggleSidebar() {
    userSidebarCollapsed = !sidebarCollapsed;
  }

  function toggleDock() {
    userDockCollapsed = !dockCollapsed;
  }

  const commands: Command[] = $derived([
    space === 'Mix'
      ? { name: 'Go to the library', shortcut: '1', run: () => (space = 'Library') }
      : { name: 'Go to Mix', shortcut: '4', run: () => (space = 'Mix') },
    { name: 'Scan a music folder', run: () => (scanOpen = true) },
    {
      name: inspectorOpen ? 'Hide the inspector' : 'Show the inspector',
      shortcut: 'I',
      run: () => (inspectorOpen = !inspectorOpen),
    },
    {
      name: sidebarCollapsed ? 'Expand the sidebar' : 'Collapse the sidebar',
      shortcut: '[',
      run: toggleSidebar,
    },
    {
      name: dockCollapsed ? 'Expand the dock' : 'Collapse the dock',
      shortcut: 'D',
      run: toggleDock,
    },
    { name: 'Open settings', run: () => (settingsOpen = true) },
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
    const isTyping = typing(event.target);
    const key = event.key;
    const mod = event.ctrlKey || event.metaKey;

    if (mod && key.toLowerCase() === 'k') {
      event.preventDefault();
      document.getElementById(commandInput)?.focus();
      return;
    }

    if (mod && !event.altKey) {
      if (key === '1') {
        event.preventDefault();
        space = 'Library';
        return;
      }
      if (key === '4') {
        event.preventDefault();
        space = 'Mix';
        return;
      }
    }

    if (isTyping) return;

    if (key === '/' && !mod && !event.altKey) {
      event.preventDefault();
      document.getElementById(commandInput)?.focus();
    } else if (key.toLowerCase() === 'i' && !mod && !event.altKey) {
      event.preventDefault();
      inspectorOpen = !inspectorOpen;
    } else if (key === '[' && !mod && !event.altKey) {
      event.preventDefault();
      toggleSidebar();
    } else if (key.toLowerCase() === 'd' && !mod && !event.altKey) {
      event.preventDefault();
      toggleDock();
    } else if (key === '1' && !mod && !event.altKey) {
      event.preventDefault();
      space = 'Library';
    } else if (key === '4' && !mod && !event.altKey) {
      event.preventDefault();
      space = 'Mix';
    }
  }

  pages.load();
</script>

<svelte:window bind:innerWidth bind:innerHeight {onkeydown} />

<div
  class="app-shell"
  style:--shell-sidebar-col={sidebarCollapsed
    ? 'var(--sidebar-collapsed-width)'
    : 'var(--sidebar-width)'}
  style:--shell-inspector-col={!isNarrow && inspectorOpen && space === 'Library'
    ? 'clamp(var(--inspector-min-width), 26.5vw, var(--inspector-max-width))'
    : '0px'}
>
  <div class="topbar">
    <TopBar onsettings={() => (settingsOpen = true)} />
  </div>

  <div class="sidebar">
    <Sidebar
      activeSpace={space}
      collapsed={sidebarCollapsed}
      trackCount={pages.state.kind === 'ready' ? pages.total : null}
      onSelectSpace={(nextSpace) => (space = nextSpace)}
      onToggleCollapse={toggleSidebar}
    />
  </div>

  <div class="workspace">
    <CommandBar {commands} inputId={commandInput} />
    <main class="workspace-content">
      {#if space === 'Mix'}
        <Mix />
      {:else}
        <Library
          {pages}
          bind:selected={pages.selected}
          onopen={() => (inspectorOpen = true)}
          onscan={() => (scanOpen = true)}
        />
      {/if}
    </main>
  </div>

  {#if !isNarrow && inspectorOpen && space === 'Library'}
    <div class="inspector-dock">
      <Inspector {track} onclose={() => (inspectorOpen = false)} />
    </div>
  {/if}

  <div class="dock">
    <TransitionDock collapsed={dockCollapsed} ontoggle={toggleDock} />
  </div>

  <div class="player">
    <PlayerBar />
  </div>
</div>

{#if isNarrow && inspectorOpen && space === 'Library'}
  <Sheet bind:open={inspectorOpen} variant="drawer" title="Inspector">
    <Inspector {track} onclose={() => (inspectorOpen = false)} />
  </Sheet>
{/if}

<SettingsSheet bind:open={settingsOpen} onclose={() => (settingsOpen = false)} />

<ScanFolder
  bind:open={scanOpen}
  scan={(folder) => core.call('scan', { folder })}
  onscanned={reload}
/>

<style>
  .app-shell {
    display: grid;
    width: 100vw;
    height: 100vh;
    overflow: hidden;
    background: var(--color-background);
    color: var(--color-text);
    grid-template-columns:
      var(--shell-sidebar-col, var(--sidebar-width))
      minmax(var(--workspace-min-width, 520px), 1fr)
      var(--shell-inspector-col, 0px);
    grid-template-rows:
      var(--topbar-height)
      minmax(var(--workspace-min-height, 280px), 1fr)
      auto
      calc(var(--player-height) + var(--player-inset-bottom));
    grid-template-areas:
      'topbar  topbar     topbar'
      'sidebar workspace  inspector'
      'dock    dock       dock'
      'player  player     player';
  }

  .topbar {
    grid-area: topbar;
  }

  .sidebar {
    grid-area: sidebar;
    min-height: 0;
    overflow: hidden;
  }

  .workspace {
    grid-area: workspace;
    display: grid;
    grid-template-rows: auto 1fr;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }

  .workspace-content {
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    background: var(--color-background);
  }

  .inspector-dock {
    grid-area: inspector;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }

  .dock {
    grid-area: dock;
  }

  .player {
    grid-area: player;
  }
</style>
