<script lang="ts">
  import Sheet from './lib/design/Sheet.svelte';
  import { core } from './lib/ipc/core';
  import Inspector from './lib/library/Inspector.svelte';
  import Library from './lib/library/Library.svelte';
  import ScanFolder from './lib/library/ScanFolder.svelte';
  import { TrackPages } from './lib/library/track-pages.svelte';
  import CommandBar, { type Command } from './lib/shell/CommandBar.svelte';
  import PlayerBar from './lib/shell/PlayerBar.svelte';
  import SettingsSheet from './lib/shell/SettingsSheet.svelte';
  import Sidebar, { type SpaceName } from './lib/shell/Sidebar.svelte';
  import TopBar from './lib/shell/TopBar.svelte';
  import TransitionDock from './lib/shell/TransitionDock.svelte';

  type Fold = 'collapsed' | 'expanded';

  const commandInput = 'command-bar';
  const pages = new TrackPages((offset, limit) => core.call('tracks', { offset, limit }));

  let shell: HTMLDivElement;
  let space = $state<SpaceName>('Library');
  // The window size sets the layout in CSS. These hold only what the user chose over it.
  let sidebarChoice = $state<Fold>();
  let dockChoice = $state<Fold>();
  let inspectorHidden = $state(false);
  let sheetOpen = $state(false);
  let settingsOpen = $state(false);
  let settingsMounted = $state(false);
  let scanOpen = $state(false);
  let scanMounted = $state(false);
  let mixPlaying = $state(false);

  function openSettings() {
    settingsMounted = true;
    settingsOpen = true;
  }

  function openScan() {
    scanMounted = true;
    scanOpen = true;
  }

  const track = $derived(pages.selectedTrack);
  const inspectorDocked = $derived(space === 'Library' && !inspectorHidden);

  /** What the stylesheet decided, read only when the user asks for the opposite. */
  function layout(property: '--sidebar-state' | '--dock-state' | '--inspector-mode'): string {
    return getComputedStyle(shell).getPropertyValue(property).trim();
  }

  function flipped(choice: Fold | undefined, property: '--sidebar-state' | '--dock-state'): Fold {
    return (choice ?? layout(property)) === 'collapsed' ? 'expanded' : 'collapsed';
  }

  function toggleSidebar() {
    sidebarChoice = flipped(sidebarChoice, '--sidebar-state');
  }

  function toggleDock() {
    dockChoice = flipped(dockChoice, '--dock-state');
  }

  function toggleInspector() {
    if (layout('--inspector-mode') === 'sheet') sheetOpen = !sheetOpen;
    else inspectorHidden = !inspectorHidden;
  }

  function showInspector() {
    if (layout('--inspector-mode') === 'sheet') sheetOpen = true;
    else inspectorHidden = false;
  }

  $effect(() => {
    if (!sheetOpen) return;
    const closeWhenDocked = () => {
      if (layout('--inspector-mode') !== 'sheet') sheetOpen = false;
    };
    addEventListener('resize', closeWhenDocked);
    return () => removeEventListener('resize', closeWhenDocked);
  });

  const commands: Command[] = $derived([
    space === 'Mix'
      ? { name: 'Go to the library', shortcut: '1', run: () => (space = 'Library') }
      : { name: 'Go to Mix', shortcut: '4', run: () => (space = 'Mix') },
    { name: 'Scan a music folder', run: openScan },
    { name: 'Show or hide the inspector', shortcut: 'I', run: toggleInspector },
    { name: 'Collapse or expand the sidebar', shortcut: '[', run: toggleSidebar },
    { name: 'Collapse or expand the dock', shortcut: 'D', run: toggleDock },
    { name: 'Open settings', run: openSettings },
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

  const keys: Record<string, () => void> = {
    '/': () => document.getElementById(commandInput)?.focus(),
    i: toggleInspector,
    '[': toggleSidebar,
    d: toggleDock,
    '1': () => (space = 'Library'),
    '4': () => (space = 'Mix'),
  };

  function onkeydown(event: KeyboardEvent) {
    // An open sheet is modal and owns the keyboard, so nothing changes behind it.
    if (event.altKey || document.querySelector('dialog[open]')) return;
    const key = event.key.toLowerCase();
    const command = event.ctrlKey || event.metaKey;

    if (command && key === 'k') {
      event.preventDefault();
      document.getElementById(commandInput)?.focus();
    } else if (command && (key === '1' || key === '4')) {
      event.preventDefault();
      keys[key]?.();
    } else if (!command && !typing(event.target) && keys[key]) {
      event.preventDefault();
      keys[key]();
    }
  }

  pages.load();
</script>

<svelte:window {onkeydown} />

<div
  bind:this={shell}
  class="app-shell"
  data-sidebar={sidebarChoice}
  data-dock={dockChoice}
  data-inspector={inspectorDocked ? undefined : 'hidden'}
>
  <div class="topbar">
    <TopBar onsettings={openSettings} />
  </div>

  <div class="sidebar">
    <Sidebar
      activeSpace={space}
      trackCount={pages.state.kind === 'ready' ? pages.total : null}
      wavePaused={space === 'Mix' && mixPlaying}
      onSelectSpace={(nextSpace) => (space = nextSpace)}
      onCollapse={(collapsed) => (sidebarChoice = collapsed ? 'collapsed' : 'expanded')}
    />
  </div>

  <div class="workspace">
    <CommandBar {commands} inputId={commandInput} />
    <main class="workspace-content">
      {#if space === 'Mix'}
        {#await import('./lib/mix/Mix.svelte') then { default: Mix }}
          <Mix onplayingchange={(playing) => (mixPlaying = playing)} />
        {/await}
      {:else}
        <Library {pages} bind:selected={pages.selected} onopen={showInspector} onscan={openScan} />
      {/if}
    </main>
  </div>

  {#if inspectorDocked}
    <div class="inspector-dock">
      <Inspector {track} onclose={() => (inspectorHidden = true)} />
    </div>
  {/if}

  <!-- The Mix spike has its own decks and transport until the real decks arrive in M7. -->
  {#if space !== 'Mix'}
    <div class="dock">
      <TransitionDock
        onCollapse={(collapsed) => (dockChoice = collapsed ? 'collapsed' : 'expanded')}
      />
    </div>

    <div class="player">
      <PlayerBar />
    </div>
  {/if}
</div>

{#if sheetOpen && space === 'Library'}
  <Sheet bind:open={sheetOpen} variant="drawer" title="Inspector">
    <Inspector {track} inSheet onclose={() => (sheetOpen = false)} />
  </Sheet>
{/if}

{#if settingsMounted}
  <SettingsSheet bind:open={settingsOpen} />
{/if}

{#if scanMounted}
  <ScanFolder
    bind:open={scanOpen}
    scan={(folder) => core.call('scan', { folder })}
    onscanned={reload}
  />
{/if}

<style>
  /*
   * The window size decides the layout here. Each state is a custom property that the sidebar
   * and dock read through style queries, and that App reads when a shortcut flips it. Media
   * queries cannot read tokens, so the breakpoints are written out.
   */
  .app-shell {
    --sidebar-state: expanded;
    --sidebar-column: var(--sidebar-width);
    --dock-state: expanded;
    --inspector-mode: docked;
    --inspector-column: clamp(var(--inspector-min-width), 26.5vw, var(--inspector-max-width));

    display: grid;
    grid-template:
      'topbar topbar topbar' var(--topbar-height)
      'sidebar workspace inspector' minmax(var(--workspace-min-height), 1fr)
      'dock dock dock' auto
      'player player player' auto
      / var(--sidebar-column) minmax(var(--workspace-min-width), 1fr) var(--inspector-column);
    width: 100vw;
    height: 100vh;
    overflow: hidden;
    background: var(--color-background);
    color: var(--color-text);
  }

  @media (width < 1360px) {
    .app-shell {
      --sidebar-state: collapsed;
      --sidebar-column: var(--sidebar-collapsed-width);
    }
  }

  @media (width < 1280px) {
    .app-shell {
      --inspector-mode: sheet;
      --inspector-column: 0px;
    }

    .inspector-dock {
      display: none;
    }
  }

  @media (height < 820px) {
    .app-shell {
      --dock-state: collapsed;
    }
  }

  .app-shell[data-sidebar='collapsed'] {
    --sidebar-state: collapsed;
    --sidebar-column: var(--sidebar-collapsed-width);
  }

  .app-shell[data-sidebar='expanded'] {
    --sidebar-state: expanded;
    --sidebar-column: var(--sidebar-width);
  }

  .app-shell[data-dock='collapsed'] {
    --dock-state: collapsed;
  }

  .app-shell[data-dock='expanded'] {
    --dock-state: expanded;
  }

  .app-shell[data-inspector='hidden'] {
    --inspector-column: 0px;
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
    display: grid;
    grid-area: workspace;
    grid-template-rows: auto minmax(0, 1fr);
    grid-template-columns: minmax(0, 1fr);
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
