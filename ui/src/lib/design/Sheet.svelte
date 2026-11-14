<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    open?: boolean;
    title: string;
    variant?: 'modal' | 'drawer';
    children: Snippet;
    /** The buttons that answer the sheet, on its bottom edge. */
    actions?: Snippet;
  }

  let { open = $bindable(false), title, variant = 'modal', children, actions }: Props = $props();
  let dialog: HTMLDialogElement;
  const titleId = $props.id();

  $effect(() => {
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  });
</script>

<dialog
  bind:this={dialog}
  class={['sheet', variant]}
  aria-labelledby={titleId}
  onclose={() => (open = false)}
>
  <h2 id={titleId} class="title">{title}</h2>
  <div class="body">{@render children()}</div>
  {#if actions}
    <div class="actions">{@render actions()}</div>
  {/if}
</dialog>

<style>
  .sheet {
    width: min(var(--sheet-max-width), calc(100vw - var(--space-8)));
    margin-top: 12vh;
    padding: 0;
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-xl);
    background: var(--color-raised);
    color: var(--color-text);
  }

  .drawer {
    width: min(var(--sheet-drawer-width), 100vw);
    height: 100vh;
    max-height: 100vh;
    margin: 0 0 0 auto;
    border-top: 0;
    border-right: 0;
    border-bottom: 0;
    border-radius: 0;
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
    border-top: var(--border-width) solid var(--color-border);
  }
</style>
