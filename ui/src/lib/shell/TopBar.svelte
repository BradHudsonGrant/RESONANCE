<script lang="ts">
  import Button from '../design/Button.svelte';
  import Icon from '../design/Icon.svelte';
  import Tooltip from '../design/Tooltip.svelte';
  import Wordmark from '../design/Wordmark.svelte';

  interface Props {
    onsettings: () => void;
    onactivity?: () => void;
    activeJobCount?: number;
    activityOpen?: boolean;
  }

  let {
    onsettings,
    onactivity = () => {},
    activeJobCount = 0,
    activityOpen = false,
  }: Props = $props();

  const activityTooltip = $derived(
    activeJobCount > 0 ? `Activity · ${activeJobCount} active` : 'Activity',
  );
</script>

<header class="topbar">
  <Wordmark />
  <div class="tools">
    <Tooltip text="Audio engine status arrives in M7" side="below">
      <Button variant="icon" aria-label="Audio engine status" aria-disabled="true">
        <Icon name="audio" size="md" />
      </Button>
    </Tooltip>
    <Tooltip text={activityTooltip} side="below">
      <Button variant="icon" aria-label="Activity" active={activityOpen} onclick={onactivity}>
        <span class="activity-glyph">
          <Icon name="activity" size="md" />
          {#if activeJobCount > 0}
            <span class="activity-dot" aria-hidden="true"></span>
          {/if}
        </span>
      </Button>
    </Tooltip>
    <Tooltip text="Settings" side="left">
      <Button variant="icon" aria-label="Settings" onclick={onsettings}>
        <Icon name="settings" size="md" />
      </Button>
    </Tooltip>
  </div>
</header>

<style>
  .topbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: var(--topbar-height);
    padding: 0 var(--space-5);
    border-bottom: var(--border-width) solid var(--color-border);
    background: var(--color-surface);
  }

  .tools {
    display: flex;
    align-items: center;
    gap: var(--space-0-5);
  }

  .activity-glyph {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .activity-dot {
    position: absolute;
    top: calc(var(--space-0-5) * -1);
    right: calc(var(--space-0-5) * -1);
    width: var(--provenance-dot-size);
    height: var(--provenance-dot-size);
    border-radius: var(--radius-pill);
    background: var(--color-accent);
  }

  @media (forced-colors: active) {
    .activity-dot {
      background: Highlight;
      forced-color-adjust: none;
    }
  }
</style>
