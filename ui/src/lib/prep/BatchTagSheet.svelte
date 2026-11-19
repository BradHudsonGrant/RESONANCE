<script lang="ts">
  import { untrack } from 'svelte';

  import Button from '../design/Button.svelte';
  import ModularDivider from '../design/ModularDivider.svelte';
  import MonoLabel from '../design/MonoLabel.svelte';
  import SegmentedTabs from '../design/SegmentedTabs.svelte';
  import Sheet from '../design/Sheet.svelte';
  import type { AcceptedProposals } from '../ipc/bindings/AcceptedProposals';
  import type { BatchTagTool } from '../ipc/bindings/BatchTagTool';
  import type { TagName } from '../ipc/bindings/TagName';
  import type { TextCaseName } from '../ipc/bindings/TextCaseName';

  export interface BatchTagClient {
    preview: (trackIds: number[], tool: BatchTagTool) => Promise<AcceptedProposals>;
    apply: (trackIds: number[], tool: BatchTagTool) => Promise<AcceptedProposals>;
  }

  export type BatchToolTab = 'edit' | 'replace' | 'case' | 'release';

  interface Props {
    open: boolean;
    trackIds: number[];
    sourceTrackId?: number | null;
    sourceTrackTitle?: string | null;
    scopeLabel: string;
    initialTab?: BatchToolTab;
    client: BatchTagClient;
    onapplied?: (report: AcceptedProposals) => void;
  }

  let {
    open = $bindable(false),
    trackIds,
    sourceTrackId = null,
    sourceTrackTitle = null,
    scopeLabel,
    initialTab = 'edit',
    client,
    onapplied,
  }: Props = $props();

  const TABS = [
    { id: 'edit', label: 'Edit fields' },
    { id: 'replace', label: 'Find and replace' },
    { id: 'case', label: 'Fix case' },
    { id: 'release', label: 'Release fields' },
  ];

  const ALL_FIELDS: { value: TagName; label: string }[] = [
    { value: 'genre', label: 'Genre' },
    { value: 'subgenre', label: 'Subgenre' },
    { value: 'artist', label: 'Artist' },
    { value: 'title', label: 'Title' },
    { value: 'mix', label: 'Mix' },
    { value: 'album', label: 'Album' },
    { value: 'album_artist', label: 'Album artist' },
    { value: 'label', label: 'Label' },
    { value: 'catno', label: 'Catalogue number' },
    { value: 'year', label: 'Year' },
    { value: 'bpm', label: 'BPM' },
    { value: 'key', label: 'Key' },
    { value: 'track_number', label: 'Track number' },
    { value: 'comment', label: 'Comment' },
    { value: 'composer', label: 'Composer' },
    { value: 'remixer', label: 'Remixer' },
  ];

  const CASE_FIELDS: { value: TagName; label: string }[] = [
    { value: 'title', label: 'Title' },
    { value: 'artist', label: 'Artist' },
    { value: 'mix', label: 'Mix' },
    { value: 'album', label: 'Album' },
    { value: 'label', label: 'Label' },
    { value: 'genre', label: 'Genre' },
  ];

  const RELEASE_FIELDS: { value: TagName; label: string }[] = [
    { value: 'album', label: 'Album' },
    { value: 'label', label: 'Label' },
    { value: 'catno', label: 'Catalogue number' },
    { value: 'year', label: 'Year' },
  ];

  let activeTab = $state<BatchToolTab>('edit');
  let editField = $state<TagName>('genre');
  let editValue = $state('');
  let replaceField = $state<'all' | TagName>('all');
  let findText = $state('');
  let replaceText = $state('');
  let caseSensitive = $state(false);
  let textCase = $state<TextCaseName>('title');
  let caseFields = $state<TagName[]>(['title', 'artist', 'mix', 'album', 'label']);
  let releaseFields = $state<TagName[]>(['album', 'label', 'catno', 'year']);

  let previewReport = $state<AcceptedProposals>({ tracks: 0, fields: 0 });
  let applying = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    if (!open) return;
    const startTab = initialTab;
    untrack(() => {
      activeTab = startTab;
      error = null;
    });
  });

  const referenceTrackId = $derived(sourceTrackId ?? trackIds[0] ?? null);

  const currentTool = $derived.by<BatchTagTool | null>(() => {
    if (activeTab === 'edit') {
      return {
        kind: 'set_fields',
        edits: [{ field: editField, value: editValue }],
      };
    }
    if (activeTab === 'replace') {
      if (!findText) return null;
      return {
        kind: 'find_and_replace',
        fields: replaceField === 'all' ? [] : [replaceField],
        find: findText,
        replace: replaceText,
        case_sensitive: caseSensitive,
      };
    }
    if (activeTab === 'case') {
      if (caseFields.length === 0) return null;
      return {
        kind: 'fix_case',
        fields: caseFields,
        text_case: textCase,
      };
    }
    if (referenceTrackId === null || releaseFields.length === 0) return null;
    return {
      kind: 'apply_release_fields',
      source_track_id: referenceTrackId,
      fields: releaseFields,
    };
  });

  let previewInFlight = false;
  let previewQueued = false;

  async function refreshPreview() {
    if (previewInFlight) {
      previewQueued = true;
      return;
    }
    previewInFlight = true;
    try {
      do {
        previewQueued = false;
        const tool = currentTool;
        const ids = [...trackIds];
        if (!open || !tool || (ids.length === 0 && activeTab !== 'release')) {
          previewReport = { tracks: 0, fields: 0 };
          return;
        }
        try {
          previewReport = await client.preview(ids, tool);
          error = null;
        } catch (err) {
          previewReport = { tracks: 0, fields: 0 };
          error = err instanceof Error ? err.message : String(err);
        }
      } while (previewQueued);
    } finally {
      previewInFlight = false;
    }
  }

  $effect(() => {
    if (!open) return;
    void currentTool;
    void trackIds.length;
    untrack(() => {
      void refreshPreview();
    });
  });

  function toggleField(list: TagName[], field: TagName): TagName[] {
    return list.includes(field) ? list.filter((item) => item !== field) : [...list, field];
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    const tool = currentTool;
    if (applying || !tool) return;
    applying = true;
    error = null;
    try {
      const report = await client.apply(trackIds, tool);
      onapplied?.(report);
      open = false;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      applying = false;
    }
  }
</script>

<Sheet bind:open title="Batch tag tools">
  <form id="batch-tag-form" class="batch-body" onsubmit={submit}>
    <div class="scope-row">
      <MonoLabel text="Scope" tone="secondary" />
      <p class="scope-text">
        <span>{scopeLabel}</span>
        <span class="numeric">
          {trackIds.length === 1 ? '1 track' : `${trackIds.length.toLocaleString('en-GB')} tracks`}
        </span>
      </p>
    </div>

    <SegmentedTabs
      items={TABS}
      value={activeTab}
      ariaLabel="Batch tag tool"
      onChange={(next) => (activeTab = next as BatchToolTab)}
    />

    <ModularDivider />

    {#if activeTab === 'edit'}
      <div class="tool-group">
        <div class="field-pair">
          <label class="field-label" for="batch-edit-field">Field</label>
          <select id="batch-edit-field" class="control-select" bind:value={editField}>
            {#each ALL_FIELDS as item (item.value)}
              <option value={item.value}>{item.label}</option>
            {/each}
          </select>
        </div>
        <div class="field-pair">
          <label class="field-label" for="batch-edit-value">Value</label>
          <input
            id="batch-edit-value"
            class="control-input"
            type="text"
            placeholder="Leave empty to clear this field"
            bind:value={editValue}
          />
        </div>
      </div>
    {:else if activeTab === 'replace'}
      <div class="tool-group">
        <div class="field-pair">
          <label class="field-label" for="batch-replace-field">In field</label>
          <select id="batch-replace-field" class="control-select" bind:value={replaceField}>
            <option value="all">All text fields</option>
            {#each ALL_FIELDS as item (item.value)}
              <option value={item.value}>{item.label}</option>
            {/each}
          </select>
        </div>
        <div class="field-pair">
          <label class="field-label" for="batch-find-input">Find</label>
          <input
            id="batch-find-input"
            class="control-input"
            type="text"
            placeholder="Text to find"
            bind:value={findText}
          />
        </div>
        <div class="field-pair">
          <label class="field-label" for="batch-replace-input">Replace with</label>
          <input
            id="batch-replace-input"
            class="control-input"
            type="text"
            placeholder="Leave empty to remove matching text"
            bind:value={replaceText}
          />
        </div>
        <label class="check-line">
          <input type="checkbox" bind:checked={caseSensitive} />
          <span>Match case</span>
        </label>
      </div>
    {:else if activeTab === 'case'}
      <div class="tool-group">
        <div class="field-pair">
          <span class="field-label">Case style</span>
          <div class="choice-row" role="radiogroup" aria-label="Case style">
            {#each [{ id: 'title', label: 'Title Case' }, { id: 'sentence', label: 'Sentence case' }, { id: 'upper', label: 'UPPER CASE' }] as choice (choice.id)}
              <button
                type="button"
                role="radio"
                aria-checked={textCase === choice.id}
                class={['choice-btn', { active: textCase === choice.id }]}
                onclick={() => (textCase = choice.id as TextCaseName)}
              >
                {choice.label}
              </button>
            {/each}
          </div>
        </div>
        <div class="field-pair">
          <span class="field-label">Fields to fix</span>
          <div class="choice-row">
            {#each CASE_FIELDS as item (item.value)}
              <button
                type="button"
                aria-pressed={caseFields.includes(item.value)}
                class={['choice-btn', { active: caseFields.includes(item.value) }]}
                onclick={() => (caseFields = toggleField(caseFields, item.value))}
              >
                {item.label}
              </button>
            {/each}
          </div>
        </div>
        <p class="note">
          DJ and format acronyms (UK, DJ, EP, VIP, LP, MC, II) stay uppercase in Title and Sentence
          case.
        </p>
      </div>
    {:else}
      <div class="tool-group">
        <p class="note">
          Copies release fields from
          <strong>{sourceTrackTitle ?? 'the focused track'}</strong>
          to every track in the same release folder or selected batch.
        </p>
        <div class="field-pair">
          <span class="field-label">Fields to copy</span>
          <div class="choice-row">
            {#each RELEASE_FIELDS as item (item.value)}
              <button
                type="button"
                aria-pressed={releaseFields.includes(item.value)}
                class={['choice-btn', { active: releaseFields.includes(item.value) }]}
                onclick={() => (releaseFields = toggleField(releaseFields, item.value))}
              >
                {item.label}
              </button>
            {/each}
          </div>
        </div>
      </div>
    {/if}

    <p class="preview-count numeric" aria-live="polite">
      Will update {previewReport.tracks.toLocaleString('en-GB')}
      {previewReport.tracks === 1 ? 'track' : 'tracks'}
      ({previewReport.fields.toLocaleString('en-GB')}
      {previewReport.fields === 1 ? 'field' : 'fields'})
    </p>

    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}
  </form>

  {#snippet actions()}
    <Button onclick={() => (open = false)} disabled={applying}>Cancel</Button>
    <Button
      variant="primary"
      type="submit"
      form="batch-tag-form"
      disabled={applying || currentTool === null || previewReport.fields === 0}
    >
      Apply to {previewReport.tracks.toLocaleString('en-GB')}
      {previewReport.tracks === 1 ? 'track' : 'tracks'}
    </Button>
  {/snippet}
</Sheet>

<style>
  .batch-body {
    display: grid;
    gap: var(--space-3);
  }

  .scope-row {
    display: grid;
    gap: var(--space-1);
  }

  .scope-text {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-2);
    margin: 0;
    color: var(--color-text);
    font-size: var(--text-md);
    font-weight: var(--weight-medium);
  }

  .tool-group {
    display: grid;
    gap: var(--space-3);
  }

  .field-pair {
    display: grid;
    gap: var(--space-1-5);
  }

  .field-label {
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }

  .control-input,
  .control-select {
    height: var(--control-height-md);
    padding: 0 var(--space-2-5);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-text);
    font-family: var(--font-sans);
    font-size: var(--text-md);
  }

  .choice-row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .choice-btn {
    height: var(--control-height-sm);
    padding: 0 var(--space-3);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-text-2);
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    cursor: pointer;
    transition:
      background-color var(--duration-fast) var(--ease-out),
      border-color var(--duration-fast) var(--ease-out),
      color var(--duration-fast) var(--ease-out);
  }

  .choice-btn:hover {
    border-color: var(--color-border-control);
    color: var(--color-text);
  }

  .choice-btn.active {
    border-color: var(--color-border-control);
    background: var(--color-raised);
    color: var(--color-text);
    font-weight: var(--weight-medium);
  }

  .check-line {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--color-text);
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .note {
    margin: 0;
    color: var(--color-text-2);
    font-size: var(--text-sm);
    line-height: var(--leading-normal);
  }

  .preview-count {
    margin: 0;
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }

  .error {
    margin: 0;
    color: var(--color-text);
    font-size: var(--text-sm);
  }

  @media (forced-colors: active) {
    .choice-btn.active {
      border-color: Highlight;
      background: Highlight;
      color: HighlightText;
      forced-color-adjust: none;
    }
  }
</style>
