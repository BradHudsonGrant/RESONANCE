<script lang="ts">
  import type { LucideIcon } from '@lucide/svelte';
  import Activity from '@lucide/svelte/icons/activity';
  import ChartLine from '@lucide/svelte/icons/chart-line';
  import Disc3 from '@lucide/svelte/icons/disc-3';
  import Inbox from '@lucide/svelte/icons/inbox';
  import Library from '@lucide/svelte/icons/library';
  import ListMusic from '@lucide/svelte/icons/list-music';
  import Settings from '@lucide/svelte/icons/settings';

  import Icon from '../design/Icon.svelte';
  import Mark from '../design/Mark.svelte';
  import Tooltip from '../design/Tooltip.svelte';

  interface Space {
    name: string;
    icon: LucideIcon;
    built: boolean;
  }

  let { current, onselect }: { current: string; onselect: (space: string) => void } = $props();

  // Library and Mix exist so far. The others are shown so the shape of the app is clear, and say
  // they are not built rather than opening an empty page.
  const spaces: Space[] = [
    { name: 'Library', icon: Library, built: true },
    { name: 'Inbox', icon: Inbox, built: false },
    { name: 'Sets', icon: ListMusic, built: false },
    { name: 'Mix', icon: Disc3, built: true },
    { name: 'Review', icon: ChartLine, built: false },
  ];
  const tools: Space[] = [
    { name: 'Activity', icon: Activity, built: false },
    { name: 'Settings', icon: Settings, built: false },
  ];
</script>

{#snippet item(space: Space)}
  <Tooltip
    text={space.built ? space.name : `${space.name} is not in this version yet`}
    side="right"
  >
    <button
      type="button"
      class="item"
      aria-label={space.name}
      aria-current={space.name === current ? 'page' : undefined}
      aria-disabled={!space.built}
      onclick={() => space.built && onselect(space.name)}
    >
      <Icon icon={space.icon} size={18} />
    </button>
  </Tooltip>
{/snippet}

<nav class="rail" aria-label="Spaces">
  <span class="mark"><Mark /></span>
  <div class="group">
    {#each spaces as space (space.name)}
      {@render item(space)}
    {/each}
  </div>
  <div class="group">
    {#each tools as space (space.name)}
      {@render item(space)}
    {/each}
  </div>
</nav>

<style>
  .rail {
    display: grid;
    grid-template-rows: auto 1fr auto;
    justify-items: center;
    gap: var(--space-4);
    width: 52px;
    padding: var(--space-3) 0;
    border-right: 1px solid var(--color-border);
    background: var(--color-surface);
  }

  .mark {
    display: inline-flex;
    width: 22px;
    color: var(--color-text);
  }

  .group {
    display: grid;
    align-content: start;
    gap: var(--space-1);
  }

  .item {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border: 0;
    /* The selection bar of a table row, turned on its side. */
    border-left: 2px solid transparent;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--color-text-2);
    cursor: pointer;
  }

  .item:hover {
    background: var(--color-surface-2);
    color: var(--color-text);
  }

  .item[aria-current='page'] {
    border-left-color: var(--color-accent);
    background: var(--color-raised);
    color: var(--color-text);
  }

  .item[aria-disabled='true'] {
    color: var(--color-muted);
    cursor: default;
  }

  .item[aria-disabled='true']:hover {
    background: transparent;
  }
</style>
