<script lang="ts">
  import Button from '../design/Button.svelte';
  import ModularDivider from '../design/ModularDivider.svelte';
  import MonoLabel from '../design/MonoLabel.svelte';
  import SegmentedTabs, { type SegmentedTabItem } from '../design/SegmentedTabs.svelte';
  import Sheet from '../design/Sheet.svelte';
  import { getTheme, setTheme, type Theme } from '../design/theme';

  interface Props {
    open: boolean;
    onclose: () => void;
  }

  let { open = $bindable(false), onclose }: Props = $props();

  const themeTabs: SegmentedTabItem[] = [
    { id: 'dark', label: 'Dark' },
    { id: 'light', label: 'Light' },
    { id: 'system', label: 'System' },
  ];

  let currentTheme = $state<Theme>(getTheme());

  function onThemeChange(id: string) {
    const next = id as Theme;
    currentTheme = next;
    setTheme(next);
  }

  $effect(() => {
    if (open) {
      currentTheme = getTheme();
    }
  });

  function close() {
    open = false;
    onclose?.();
  }
</script>

<Sheet bind:open variant="drawer" title="Settings">
  <div class="settings-content">
    <section class="section">
      <div class="section-head">
        <MonoLabel text="Appearance" tone="secondary" />
      </div>
      <p class="description">Select interface theme.</p>
      <SegmentedTabs
        items={themeTabs}
        value={currentTheme}
        ariaLabel="Theme selection"
        onChange={onThemeChange}
      />
    </section>

    <ModularDivider />

    <section class="section">
      <div class="section-head">
        <MonoLabel text="Library sources" tone="secondary" />
      </div>
      <p class="waiting">Library sources arrive in M2.</p>
    </section>

    <ModularDivider />

    <section class="section">
      <div class="section-head">
        <MonoLabel text="AI provider keys" tone="secondary" />
      </div>
      <p class="waiting">AI provider keys arrive in M3.</p>
    </section>

    <ModularDivider />

    <section class="section">
      <div class="section-head">
        <MonoLabel text="Audio & MIDI" tone="secondary" />
      </div>
      <p class="waiting">Audio and MIDI device configuration arrives in M7.</p>
    </section>
  </div>

  {#snippet actions()}
    <Button variant="quiet" onclick={close}>Done</Button>
  {/snippet}
</Sheet>

<style>
  .settings-content {
    display: grid;
    gap: var(--space-4);
  }

  .section {
    display: grid;
    gap: var(--space-2);
  }

  .section-head {
    display: flex;
    align-items: center;
  }

  .description {
    margin: 0;
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }

  .waiting {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-sm);
  }
</style>
