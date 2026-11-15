<script lang="ts">
  import { tick, untrack } from 'svelte';

  import Button from '../design/Button.svelte';
  import Icon from '../design/Icon.svelte';
  import type { LibraryCollection } from '../ipc/bindings/LibraryCollection';
  import type { LibraryTrack } from '../ipc/bindings/LibraryTrack';

  interface Props {
    open: boolean;
    anchorEl: HTMLElement | null;
    label?: string;
    collections?: LibraryCollection[];
    selectedTracks?: LibraryTrack[];
    activeCollection?: LibraryCollection | null;
    showFavourite?: boolean;
    onToggleFavourite?: (track: LibraryTrack) => void;
    onAdd?: (collectionId: number, trackIds: number[]) => Promise<void> | void;
    onRemove?: (collectionId: number, trackIds: number[]) => Promise<void> | void;
    onCreateAndAdd?: (name: string, trackIds: number[]) => Promise<void> | void;
    onClose: () => void;
  }

  let {
    open,
    anchorEl,
    label = 'Track actions',
    collections = [],
    selectedTracks = [],
    activeCollection = null,
    showFavourite = false,
    onToggleFavourite,
    onAdd,
    onRemove,
    onCreateAndAdd,
    onClose,
  }: Props = $props();

  let menuEl = $state<HTMLDivElement | undefined>();
  let nameInput = $state<HTMLInputElement | undefined>();
  let newCollectionBtn = $state<HTMLButtonElement | undefined>();
  let creating = $state(false);
  let newName = $state('');
  let errorMsg = $state<string | null>(null);
  let busy = $state(false);
  let pos = $state({ top: 0, left: 0 });

  const targetIds = $derived(selectedTracks.map((track) => track.id));
  const singleTrack = $derived(selectedTracks.length === 1 ? selectedTracks[0] : undefined);
  const canRemoveActive = $derived(
    activeCollection != null &&
      selectedTracks.some((track) =>
        track.collections.some((col) => col.id === activeCollection.id),
      ),
  );

  function membershipState(collectionId: number): 'true' | 'mixed' | 'false' {
    if (selectedTracks.length === 0) return 'false';
    let hits = 0;
    for (const track of selectedTracks) {
      if (track.collections.some((col) => col.id === collectionId)) hits += 1;
    }
    if (hits === selectedTracks.length) return 'true';
    if (hits > 0) return 'mixed';
    return 'false';
  }

  function updatePosition() {
    if (!menuEl) return;
    const margin = 8;
    const rect = anchorEl?.getBoundingClientRect();
    const width = menuEl.offsetWidth || 220;
    const height = menuEl.offsetHeight || 200;
    const vw = window.innerWidth || 1280;
    const vh = window.innerHeight || 800;

    if (!rect || (rect.width === 0 && rect.height === 0 && rect.top === 0 && rect.left === 0)) {
      pos = { top: margin, left: margin };
      return;
    }

    const preferRight = rect.right - width;
    const left = Math.max(margin, Math.min(vw - width - margin, Math.max(margin, preferRight)));
    const fitsBelow = rect.bottom + 4 + height <= vh - margin;
    const top = fitsBelow
      ? Math.max(margin, rect.bottom + 4)
      : Math.max(margin, Math.min(vh - height - margin, rect.top - height - 4));
    pos = { top, left };
  }

  $effect(() => {
    if (!open || !menuEl) {
      untrack(() => {
        creating = false;
        newName = '';
        errorMsg = null;
        busy = false;
      });
      return;
    }

    try {
      menuEl.showPopover?.();
    } catch {
      // Fallback safely when running under environments without Popover API support.
    }
    updatePosition();
    untrack(() => {
      if (menuEl && !menuEl.contains(document.activeElement)) {
        menuEl
          .querySelector<HTMLElement>(
            '[role="menuitem"]:not(:disabled), [role="menuitemcheckbox"]:not(:disabled)',
          )
          ?.focus();
      }
    });

    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node | null;
      if (!target) return;
      if (menuEl?.contains(target) || anchorEl?.contains(target)) return;
      onClose();
    };
    const onWindowKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        if (creating) {
          creating = false;
          newName = '';
          errorMsg = null;
          void tick().then(() => newCollectionBtn?.focus());
        } else {
          onClose();
        }
        return;
      }

      if (event.altKey || event.ctrlKey || event.metaKey) return;
      const inInput = document.activeElement instanceof HTMLInputElement;
      if (inInput && (event.key === 'Home' || event.key === 'End')) return;
      if (
        event.key !== 'ArrowDown' &&
        event.key !== 'ArrowUp' &&
        event.key !== 'Home' &&
        event.key !== 'End'
      ) {
        return;
      }
      const items = Array.from(
        menuEl?.querySelectorAll<HTMLElement>(
          '[role="menuitem"]:not(:disabled), [role="menuitemcheckbox"]:not(:disabled)',
        ) ?? [],
      );
      if (items.length === 0) return;
      event.preventDefault();
      event.stopPropagation();
      const current =
        document.activeElement instanceof HTMLElement ? items.indexOf(document.activeElement) : -1;
      let next: number;
      if (event.key === 'Home') {
        next = 0;
      } else if (event.key === 'End') {
        next = items.length - 1;
      } else if (event.key === 'ArrowDown') {
        next = current < 0 ? 0 : (current + 1) % items.length;
      } else {
        next = current <= 0 ? items.length - 1 : current - 1;
      }
      items[next]?.focus();
      items[next]?.scrollIntoView?.({ block: 'nearest' });
    };

    window.addEventListener('pointerdown', onPointerDown, true);
    window.addEventListener('keydown', onWindowKeyDown, true);
    return () => {
      window.removeEventListener('pointerdown', onPointerDown, true);
      window.removeEventListener('keydown', onWindowKeyDown, true);
      try {
        if (menuEl?.matches?.(':popover-open')) menuEl.hidePopover?.();
      } catch {
        // Ignore popover teardown errors on detached elements.
      }
    };
  });

  $effect(() => {
    if (creating && nameInput) {
      nameInput.focus();
    }
  });

  async function toggleCollection(collectionId: number) {
    if (busy || targetIds.length === 0) return;
    busy = true;
    errorMsg = null;
    try {
      if (membershipState(collectionId) === 'true') {
        await onRemove?.(collectionId, targetIds);
      } else {
        await onAdd?.(collectionId, targetIds);
      }
      onClose();
    } catch (error) {
      errorMsg = error instanceof Error ? error.message : String(error);
    } finally {
      busy = false;
    }
  }

  async function removeFromActive() {
    if (busy || !activeCollection || targetIds.length === 0) return;
    busy = true;
    errorMsg = null;
    try {
      await onRemove?.(activeCollection.id, targetIds);
      onClose();
    } catch (error) {
      errorMsg = error instanceof Error ? error.message : String(error);
    } finally {
      busy = false;
    }
  }

  async function submitCreate(event: SubmitEvent) {
    event.preventDefault();
    if (busy || targetIds.length === 0) return;
    busy = true;
    errorMsg = null;
    try {
      await onCreateAndAdd?.(newName, targetIds);
      onClose();
    } catch (error) {
      errorMsg = error instanceof Error ? error.message : String(error);
    } finally {
      busy = false;
    }
  }
</script>

{#if open}
  <div
    bind:this={menuEl}
    class="menu"
    role="menu"
    aria-label={label}
    popover="manual"
    style:top="{pos.top}px"
    style:left="{pos.left}px"
  >
    {#if showFavourite && singleTrack && onToggleFavourite}
      <button
        type="button"
        class="item"
        role="menuitem"
        onclick={() => {
          onToggleFavourite(singleTrack);
          onClose();
        }}
      >
        <Icon name="star" size="xs" filled={singleTrack.favourite} />
        <span class="item-label">
          {singleTrack.favourite ? 'Remove from favourites' : 'Add to favourites'}
        </span>
      </button>
    {/if}

    {#if canRemoveActive && activeCollection && onRemove}
      <button type="button" class="item" role="menuitem" disabled={busy} onclick={removeFromActive}>
        <Icon name="close" size="xs" />
        <span class="item-label">Remove from {activeCollection.name}</span>
      </button>
    {/if}

    {#if (showFavourite && singleTrack && onToggleFavourite) || (canRemoveActive && activeCollection)}
      <div class="sep" role="separator"></div>
    {/if}

    <div class="section-head">Collections</div>

    {#if collections.length > 0}
      <div class="list">
        {#each collections as col (col.id)}
          {@const checked = membershipState(col.id)}
          <button
            type="button"
            class="item"
            role="menuitemcheckbox"
            aria-checked={checked}
            disabled={busy}
            onclick={() => toggleCollection(col.id)}
          >
            <span
              class={['box', { checked: checked === 'true', mixed: checked === 'mixed' }]}
              aria-hidden="true"
            >
              {#if checked === 'true'}
                <Icon name="check" size="xs" />
              {:else if checked === 'mixed'}
                <span class="dash"></span>
              {/if}
            </span>
            <span class="item-label">{col.name}</span>
            <span class="item-count numeric">{col.track_count.toLocaleString('en-GB')}</span>
          </button>
        {/each}
      </div>
    {:else}
      <p class="empty">No collections yet.</p>
    {/if}

    {#if onCreateAndAdd}
      <div class="sep" role="separator"></div>
      {#if creating}
        <form class="create-form" onsubmit={submitCreate}>
          <input
            bind:this={nameInput}
            type="text"
            class="create-input"
            aria-label="New collection name"
            placeholder="Collection name"
            bind:value={newName}
            disabled={busy}
          />
          <Button type="submit" variant="primary" size="sm" disabled={busy}>Create</Button>
        </form>
      {:else}
        <button
          bind:this={newCollectionBtn}
          type="button"
          class="item"
          role="menuitem"
          onclick={() => {
            creating = true;
            errorMsg = null;
          }}
        >
          <Icon name="plus" size="xs" />
          <span class="item-label">New collection…</span>
        </button>
      {/if}
    {/if}

    {#if errorMsg}
      <p class="error" role="alert">{errorMsg}</p>
    {/if}
  </div>
{/if}

<style>
  .menu {
    position: fixed;
    inset: auto;
    display: grid;
    gap: var(--space-0-5);
    min-width: calc(var(--sidebar-width) - var(--space-8));
    max-width: var(--sidebar-width);
    margin: 0;
    padding: var(--space-1);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-md);
    background: var(--color-raised);
    color: var(--color-text);
  }

  .section-head {
    padding: var(--space-1) var(--space-2) var(--space-0-5);
    color: var(--color-muted);
    font-family: var(--font-mono);
    font-size: var(--text-2xs);
    letter-spacing: var(--tracking-mono);
    text-transform: uppercase;
  }

  .list {
    display: grid;
    gap: var(--space-0-5);
    max-height: calc(var(--sidebar-collection-height) * 7);
    overflow-y: auto;
  }

  .item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    min-height: var(--control-height);
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--color-text);
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    text-align: left;
    cursor: pointer;
  }

  .item:hover:not(:disabled),
  .item:focus-visible:not(:disabled) {
    background: var(--color-surface-2);
  }

  .item-label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item-count {
    color: var(--color-muted);
    font-size: var(--text-xs);
  }

  .box {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    width: var(--checkbox-size);
    height: var(--checkbox-size);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-xs);
    color: var(--color-background);
  }

  .box.checked,
  .box.mixed {
    border-color: var(--color-text);
    background: var(--color-text);
  }

  .dash {
    width: var(--space-2);
    height: var(--selection-bar-width);
    background: var(--color-background);
  }

  .sep {
    height: var(--border-width);
    margin: var(--space-0-5) 0;
    background: var(--color-border);
  }

  .empty {
    margin: 0;
    padding: var(--space-1) var(--space-2);
    color: var(--color-muted);
    font-size: var(--text-sm);
  }

  .create-form {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: var(--space-1-5);
    padding: var(--space-1);
  }

  .create-input {
    min-width: 0;
    height: var(--control-height-sm);
    padding: 0 var(--space-2);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-text);
    font-family: var(--font-sans);
    font-size: var(--text-sm);
  }

  .error {
    margin: var(--space-0-5) var(--space-1);
    padding-left: var(--space-2);
    border-left: var(--selection-bar-width) solid var(--color-accent);
    color: var(--color-text);
    font-size: var(--text-xs);
  }

  @media (forced-colors: active) {
    .box.checked,
    .box.mixed {
      border-color: Highlight;
      background: Highlight;
      color: HighlightText;
      forced-color-adjust: none;
    }
  }
</style>
