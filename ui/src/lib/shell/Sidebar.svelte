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
  import Button from '../design/Button.svelte';
  import Icon from '../design/Icon.svelte';
  import ModularDivider from '../design/ModularDivider.svelte';
  import MonoLabel from '../design/MonoLabel.svelte';
  import Tooltip from '../design/Tooltip.svelte';
  import Wave from '../wave/Wave.svelte';

  interface Props {
    activeSpace: SpaceName;
    collapsed: boolean;
    /** Missing until the library has been read. */
    trackCount: number | null;
    onSelectSpace: (space: SpaceName) => void;
    onToggleCollapse: () => void;
  }

  let { activeSpace, collapsed, trackCount, onSelectSpace, onToggleCollapse }: Props = $props();

  const toggleLabel = $derived(collapsed ? 'Expand the sidebar' : 'Collapse the sidebar');
</script>

{#snippet spaceButton(space: Space, index: number)}
  {@const active = space.name === activeSpace}
  <Tooltip
    text={space.arrives ? `${space.name} arrives in ${space.arrives}` : space.name}
    shortcut={space.arrives ? undefined : String(index + 1)}
    side="right"
  >
    <button
      type="button"
      class="space"
      aria-label={space.name}
      aria-current={active ? 'page' : undefined}
      aria-disabled={space.arrives ? 'true' : undefined}
      onclick={() => {
        if (!space.arrives) onSelectSpace(space.name);
      }}
    >
      <Icon name={space.icon} size="sm" />
      {#if !collapsed}
        <span class="label">{space.name}</span>
        {#if space.name === 'Library' && trackCount !== null}
          <span class="count numeric">{trackCount.toLocaleString('en-GB')}</span>
        {/if}
      {/if}
    </button>
  </Tooltip>
{/snippet}

<nav class={['sidebar', { collapsed }]} aria-label="Spaces">
  <div class="head">
    {#if !collapsed}
      <MonoLabel text="Music intelligence&#10;for DJs" tone="secondary" />
    {/if}
    <Tooltip text={toggleLabel} shortcut="[" side="right">
      <Button variant="icon" aria-label={toggleLabel} onclick={onToggleCollapse}>
        <Icon name="sidebar-collapse" size="sm" />
      </Button>
    </Tooltip>
  </div>

  <div class="spaces">
    {#each spaces as space, index (space.name)}
      {@render spaceButton(space, index)}
      {#if space.name === 'Library' && !collapsed}
        <div class="collections">
          <div class="collections-head">
            <Icon name="collections" size="sm" />
            <span class="label">Collections</span>
            <Tooltip text="Collections arrive in M2" side="right">
              <Button variant="icon" size="sm" aria-label="New collection" aria-disabled="true">
                <Icon name="plus" size="xs" />
              </Button>
            </Tooltip>
          </div>
          <p class="waiting">None yet. Collections arrive in M2.</p>
        </div>
      {/if}
    {/each}
  </div>

  {#if !collapsed}
    <div class="foot">
      <div class="active-set">
        <ModularDivider />
        <MonoLabel text="Active set" tone="secondary" />
        <p class="waiting">No set yet. Sets arrive in M10.</p>
      </div>
      <Wave variant="sidebar" />
    </div>
  {/if}
</nav>

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

  .collapsed .head {
    justify-content: center;
    padding: var(--space-3) 0;
  }

  .spaces {
    display: grid;
    align-content: start;
    gap: var(--space-1);
    padding: 0 var(--space-3);
    overflow-y: auto;
  }

  .collapsed .spaces {
    justify-items: center;
    padding: 0;
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

  .collapsed .space {
    justify-content: center;
    width: var(--control-height-nav);
    padding: 0;
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
    padding: var(--space-3) 0 var(--space-4) var(--space-4);
  }

  .collections-head {
    display: flex;
    align-items: center;
    gap: var(--space-5);
    color: var(--color-text-2);
    font-size: var(--text-base);
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
</style>
