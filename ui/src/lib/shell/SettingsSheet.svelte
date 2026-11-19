<script lang="ts">
  import Button from '../design/Button.svelte';
  import ModularDivider from '../design/ModularDivider.svelte';
  import MonoLabel from '../design/MonoLabel.svelte';
  import SegmentedTabs, { type SegmentedTabItem } from '../design/SegmentedTabs.svelte';
  import Sheet from '../design/Sheet.svelte';
  import TextInput from '../design/TextInput.svelte';
  import type { PreviewStartPoint } from '../ipc/bindings/PreviewStartPoint';
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
    /** Where auto-preview starts each track. */
    previewStart?: PreviewStartPoint;
    maxStartSeconds?: number;
    onpreviewstart?: (start: PreviewStartPoint) => void;
    /** Whether the quick filters bar recommends filters. */
    recommendFilters?: boolean;
    onrecommendfilters?: (on: boolean) => void;
    onsourceschanged?: () => void;
  }

  let {
    open = $bindable(false),
    sourcesClient,
    providerKeysClient,
    renameClient,
    sourcesRevision = 0,
    previewStart = { kind: 'thirty_percent' },
    maxStartSeconds = 600,
    onpreviewstart,
    recommendFilters = true,
    onrecommendfilters,
    onsourceschanged,
  }: Props = $props();

  const startTabs: SegmentedTabItem[] = [
    { id: 'beginning', label: 'Beginning' },
    { id: 'thirty_percent', label: '30 % in' },
    { id: 'seconds', label: 'Fixed time' },
  ];

  const DEFAULT_START_SECONDS = 30;

  let secondsText = $derived(
    String(previewStart.kind === 'seconds' ? previewStart.seconds : DEFAULT_START_SECONDS),
  );

  function chooseStart(kind: string) {
    if (kind === 'seconds') {
      onpreviewstart?.({ kind, seconds: Number(secondsText) || DEFAULT_START_SECONDS });
    } else if (kind === 'beginning' || kind === 'thirty_percent') {
      onpreviewstart?.({ kind });
    }
  }

  function commitSeconds() {
    const seconds = Math.round(Number(secondsText));
    if (Number.isFinite(seconds) && seconds >= 0) onpreviewstart?.({ kind: 'seconds', seconds });
  }

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

    <ModularDivider />

    <section class="section">
      <div class="section-head">
        <MonoLabel text="Previews" tone="secondary" />
      </div>
      <SegmentedTabs
        items={startTabs}
        value={previewStart.kind}
        ariaLabel="Preview start"
        onChange={chooseStart}
      />
      {#if previewStart.kind === 'seconds'}
        <div class="seconds">
          <TextInput
            label="Seconds in"
            type="number"
            inputmode="numeric"
            min="0"
            max={maxStartSeconds}
            step="1"
            bind:value={secondsText}
            onchange={commitSeconds}
          />
        </div>
      {/if}
      <p class="note">
        Where auto-preview starts each track. Turn auto-preview on in the player bar, or press P.
      </p>
    </section>

    <ModularDivider />

    <section class="section">
      <div class="section-head">
        <MonoLabel text="Quick filters" tone="secondary" />
      </div>
      <label class="check">
        <input
          type="checkbox"
          checked={recommendFilters}
          onchange={(event) => onrecommendfilters?.(event.currentTarget.checked)}
        />
        <span>Recommend quick filters</span>
      </label>
      <p class="note">
        Up to five filters you use most show beside the ones you pin. They are counted on this
        computer only.
      </p>
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

  .check {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1-5);
    color: var(--color-text);
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .check input {
    width: var(--checkbox-size);
    height: var(--checkbox-size);
    margin: 0;
    accent-color: var(--color-text);
  }

  .seconds {
    max-width: calc(var(--space-6) * 6);
  }
</style>
