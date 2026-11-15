<script lang="ts" module>
  import type { IconName } from '../design/Icon.svelte';

  export type SpaceName = 'Library' | 'Prep' | 'Sets' | 'Mix' | 'Replay';

  export interface Space {
    name: SpaceName;
    icon: IconName;
    /** The milestone that builds it. Missing once the space exists. */
    arrives?: string;
  }

  /** In sidebar order. The number keys 1 to 5 follow it. */
  export const spaces: Space[] = [
    { name: 'Library', icon: 'library' },
    { name: 'Prep', icon: 'prep', arrives: 'M3' },
    { name: 'Sets', icon: 'sets', arrives: 'M10' },
    { name: 'Mix', icon: 'mix' },
    { name: 'Replay', icon: 'replay', arrives: 'M9' },
  ];
</script>

<script lang="ts">
  import { untrack } from 'svelte';

  import Button from '../design/Button.svelte';
  import Icon from '../design/Icon.svelte';
  import ModularDivider from '../design/ModularDivider.svelte';
  import MonoLabel from '../design/MonoLabel.svelte';
  import Sheet from '../design/Sheet.svelte';
  import Tooltip from '../design/Tooltip.svelte';
  import type { LibraryCollection } from '../ipc/bindings/LibraryCollection';
  import Wave from '../wave/Wave.svelte';

  interface Props {
    activeSpace: SpaceName;
    /** Missing until the library has been read. */
    trackCount: number | null;
    /** Pauses the ambient dot wave while Mix is playing. */
    wavePaused?: boolean;
    collections?: LibraryCollection[];
    activeCollectionId?: number | null;
    onSelectSpace: (space: SpaceName) => void;
    onSelectCollection?: (collectionId: number | null) => void;
    onCreateCollection?: (name: string) => Promise<void> | void;
    onRenameCollection?: (collectionId: number, name: string) => Promise<void> | void;
    onDeleteCollection?: (collectionId: number) => Promise<void> | void;
    onCollapse: (collapsed: boolean) => void;
  }

  let {
    activeSpace,
    trackCount,
    wavePaused = false,
    collections = [],
    activeCollectionId = null,
    onSelectSpace,
    onSelectCollection,
    onCreateCollection,
    onRenameCollection,
    onDeleteCollection,
    onCollapse,
  }: Props = $props();

  let creating = $state(false);
  let createName = $state('');
  let createError = $state<string | null>(null);
  let createInput = $state<HTMLInputElement | undefined>();

  let editingId = $state<number | null>(null);
  let renameValue = $state('');
  let renameError = $state<string | null>(null);
  let renameInput = $state<HTMLInputElement | undefined>();

  let menuCollectionId = $state<number | null>(null);
  let menuAnchor = $state<HTMLElement | null>(null);
  let rowMenuEl = $state<HTMLDivElement | undefined>();
  let rowMenuPos = $state({ top: 0, left: 0 });

  let deleteOpen = $state(false);
  let deleteTarget = $state<LibraryCollection | null>(null);
  let busy = $state(false);

  const activeCollection = $derived(
    collections.find((col) => col.id === activeCollectionId) ?? null,
  );
  const menuCollection = $derived(
    menuCollectionId != null
      ? (collections.find((col) => col.id === menuCollectionId) ?? null)
      : null,
  );

  // Only the first space still to come takes focus, so Tab does not stop on every placeholder.
  const firstToCome = spaces.find((space) => space.arrives)?.name;

  $effect(() => {
    if (creating && createInput) createInput.focus();
  });

  $effect(() => {
    if (editingId !== null && renameInput) {
      renameInput.focus();
      renameInput.select();
    }
  });

  $effect(() => {
    if (!menuCollection || !rowMenuEl) return;
    try {
      rowMenuEl.showPopover?.();
    } catch {
      // Safe fallback when Popover API is unavailable in test DOMs.
    }
    const rect = menuAnchor?.getBoundingClientRect();
    const width = rowMenuEl.offsetWidth || 130;
    const height = rowMenuEl.offsetHeight || 72;
    const vw = window.innerWidth || 1280;
    const vh = window.innerHeight || 800;
    if (rect && (rect.width > 0 || rect.height > 0)) {
      rowMenuPos = {
        left: Math.max(8, Math.min(vw - width - 8, rect.right - width)),
        top: Math.max(8, Math.min(vh - height - 8, rect.bottom + 4)),
      };
    } else {
      rowMenuPos = { top: 8, left: 8 };
    }

    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node | null;
      if (!target) return;
      if (rowMenuEl?.contains(target) || menuAnchor?.contains(target)) return;
      menuCollectionId = null;
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        menuCollectionId = null;
      }
    };
    window.addEventListener('pointerdown', onPointerDown, true);
    window.addEventListener('keydown', onKeyDown, true);
    return () => {
      window.removeEventListener('pointerdown', onPointerDown, true);
      window.removeEventListener('keydown', onKeyDown, true);
      try {
        if (rowMenuEl?.matches?.(':popover-open')) rowMenuEl.hidePopover?.();
      } catch {
        // Ignore teardown issues on detached popovers.
      }
    };
  });

  function startCreate() {
    menuCollectionId = null;
    editingId = null;
    renameError = null;
    creating = true;
    createName = '';
    createError = null;
  }

  async function submitCreate(event: SubmitEvent) {
    event.preventDefault();
    if (busy || !onCreateCollection) return;
    busy = true;
    createError = null;
    try {
      await onCreateCollection(createName);
      untrack(() => {
        creating = false;
        createName = '';
      });
    } catch (error) {
      createError = error instanceof Error ? error.message : String(error);
    } finally {
      busy = false;
    }
  }

  function startRename(col: LibraryCollection) {
    menuCollectionId = null;
    creating = false;
    createError = null;
    editingId = col.id;
    renameValue = col.name;
    renameError = null;
  }

  async function submitRename(event: SubmitEvent) {
    event.preventDefault();
    if (busy || editingId === null || !onRenameCollection) return;
    busy = true;
    renameError = null;
    const targetId = editingId;
    try {
      await onRenameCollection(targetId, renameValue);
      untrack(() => {
        editingId = null;
        renameValue = '';
      });
    } catch (error) {
      renameError = error instanceof Error ? error.message : String(error);
    } finally {
      busy = false;
    }
  }

  function askDelete(col: LibraryCollection) {
    menuCollectionId = null;
    deleteTarget = col;
    deleteOpen = true;
  }

  async function confirmDelete() {
    if (busy || !deleteTarget || !onDeleteCollection) return;
    busy = true;
    try {
      await onDeleteCollection(deleteTarget.id);
      deleteOpen = false;
      deleteTarget = null;
    } finally {
      busy = false;
    }
  }
</script>

{#snippet spaceButton(space: Space, index: number)}
  {@const tooltipText = space.arrives
    ? `${space.name} arrives in ${space.arrives}`
    : space.name === 'Library' && activeCollection
      ? `Library · ${activeCollection.name}`
      : space.name}
  {@const subActive = space.name === 'Library' && activeCollectionId != null}
  <Tooltip text={tooltipText} shortcut={space.arrives ? undefined : String(index + 1)} side="right">
    <button
      type="button"
      class={['space', { 'sub-active': subActive }]}
      aria-label={space.name}
      aria-current={space.name === activeSpace ? 'page' : undefined}
      aria-disabled={space.arrives ? 'true' : undefined}
      tabindex={space.arrives && space.name !== firstToCome ? -1 : undefined}
      onclick={() => {
        if (!space.arrives) {
          if (space.name === 'Library') onSelectCollection?.(null);
          onSelectSpace(space.name);
        }
      }}
    >
      <Icon name={space.icon} size="sm" />
      <span class="label">{space.name}</span>
      {#if space.name === 'Library' && trackCount !== null}
        <span class="count numeric">{trackCount.toLocaleString('en-GB')}</span>
      {/if}
    </button>
  </Tooltip>
{/snippet}

<nav class="sidebar" aria-label="Spaces">
  <div class="head">
    <span class="tagline"><MonoLabel text="Music intelligence&#10;for DJs" tone="secondary" /></span
    >
    <span class="collapse">
      <Tooltip text="Collapse the sidebar" shortcut="[" side="right">
        <Button variant="icon" aria-label="Collapse the sidebar" onclick={() => onCollapse(true)}>
          <Icon name="sidebar-collapse" size="sm" />
        </Button>
      </Tooltip>
    </span>
    <span class="expand">
      <Tooltip text="Expand the sidebar" shortcut="[" side="right">
        <Button variant="icon" aria-label="Expand the sidebar" onclick={() => onCollapse(false)}>
          <Icon name="sidebar-collapse" size="sm" />
        </Button>
      </Tooltip>
    </span>
  </div>

  <div class="spaces">
    {#each spaces as space, index (space.name)}
      {@render spaceButton(space, index)}
      {#if space.name === 'Library'}
        <div class="collections">
          <div class="collections-head">
            <Icon name="collections" size="sm" />
            <span class="label">Collections</span>
            <Tooltip text="New collection" side="right">
              <Button variant="icon" size="sm" aria-label="New collection" onclick={startCreate}>
                <Icon name="plus" size="xs" />
              </Button>
            </Tooltip>
          </div>
          {#if creating}
            <form class="collection-form" onsubmit={submitCreate}>
              <input
                bind:this={createInput}
                type="text"
                class="collection-input"
                aria-label="New collection name"
                placeholder="Collection name"
                bind:value={createName}
                disabled={busy}
                onkeydown={(event) => {
                  if (event.key === 'Escape') {
                    event.preventDefault();
                    creating = false;
                    createName = '';
                    createError = null;
                  }
                }}
              />
              {#if createError}
                <p class="collection-error" role="alert">{createError}</p>
              {/if}
            </form>
          {/if}
          {#if collections.length > 0}
            <div class="collection-list" role="list">
              {#each collections as col (col.id)}
                <div class="collection-entry" role="listitem">
                  {#if editingId === col.id}
                    <form class="collection-form" onsubmit={submitRename}>
                      <input
                        bind:this={renameInput}
                        type="text"
                        class="collection-input"
                        aria-label="Rename collection"
                        bind:value={renameValue}
                        disabled={busy}
                        onkeydown={(event) => {
                          if (event.key === 'Escape') {
                            event.preventDefault();
                            editingId = null;
                            renameError = null;
                          }
                        }}
                      />
                      {#if renameError}
                        <p class="collection-error" role="alert">{renameError}</p>
                      {/if}
                    </form>
                  {:else}
                    <div
                      class={[
                        'collection-row',
                        { active: activeSpace === 'Library' && col.id === activeCollectionId },
                      ]}
                    >
                      <button
                        type="button"
                        class="collection-btn"
                        aria-current={activeSpace === 'Library' && col.id === activeCollectionId
                          ? 'true'
                          : undefined}
                        onclick={() => {
                          onSelectSpace('Library');
                          onSelectCollection?.(col.id);
                        }}
                      >
                        <span class="label">{col.name}</span>
                        <span class="count numeric">{col.track_count.toLocaleString('en-GB')}</span>
                      </button>
                      <button
                        type="button"
                        class="collection-more"
                        aria-label="Actions for {col.name}"
                        aria-haspopup="menu"
                        aria-expanded={menuCollectionId === col.id}
                        onclick={(event) => {
                          event.stopPropagation();
                          if (menuCollectionId === col.id) {
                            menuCollectionId = null;
                          } else {
                            menuAnchor = event.currentTarget;
                            menuCollectionId = col.id;
                          }
                        }}
                      >
                        <Icon name="more" size="xs" />
                      </button>
                    </div>
                  {/if}
                </div>
              {/each}
            </div>
          {:else if !creating}
            <p class="waiting">No collections yet.</p>
          {/if}
        </div>
      {/if}
    {/each}
  </div>

  <div class="foot">
    <div class="active-set">
      <ModularDivider />
      <MonoLabel text="Active set" tone="secondary" />
      <p class="waiting">No set yet. Sets arrive in M10.</p>
    </div>
    <Wave variant="sidebar" paused={wavePaused} />
  </div>
</nav>

{#if menuCollection}
  <div
    bind:this={rowMenuEl}
    class="row-menu"
    role="menu"
    aria-label="Collection actions"
    popover="manual"
    style:top="{rowMenuPos.top}px"
    style:left="{rowMenuPos.left}px"
  >
    <button
      type="button"
      class="row-menu-item"
      role="menuitem"
      onclick={() => startRename(menuCollection)}
    >
      Rename
    </button>
    <button
      type="button"
      class="row-menu-item"
      role="menuitem"
      onclick={() => askDelete(menuCollection)}
    >
      Delete
    </button>
  </div>
{/if}

<Sheet bind:open={deleteOpen} title="Delete collection">
  <p class="confirm-copy">
    Delete "{deleteTarget?.name}"? Tracks and files on disk will stay in your library.
  </p>
  {#snippet actions()}
    <Button onclick={() => (deleteOpen = false)} disabled={busy}>Cancel</Button>
    <Button variant="danger" onclick={confirmDelete} disabled={busy}>Delete collection</Button>
  {/snippet}
</Sheet>

<style>
  .sidebar {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    height: 100%;
    min-height: 0;
    overflow: hidden;
    border-right: var(--border-width) solid var(--color-border);
    background: var(--color-surface);
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding: var(--space-5) var(--space-3) var(--space-4) var(--space-6);
  }

  .expand {
    display: none;
  }

  .spaces {
    display: grid;
    align-content: start;
    gap: var(--space-1);
    padding: 0 var(--space-3);
    overflow: hidden auto;
  }

  .spaces > :global(.anchor) {
    display: flex;
  }

  .space {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--space-5);
    width: 100%;
    height: var(--control-height-nav);
    padding: 0 var(--space-4);
    border: var(--border-width) solid transparent;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--color-text-2);
    font-family: var(--font-sans);
    font-size: var(--text-base);
    font-weight: var(--weight-medium);
    text-align: left;
    cursor: pointer;
    transition:
      background-color var(--duration-fast) var(--ease-out),
      color var(--duration-fast) var(--ease-out);
  }

  .space:hover:not([aria-disabled='true']) {
    color: var(--color-text);
  }

  .space[aria-current='page'] {
    border-color: var(--color-border);
    background: var(--color-raised);
    color: var(--color-text);
  }

  .space[aria-current='page']::before {
    position: absolute;
    top: var(--space-2);
    bottom: var(--space-2);
    left: calc(-1 * var(--border-width));
    width: var(--selection-bar-width);
    background: var(--color-accent);
    content: '';
  }

  .space[aria-disabled='true'] {
    color: var(--color-muted);
    cursor: default;
  }

  .space.sub-active[aria-current='page'] {
    background: transparent;
    color: var(--color-text-2);
    font-weight: var(--weight-medium);
  }

  .space.sub-active[aria-current='page']::before {
    display: none;
  }

  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .count {
    color: var(--color-text-2);
    font-size: var(--text-sm);
    font-weight: var(--weight-regular);
  }

  .collections {
    display: grid;
    gap: var(--space-1);
    min-width: 0;
    padding: var(--space-3) 0 var(--space-4) var(--space-4);
  }

  .collections-head {
    display: flex;
    align-items: center;
    gap: var(--space-5);
    min-width: 0;
    color: var(--color-text-2);
    font-size: var(--text-base);
  }

  .collection-list {
    display: grid;
    gap: var(--space-0-5);
    min-width: 0;
  }

  .collection-entry {
    min-width: 0;
  }

  .collection-row {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--space-1);
    min-width: 0;
    height: var(--sidebar-collection-height);
    padding: 0 var(--space-1-5) 0 var(--sidebar-collection-indent);
    border-radius: var(--radius-sm);
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }

  .collection-row:hover {
    background: var(--color-surface-2);
    color: var(--color-text);
  }

  .collection-row.active {
    background: var(--color-surface-2);
    color: var(--color-text);
    font-weight: var(--weight-medium);
  }

  .collection-row.active::before {
    position: absolute;
    top: var(--space-1);
    bottom: var(--space-1);
    left: calc(-1 * var(--border-width));
    width: var(--selection-bar-width);
    background: var(--color-accent);
    content: '';
  }

  .collection-btn {
    display: flex;
    flex: 1;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
    height: 100%;
    padding: 0;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .collection-btn .count {
    color: var(--color-muted);
    font-size: var(--text-sm);
  }

  .collection-more {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    width: var(--control-height-xs);
    height: var(--control-height-xs);
    padding: 0;
    border: 0;
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--color-muted);
    opacity: 0;
    cursor: pointer;
    transition:
      opacity var(--duration-fast) var(--ease-out),
      color var(--duration-fast) var(--ease-out);
  }

  .collection-row:hover .collection-more,
  .collection-row:focus-within .collection-more,
  .collection-more[aria-expanded='true'] {
    opacity: 1;
  }

  .collection-more:hover {
    color: var(--color-text);
  }

  .collection-form {
    display: grid;
    gap: var(--space-1);
    min-width: 0;
    padding: var(--space-0-5) var(--space-1) var(--space-0-5) var(--sidebar-collection-indent);
  }

  .collection-input {
    box-sizing: border-box;
    width: 100%;
    min-width: 0;
    height: var(--sidebar-collection-height);
    padding: 0 var(--space-2);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-sm);
    background: var(--color-background);
    color: var(--color-text);
    font-family: var(--font-sans);
    font-size: var(--text-sm);
  }

  .collection-error {
    margin: 0;
    padding-left: var(--space-2);
    border-left: var(--selection-bar-width) solid var(--color-accent);
    color: var(--color-text);
    font-size: var(--text-xs);
  }

  .row-menu {
    position: fixed;
    inset: auto;
    display: grid;
    gap: var(--space-0-5);
    margin: 0;
    padding: var(--space-1);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-md);
    background: var(--color-raised);
    color: var(--color-text);
  }

  .row-menu-item {
    display: flex;
    align-items: center;
    min-width: calc(var(--sidebar-width) / 2);
    height: var(--control-height);
    padding: 0 var(--space-2-5);
    border: 0;
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--color-text);
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    text-align: left;
    cursor: pointer;
  }

  .row-menu-item:hover,
  .row-menu-item:focus-visible {
    background: var(--color-surface-2);
    outline: none;
  }

  .confirm-copy {
    margin: 0;
    color: var(--color-text-2);
    font-size: var(--text-base);
    line-height: var(--leading-normal);
  }

  .waiting {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-sm);
  }

  .collections .waiting {
    padding-left: var(--sidebar-collection-indent);
  }

  .foot {
    display: grid;
    gap: var(--space-4);
  }

  .active-set {
    display: grid;
    gap: var(--space-3);
    padding: 0 var(--space-5);
  }

  @media (forced-colors: active) {
    .space[aria-disabled='true'] {
      color: GrayText;
    }
  }

  /* The shell sets --sidebar-state from the window size, or from the user's own choice. */
  @container style(--sidebar-state: collapsed) {
    .tagline,
    .collapse,
    .label,
    .count,
    .collections,
    .foot {
      display: none;
    }

    .expand {
      display: inline-flex;
    }

    .head {
      justify-content: center;
      padding: var(--space-3) 0;
    }

    .spaces {
      justify-items: center;
      padding: 0;
    }

    .space {
      justify-content: center;
      width: var(--control-height-nav);
      padding: 0;
    }
  }
</style>
