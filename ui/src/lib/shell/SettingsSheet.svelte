<script lang="ts">
  import Button from '../design/Button.svelte';
  import ModularDivider from '../design/ModularDivider.svelte';
  import MonoLabel from '../design/MonoLabel.svelte';
  import SegmentedTabs, { type SegmentedTabItem } from '../design/SegmentedTabs.svelte';
  import Sheet from '../design/Sheet.svelte';
  import { getTheme, setTheme, type Theme } from '../design/theme';
  import FilenameSettings from '../library/FilenameSettings.svelte';
  import { keyNotation, setKeyNotation, type KeyNotation } from '../library/key-notation.svelte';
  import type { RenamePreviewClient } from '../library/rename-template.svelte';
  import SourcesSettings, { type SourcesClient } from '../library/SourcesSettings.svelte';
  import ProviderKeysSettings, { type ProviderKeysClient } from './ProviderKeysSettings.svelte';

  interface Props {
    open: boolean;
    sourcesClient?: SourcesClient;
    providerKeysClient?: ProviderKeysClient;
    renameClient?: RenamePreviewClient;
    sourcesRevision?: number;
    onsourceschanged?: () => void;
  }

  let {
    open = $bindable(false),
    sourcesClient,
    providerKeysClient,
    renameClient,
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

  const keyTabs: SegmentedTabItem[] = [
    { id: 'camelot', label: 'Camelot' },
    { id: 'open_key', label: 'Open Key' },
    { id: 'musical', label: 'Musical' },
  ];
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
        <MonoLabel text="Keys" tone="secondary" />
      </div>
      <SegmentedTabs
        items={keyTabs}
        value={keyNotation.current}
        ariaLabel="Key notation"
        onChange={(id) => setKeyNotation(id as KeyNotation)}
      />
      <p class="note">Sorting and key filters work the same in every notation.</p>
    </section>

    {#if renameClient}
      <ModularDivider />

      <section class="section">
        <div class="section-head">
          <MonoLabel text="File names" tone="secondary" />
        </div>
        <FilenameSettings client={renameClient} />
      </section>
    {/if}

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
        <MonoLabel text="Providers" tone="secondary" />
      </div>
      <ProviderKeysSettings client={providerKeysClient} active={open} />
    </section>

    <ModularDivider />

    <section class="section">
      <div class="section-head">
        <MonoLabel text="Audio & MIDI" tone="secondary" />
      </div>
      <p class="note">Audio and MIDI device configuration arrives in M7.</p>
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

  .note {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-sm);
  }
</style>
