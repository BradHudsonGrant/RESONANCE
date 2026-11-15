<script lang="ts">
  import Button from '../design/Button.svelte';
  import Icon from '../design/Icon.svelte';
  import Tooltip from '../design/Tooltip.svelte';
  import Wordmark from '../design/Wordmark.svelte';

  let { onsettings }: { onsettings: () => void } = $props();
</script>

<header class="topbar">
  <Wordmark />
  <div class="tools">
    <Tooltip text="Audio engine status arrives in M7" side="below">
      <Button variant="icon" aria-label="Audio engine status" aria-disabled="true">
        <Icon name="audio" size="md" />
      </Button>
    </Tooltip>
    <Tooltip text="Activity arrives in M2" side="below">
      <Button variant="icon" aria-label="Activity" aria-disabled="true" tabindex={-1}>
        <Icon name="activity" size="md" />
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
</style>
