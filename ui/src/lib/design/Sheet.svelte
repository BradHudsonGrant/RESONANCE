<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    open?: boolean;
    title: string;
    children: Snippet;
    /** The buttons that answer the sheet, on its bottom edge. */
    actions?: Snippet;
  }

  let { open = $bindable(false), title, children, actions }: Props = $props();
  let dialog: HTMLDialogElement;
  const titleId = $props.id();

  $effect(() => {
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  });
</script>

<dialog bind:this={dialog} class="sheet" aria-labelledby={titleId} onclose={() => (open = false)}>
  <h2 id={titleId} class="title">{title}</h2>
  <div class="body">{@render children()}</div>
  {#if actions}
    <div class="actions">{@render actions()}</div>
  {/if}
</dialog>

<style>
  .sheet {
    width: min(560px, calc(100vw - var(--space-8)));
    margin-top: 12vh;
    padding: 0;
    border: 1px solid var(--color-border-control);
    border-radius: var(--radius-panel);
    background: var(--color-raised);
    color: var(--color-text);
  }

  .sheet::backdrop {
    background: rgb(0 0 0 / 50%);
  }

  .title {
    padding: var(--space-4) var(--space-4) 0;
    font-size: var(--text-lg);
  }

  .body {
    padding: var(--space-3) var(--space-4) var(--space-4);
    color: var(--color-text-2);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    border-top: 1px solid var(--color-border);
  }
</style>
