<script lang="ts">
  import Button from '../design/Button.svelte';
  import ModularDivider from '../design/ModularDivider.svelte';
  import MonoLabel from '../design/MonoLabel.svelte';
  import Sheet from '../design/Sheet.svelte';
  import type { LibraryFacets } from '../ipc/bindings/LibraryFacets';
  import type { TrackFormat } from '../ipc/bindings/TrackFormat';
  import type { TrackQuery } from '../ipc/bindings/TrackQuery';
  import { KEY_WHEEL } from '../ipc/bindings/KeyWheel';
  import { keyCode, keySecond } from './key-notation.svelte';
  import { countActiveDrawerFilters } from './track-pages.svelte';

  interface Props {
    open: boolean;
    query: TrackQuery;
    facets: LibraryFacets | null;
    onchange: (next: TrackQuery) => void;
    onclear: () => void;
  }

  const formats: { id: TrackFormat; label: string }[] = [
    { id: 'mp3', label: 'MP3' },
    { id: 'flac', label: 'FLAC' },
    { id: 'wav', label: 'WAV' },
    { id: 'aiff', label: 'AIFF' },
    { id: 'm4a', label: 'M4A' },
  ];

  let { open = $bindable(false), query, facets, onchange, onclear }: Props = $props();

  const baseId = $props.id();
  const ids = {
    bpmMin: `${baseId}-bpm-min`,
    bpmMax: `${baseId}-bpm-max`,
    yearMin: `${baseId}-year-min`,
    yearMax: `${baseId}-year-max`,
    label: `${baseId}-label`,
  };

  const activeCount = $derived(countActiveDrawerFilters(query));
  const selectedKeys = $derived(new Set(query.keys ?? []));
  const selectedFormats = $derived(new Set(query.formats ?? []));

  function parseNumber(raw: string, integer: boolean): number | null {
    const trimmed = raw.trim();
    if (!trimmed) return null;
    const parsed = integer ? Number.parseInt(trimmed, 10) : Number.parseFloat(trimmed);
    return Number.isFinite(parsed) ? parsed : null;
  }

  function updateBpm(edge: 'bpm_min' | 'bpm_max', value: string) {
    onchange({ ...query, [edge]: parseNumber(value, false) });
  }

  function updateYear(edge: 'year_min' | 'year_max', value: string) {
    onchange({ ...query, [edge]: parseNumber(value, true) });
  }

  function toggleKey(code: string) {
    const current = query.keys ?? [];
    const next = current.includes(code)
      ? current.filter((item) => item !== code)
      : [...current, code];
    onchange({ ...query, keys: next.length > 0 ? next : null });
  }

  function toggleFormat(format: TrackFormat) {
    const current = query.formats ?? [];
    const next = current.includes(format)
      ? current.filter((item) => item !== format)
      : [...current, format];
    onchange({ ...query, formats: next.length > 0 ? next : null });
  }

  function updateLabel(value: string) {
    onchange({ ...query, label: value.trim() || null });
  }
</script>

<Sheet bind:open variant="drawer" title="Filters">
  <div class="drawer-content">
    <section class="section">
      <div class="section-head">
        <MonoLabel text="Tempo (BPM)" tone="secondary" />
        {#if facets?.bpm_min != null && facets?.bpm_max != null}
          <span class="bounds numeric">
            {Math.round(facets.bpm_min)}–{Math.round(facets.bpm_max)}
          </span>
        {/if}
      </div>
      <div class="range-pair">
        <label class="range-field" for={ids.bpmMin}>
          <span class="range-caption">Min</span>
          <input
            id={ids.bpmMin}
            class="field numeric"
            type="number"
            inputmode="decimal"
            step="0.1"
            min="1"
            placeholder={facets?.bpm_min != null ? String(Math.round(facets.bpm_min)) : 'Any'}
            value={query.bpm_min ?? ''}
            oninput={(event) => updateBpm('bpm_min', event.currentTarget.value)}
          />
        </label>
        <label class="range-field" for={ids.bpmMax}>
          <span class="range-caption">Max</span>
          <input
            id={ids.bpmMax}
            class="field numeric"
            type="number"
            inputmode="decimal"
            step="0.1"
            min="1"
            placeholder={facets?.bpm_max != null ? String(Math.round(facets.bpm_max)) : 'Any'}
            value={query.bpm_max ?? ''}
            oninput={(event) => updateBpm('bpm_max', event.currentTarget.value)}
          />
        </label>
      </div>
    </section>

    <ModularDivider />

    <section class="section">
      <div class="section-head">
        <MonoLabel text="Key" tone="secondary" />
        {#if selectedKeys.size > 0}
          <span class="bounds numeric">{selectedKeys.size} selected</span>
        {/if}
      </div>
      <div class="key-grid" role="group" aria-label="Keys">
        {#each KEY_WHEEL as item (item.camelot)}
          {@const code = keyCode(item)}
          {@const other = keySecond(item)}
          <button
            type="button"
            class={['key-btn', { pressed: selectedKeys.has(item.camelot) }]}
            aria-label="{code} ({other})"
            aria-pressed={selectedKeys.has(item.camelot)}
            title="{code} · {other}"
            onclick={() => toggleKey(item.camelot)}
          >
            <span class="key-code numeric">{code}</span>
            <span class="key-name">{other}</span>
          </button>
        {/each}
      </div>
    </section>

    <ModularDivider />

    <section class="section">
      <div class="section-head">
        <MonoLabel text="Year" tone="secondary" />
        {#if facets?.year_min != null && facets?.year_max != null}
          <span class="bounds numeric">{facets.year_min}–{facets.year_max}</span>
        {/if}
      </div>
      <div class="range-pair">
        <label class="range-field" for={ids.yearMin}>
          <span class="range-caption">From</span>
          <input
            id={ids.yearMin}
            class="field numeric"
            type="number"
            inputmode="numeric"
            step="1"
            min="1900"
            max="2999"
            placeholder={facets?.year_min != null ? String(facets.year_min) : 'Any'}
            value={query.year_min ?? ''}
            oninput={(event) => updateYear('year_min', event.currentTarget.value)}
          />
        </label>
        <label class="range-field" for={ids.yearMax}>
          <span class="range-caption">To</span>
          <input
            id={ids.yearMax}
            class="field numeric"
            type="number"
            inputmode="numeric"
            step="1"
            min="1900"
            max="2999"
            placeholder={facets?.year_max != null ? String(facets.year_max) : 'Any'}
            value={query.year_max ?? ''}
            oninput={(event) => updateYear('year_max', event.currentTarget.value)}
          />
        </label>
      </div>
    </section>

    <ModularDivider />

    <section class="section">
      <label class="select-wrap" for={ids.label}>
        <MonoLabel text="Label" tone="secondary" />
        <select
          id={ids.label}
          class="field select"
          value={query.label ?? ''}
          onchange={(event) => updateLabel(event.currentTarget.value)}
        >
          <option value="">All labels</option>
          {#each facets?.labels ?? [] as label (label)}
            <option value={label}>{label}</option>
          {/each}
        </select>
      </label>
    </section>

    <ModularDivider />

    <section class="section">
      <MonoLabel text="Format" tone="secondary" />
      <div class="format-list" role="group" aria-label="Audio formats">
        {#each formats as item (item.id)}
          <label class="format-check">
            <input
              type="checkbox"
              checked={selectedFormats.has(item.id)}
              onchange={() => toggleFormat(item.id)}
            />
            <span class="numeric">{item.label}</span>
          </label>
        {/each}
      </div>
    </section>

    <ModularDivider />

    <section class="section">
      <div class="section-head">
        <MonoLabel text="File status" tone="secondary" />
        {#if facets != null}
          <span class="bounds numeric">{facets.missing_count} missing</span>
        {/if}
      </div>
      <label class="format-check">
        <input
          type="checkbox"
          checked={Boolean(query.missing_only)}
          onchange={() => onchange({ ...query, missing_only: query.missing_only ? null : true })}
        />
        <span>Missing files only</span>
      </label>
    </section>
  </div>

  {#snippet actions()}
    <Button variant="quiet" disabled={activeCount === 0} onclick={onclear}>Clear</Button>
    <Button variant="primary" onclick={() => (open = false)}>Done</Button>
  {/snippet}
</Sheet>

<style>
  .drawer-content {
    display: grid;
    gap: var(--space-3-5);
  }

  .section {
    display: grid;
    gap: var(--space-2);
  }

  .section-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .bounds {
    color: var(--color-muted);
    font-size: var(--text-xs);
  }

  .range-pair {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-2);
  }

  .range-field,
  .select-wrap {
    display: grid;
    gap: var(--space-1);
  }

  .range-caption {
    color: var(--color-text-2);
    font-size: var(--text-xs);
  }

  .field {
    width: 100%;
    height: var(--control-height-md);
    padding: 0 var(--space-2-5);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    font-size: var(--text-md);
  }

  .field:focus-visible {
    border-color: var(--color-text);
  }

  .key-grid {
    display: grid;
    grid-template-columns: repeat(6, minmax(0, 1fr));
    gap: var(--space-1);
  }

  .key-btn {
    display: grid;
    justify-items: center;
    padding: var(--space-1) 0;
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-text-2);
    cursor: pointer;
    transition:
      background-color var(--duration-fast) var(--ease-out),
      border-color var(--duration-fast) var(--ease-out),
      color var(--duration-fast) var(--ease-out);
  }

  .key-btn:hover {
    background: var(--color-surface-2);
    color: var(--color-text);
  }

  .key-btn.pressed {
    border-color: var(--color-text);
    background: var(--color-text);
    color: var(--color-background);
  }

  .key-code {
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    line-height: var(--leading-compact);
  }

  .key-name {
    font-size: var(--text-xs);
    line-height: var(--leading-compact);
    opacity: 0.78;
  }

  .format-list {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2-5);
  }

  .format-check {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1-5);
    color: var(--color-text);
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .format-check input {
    width: var(--checkbox-size);
    height: var(--checkbox-size);
    margin: 0;
    accent-color: var(--color-text);
  }

  @media (forced-colors: active) {
    .key-btn.pressed {
      border-color: Highlight;
      background: Highlight;
      color: HighlightText;
      forced-color-adjust: none;
    }
  }
</style>
