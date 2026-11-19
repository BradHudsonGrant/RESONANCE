<script lang="ts">
  import {
    filenameTemplate,
    LivePreview,
    setFilenameTemplate,
    type RenamePreviewClient,
  } from './rename-template.svelte';
  import TemplateField from './TemplateField.svelte';

  interface Props {
    client: RenamePreviewClient;
  }

  let { client }: Props = $props();

  const live = new LivePreview(() => client, 3);
  let template = $state(filenameTemplate.current);

  $effect(() => {
    live.request({ kind: 'query', query: {} }, template);
  });

  $effect(() => {
    if (live.template !== null && live.template === template) setFilenameTemplate(template);
  });
</script>

<TemplateField
  bind:value={template}
  preview={live.preview}
  error={live.error}
  previewLabel="New names for the first tracks in your library"
/>
<p class="note">
  The Rename sheet starts from this template. Files are only renamed when you rename them there.
</p>

<style>
  .note {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-sm);
  }
</style>
