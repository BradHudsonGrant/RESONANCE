<script lang="ts">
  import Icon from '../design/Icon.svelte';
  import MonoLabel from '../design/MonoLabel.svelte';
  import TextInput from '../design/TextInput.svelte';
  import type { RenameFile } from '../ipc/bindings/RenameFile';
  import type { RenamePreview } from '../ipc/bindings/RenamePreview';
  import type { TagName } from '../ipc/bindings/TagName';
  import { templateTokens } from './rename-template.svelte';

  interface Props {
    value: string;
    preview: RenamePreview | null;
    error: string | null;
    /** What the preview rows are, for screen readers. */
    previewLabel: string;
    disabled?: boolean;
  }

  let { value = $bindable(), preview, error, previewLabel, disabled = false }: Props = $props();

  const tagNames: Record<TagName, string> = {
    title: 'title',
    artist: 'artist',
    mix: 'mix',
    album: 'album',
    album_artist: 'album artist',
    genre: 'genre',
    subgenre: 'subgenre',
    label: 'label',
    catno: 'catalogue number',
    year: 'year',
    bpm: 'BPM',
    key: 'key',
    track_number: 'track number',
    comment: 'comment',
  };

  function statusText(file: RenameFile): string {
    switch (file.status) {
      case 'ready':
        return 'Ready';
      case 'unchanged':
        return 'No change';
      case 'collision':
        return 'Name taken';
      case 'missing_fields':
        return `No ${file.missing.map((name) => tagNames[name]).join(' or ')}`;
      case 'too_long':
        return 'Folder path too long';
      case 'file_missing':
        return 'File missing';
    }
  }
</script>

<div class="template-field">
  <TextInput
    label="Filename template"
    bind:value
    {disabled}
    spellcheck="false"
    autocomplete="off"
    aria-invalid={error ? 'true' : undefined}
  />
  <p class="tokens">
    Tokens:
    {#each templateTokens as token, index (token)}
      <code class="token">{`{${token}}`}</code>{index < templateTokens.length - 1 ? ' ' : ''}
    {/each}
  </p>

  {#if error}
    <p class="error" role="alert">{error}</p>
  {:else if preview && preview.files.length > 0}
    <ul class="rows" aria-label={previewLabel}>
      {#each preview.files as file (file.track_id)}
        <li class={['row', file.status]}>
          <span class="old numeric">{file.old_name}</span>
          <span class="arrow" aria-hidden="true"><Icon name="arrow-right" size="xs" /></span>
          <span class="new numeric">{file.new_name ?? file.old_name}</span>
          <span class="status">
            <MonoLabel
              text={statusText(file)}
              tone={file.status === 'ready' ? 'primary' : 'muted'}
            />
          </span>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .template-field {
    display: grid;
    gap: var(--space-2);
    min-width: 0;
    container-type: inline-size;
  }

  .tokens {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-xs);
    line-height: var(--leading-normal);
  }

  .token {
    font-family: var(--font-mono);
  }

  .error {
    margin: 0;
    padding-left: var(--space-2-5);
    border-left: var(--selection-bar-width) solid var(--color-accent);
    color: var(--color-text);
    font-size: var(--text-sm);
  }

  .rows {
    display: grid;
    margin: 0;
    padding: 0;
    list-style: none;
    border-top: var(--border-width) solid var(--color-border);
  }

  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1-5) 0;
    border-bottom: var(--border-width) solid var(--color-border);
    font-size: var(--text-xs);
  }

  .old {
    overflow: hidden;
    color: var(--color-muted);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .arrow {
    display: inline-flex;
    color: var(--color-muted);
  }

  .new {
    overflow: hidden;
    color: var(--color-text);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row.unchanged .new,
  .row.missing_fields .new,
  .row.too_long .new,
  .row.file_missing .new {
    color: var(--color-muted);
  }

  .row.collision .new {
    text-decoration: line-through;
  }

  @container (width < 520px) {
    .row {
      grid-template-columns: minmax(0, 1fr) auto;
    }

    .old {
      grid-column: 1 / -1;
    }

    .arrow {
      display: none;
    }
  }

  @media (forced-colors: active) {
    .rows,
    .row {
      border-color: CanvasText;
    }
  }
</style>
