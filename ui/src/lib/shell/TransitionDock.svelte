<script lang="ts">
  import Button from '../design/Button.svelte';
  import Icon from '../design/Icon.svelte';
  import Tooltip from '../design/Tooltip.svelte';
  import Deck from '../mix/Deck.svelte';

  let { collapsed, ontoggle }: { collapsed: boolean; ontoggle: () => void } = $props();

  const toggleLabel = $derived(collapsed ? 'Expand the dock' : 'Collapse the dock');
</script>

<section class="dock" aria-label="Transition">
  <div class="head">
    <div class="name">
      <span class="pair">A → B</span>
      <span class="label">Transition</span>
    </div>
    <div class="controls">
      <span class="label">Mix Point</span>
      <Tooltip text="Mix points arrive in M10">
        <Button variant="select" size="xs" aria-label="Mix Point" aria-disabled="true">—</Button>
      </Tooltip>
      <span class="label">Blend</span>
      <Tooltip text="Blends arrive in M10">
        <Button variant="select" size="xs" aria-label="Blend" aria-disabled="true">—</Button>
      </Tooltip>
    </div>
    <Tooltip text={toggleLabel} shortcut="D" side="left">
      <Button variant="icon" aria-label={toggleLabel} aria-expanded={!collapsed} onclick={ontoggle}>
        <Icon name={collapsed ? 'chevron-right' : 'chevron-down'} size="sm" />
      </Button>
    </Tooltip>
  </div>

  {#if !collapsed}
    <div class="body">
      <Deck deck="A" />
      <div class="crossfade">Two loaded decks show their crossfade here. Decks arrive in M7.</div>
      <Deck deck="B" />
    </div>
  {/if}
</section>

<style>
  .dock {
    border-top: var(--border-width) solid var(--color-border);
  }

  .head {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    gap: var(--space-4);
    height: var(--dock-header-height);
    padding: 0 var(--player-inset-x);
  }

  .head > :global(:last-child) {
    justify-self: end;
  }

  .name,
  .controls {
    display: flex;
    align-items: center;
    gap: var(--space-2-5);
    white-space: nowrap;
  }

  .pair {
    font-family: var(--font-display);
    font-size: var(--text-lg);
    font-weight: var(--weight-semibold);
  }

  .label {
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }

  .name .label {
    font-size: var(--text-md);
  }

  .controls .label:not(:first-child) {
    margin-left: var(--space-6);
  }

  .body {
    display: grid;
    grid-template-columns:
      clamp(var(--dock-card-min-width), 21.5vw, var(--dock-card-width))
      minmax(var(--dock-wave-min-width), 1fr)
      clamp(var(--dock-card-min-width), 21.5vw, var(--dock-card-width));
    align-items: center;
    gap: var(--dock-gap);
    padding: 0 var(--player-inset-x) var(--space-1-5);
  }

  .crossfade {
    display: grid;
    place-items: center;
    height: var(--dock-waveform-height);
    padding: 0 var(--space-4);
    color: var(--color-muted);
    font-size: var(--text-sm);
    text-align: center;
  }
</style>
