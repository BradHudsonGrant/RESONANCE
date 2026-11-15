<script lang="ts">
  import Button from '../design/Button.svelte';
  import ModularDivider from '../design/ModularDivider.svelte';
  import MonoLabel from '../design/MonoLabel.svelte';
  import SegmentedTabs, { type SegmentedTabItem } from '../design/SegmentedTabs.svelte';
  import Sheet from '../design/Sheet.svelte';
  import { getTheme, setTheme, type Theme } from '../design/theme';
  import SourcesSettings, { type SourcesClient } from '../library/SourcesSettings.svelte';

  interface Props {
    open: boolean;
    sourcesClient?: SourcesClient;
    sourcesRevision?: number;
    onsourceschanged?: () => void;
  }

  let {
    open = $bindable(false),
    sourcesClient,
    sourcesRevision = 0,
    onsourceschanged,
  }: Props = $props();

  const themeTabs: SegmentedTabItem[] = [
    { id: 'dark', label: 'Dark' },
    { id: 'light', label: 'Light' },
    { id: 'system', label: 'System' },
  ];

  let theme = $state<Theme>(getTheme());

  function choose(id: string) {
    theme = id as Theme;
    setTheme(theme);
  }
</script>

<Sheet bind:open variant="drawer" title="Settings">
  <div class="settings-content">
    <section class="section">
      <div class="section-head">
        <MonoLabel text="Appearance" tone="secondary" />
      </div>
      <SegmentedTabs items={themeTabs} value={theme} ariaLabel="Theme" onChange={choose} />
    </section>

    <ModularDivider />

    <section class="section">
      <div class="section-head">
        <MonoLabel text="Library sources" tone="secondary" />
      </div>
      <SourcesSettings
        client={sourcesClient}
        active={open}
        revision={sourcesRevision}
        onchanged={onsourceschanged}
      />
    </section>

    <ModularDivider />

    <section class="section">
      <div class="section-head">
        <MonoLabel text="Provider keys" tone="secondary" />
      </div>
      <p class="waiting">Keys for AcoustID, Discogs and paid recognisers arrive in M3.</p>
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
    <Button variant="quiet" onclick={() => (open = false)}>Done</Button>
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

  .waiting {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-sm);
  }
</style>
