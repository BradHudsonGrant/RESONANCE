<script lang="ts" module>
  import type { LibrarySources } from '../ipc/bindings/LibrarySources';
  import type { ScanReport } from '../ipc/bindings/ScanReport';
  import type { SourceSubfolder } from '../ipc/bindings/SourceSubfolder';

  export interface SourcesClient {
    sources: () => Promise<LibrarySources>;
    subfolders: (sourceId: number, parent: string | null) => Promise<SourceSubfolder[]>;
    scan: (folder: string) => Promise<ScanReport>;
    updateSource: (params: {
      source_id: number;
      include_subfolders: boolean;
      watch: boolean;
      excluded_subfolders: string[];
    }) => Promise<LibrarySources>;
    removeSource: (sourceId: number) => Promise<LibrarySources>;
    setIgnoredFolders: (folders: string[]) => Promise<LibrarySources>;
  }
</script>

<script lang="ts">
  import { SvelteMap, SvelteSet } from 'svelte/reactivity';

  import Button from '../design/Button.svelte';
  import DataTag from '../design/DataTag.svelte';
  import Icon from '../design/Icon.svelte';
  import MonoLabel from '../design/MonoLabel.svelte';
  import TextInput from '../design/TextInput.svelte';
  import type { LibrarySource } from '../ipc/bindings/LibrarySource';
  import { trackCount } from './format';

  const DEFAULT_IGNORED = ['_replaced', '_wrong_match', '_trash', '_converted_originals'];

  interface Props {
    client?: SourcesClient;
    active?: boolean;
    revision?: number;
    onchanged?: () => void;
  }

  interface VisibleNode {
    entry: SourceSubfolder;
    depth: number;
    parentDisabled: boolean;
    expanded: boolean;
  }

  let { client, active = true, revision = 0, onchanged }: Props = $props();

  let sources = $state<LibrarySource[]>([]);
  let ignoredFolders = $state<string[]>([...DEFAULT_IGNORED]);
  let newFolder = $state('');
  let newIgnored = $state('');
  let busy = $state(false);
  let status = $state('');
  let error = $state('');

  const openTrees = new SvelteSet<number>();
  const expandedSubfolders = new SvelteSet<string>();
  const subfolderLists = new SvelteMap<string, SourceSubfolder[]>();

  function treeKey(sourceId: number, parent: string | null): string {
    return `${sourceId}:${parent ?? ''}`;
  }

  function applySources(next: LibrarySources) {
    sources = next.sources;
    ignoredFolders = next.ignored_folders;
  }

  async function refreshLoadedSubfolders(sourceId: number) {
    if (!client) return;
    const prefix = `${sourceId}:`;
    const keys = [...subfolderLists.keys()].filter((key) => key.startsWith(prefix));
    await Promise.all(
      keys.map(async (key) => {
        const parent = key.slice(prefix.length);
        const list = await client.subfolders(sourceId, parent === '' ? null : parent);
        subfolderLists.set(key, list);
      }),
    );
  }

  $effect(() => {
    void revision;
    if (!client || !active) return;
    let cancelled = false;
    void client
      .sources()
      .then((result) => {
        if (!cancelled) applySources(result);
      })
      .catch((failure: unknown) => {
        if (!cancelled) {
          error = failure instanceof Error ? failure.message : String(failure);
        }
      });
    return () => {
      cancelled = true;
    };
  });

  async function runAction(action: () => Promise<void>) {
    if (!client || busy) return;
    busy = true;
    error = '';
    try {
      await action();
    } catch (failure) {
      error = failure instanceof Error ? failure.message : String(failure);
    } finally {
      busy = false;
    }
  }

  async function addSource(event: SubmitEvent) {
    event.preventDefault();
    const folder = newFolder.trim();
    if (!client || !folder) return;
    await runAction(async () => {
      const report = await client.scan(folder);
      applySources(await client.sources());
      newFolder = '';
      status = `${report.found} files: ${report.added} added, ${report.changed} changed, ${report.unchanged} unchanged.`;
      onchanged?.();
    });
  }

  async function rescan(folder: string) {
    if (!client) return;
    await runAction(async () => {
      const report = await client.scan(folder);
      applySources(await client.sources());
      status = `${report.found} files: ${report.added} added, ${report.changed} changed, ${report.unchanged} unchanged.`;
      onchanged?.();
    });
  }

  async function remove(sourceId: number) {
    if (!client) return;
    await runAction(async () => {
      status = '';
      applySources(await client.removeSource(sourceId));
      openTrees.delete(sourceId);
      for (const key of [...subfolderLists.keys()]) {
        if (key.startsWith(`${sourceId}:`)) subfolderLists.delete(key);
      }
      onchanged?.();
    });
  }

  async function toggleIncludeSubfolders(source: LibrarySource, include: boolean) {
    if (!client) return;
    await runAction(async () => {
      status = '';
      applySources(
        await client.updateSource({
          source_id: source.id,
          include_subfolders: include,
          watch: source.watch,
          excluded_subfolders: source.excluded_subfolders,
        }),
      );
      if (include && openTrees.has(source.id)) {
        await refreshLoadedSubfolders(source.id);
      }
      onchanged?.();
    });
  }

  async function toggleWatch(source: LibrarySource, watch: boolean) {
    if (!client) return;
    await runAction(async () => {
      status = '';
      applySources(
        await client.updateSource({
          source_id: source.id,
          include_subfolders: source.include_subfolders,
          watch,
          excluded_subfolders: source.excluded_subfolders,
        }),
      );
    });
  }

  async function toggleSourceTree(sourceId: number) {
    if (!client) return;
    if (openTrees.has(sourceId)) {
      openTrees.delete(sourceId);
      return;
    }
    openTrees.add(sourceId);
    const key = treeKey(sourceId, null);
    if (!subfolderLists.has(key)) {
      try {
        subfolderLists.set(key, await client.subfolders(sourceId, null));
      } catch (failure) {
        error = failure instanceof Error ? failure.message : String(failure);
      }
    }
  }

  async function toggleSubfolderExpand(sourceId: number, path: string) {
    if (!client) return;
    const expandKey = treeKey(sourceId, path);
    if (expandedSubfolders.has(expandKey)) {
      expandedSubfolders.delete(expandKey);
      return;
    }
    expandedSubfolders.add(expandKey);
    if (!subfolderLists.has(expandKey)) {
      try {
        subfolderLists.set(expandKey, await client.subfolders(sourceId, path));
      } catch (failure) {
        error = failure instanceof Error ? failure.message : String(failure);
      }
    }
  }

  async function toggleSubfolderEnabled(source: LibrarySource, path: string, enabled: boolean) {
    if (!client) return;
    const nextExcluded = enabled
      ? source.excluded_subfolders.filter((item) => item !== path)
      : [...source.excluded_subfolders, path];
    await runAction(async () => {
      status = '';
      applySources(
        await client.updateSource({
          source_id: source.id,
          include_subfolders: source.include_subfolders,
          watch: source.watch,
          excluded_subfolders: nextExcluded,
        }),
      );
      onchanged?.();
      await refreshLoadedSubfolders(source.id);
    });
  }

  function visibleNodes(
    sourceId: number,
    parent: string | null,
    depth: number,
    parentDisabled: boolean,
  ): VisibleNode[] {
    const entries = subfolderLists.get(treeKey(sourceId, parent)) ?? [];
    const nodes: VisibleNode[] = [];
    for (const entry of entries) {
      const expandKey = treeKey(sourceId, entry.path);
      const expanded = expandedSubfolders.has(expandKey);
      nodes.push({ entry, depth, parentDisabled, expanded });
      if (expanded) {
        nodes.push(...visibleNodes(sourceId, entry.path, depth + 1, !entry.enabled));
      }
    }
    return nodes;
  }

  async function addIgnored(event: SubmitEvent) {
    event.preventDefault();
    const trimmed = newIgnored.trim();
    if (!client || !trimmed) return;
    await runAction(async () => {
      status = '';
      applySources(await client.setIgnoredFolders([...ignoredFolders, trimmed]));
      newIgnored = '';
      for (const source of sources) {
        if (openTrees.has(source.id)) await refreshLoadedSubfolders(source.id);
      }
      onchanged?.();
    });
  }

  async function removeIgnored(name: string) {
    if (!client) return;
    await runAction(async () => {
      status = '';
      applySources(
        await client.setIgnoredFolders(ignoredFolders.filter((existing) => existing !== name)),
      );
      for (const source of sources) {
        if (openTrees.has(source.id)) await refreshLoadedSubfolders(source.id);
      }
      onchanged?.();
    });
  }

  async function resetIgnored() {
    if (!client) return;
    await runAction(async () => {
      status = '';
      applySources(await client.setIgnoredFolders([...DEFAULT_IGNORED]));
      for (const source of sources) {
        if (openTrees.has(source.id)) await refreshLoadedSubfolders(source.id);
      }
      onchanged?.();
    });
  }

  const defaultsChanged = $derived(
    ignoredFolders.length !== DEFAULT_IGNORED.length ||
      ignoredFolders.some((name, index) => name !== DEFAULT_IGNORED[index]),
  );
</script>

<div class="sources-settings">
  <form class="inline-form" onsubmit={addSource}>
    <div class="field-grow">
      <TextInput
        label="Add music folder"
        hideLabel
        placeholder="Paste a music folder path…"
        bind:value={newFolder}
        disabled={busy}
      />
    </div>
    <Button variant="primary" size="sm" type="submit" disabled={busy || !newFolder.trim()}>
      Add folder
    </Button>
  </form>

  {#if status}
    <p class="status numeric" role="status" aria-live="polite">{status}</p>
  {/if}
  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}

  {#if sources.length === 0}
    <p class="empty">No music folders added yet. Files on disk are never changed by a scan.</p>
  {:else}
    <div class="source-list">
      {#each sources as source (source.id)}
        <div class="source-card" role="region" aria-label={source.folder}>
          <div class="source-top">
            <div class="source-identity">
              <span class="source-path numeric">{source.folder}</span>
              <span class="source-meta numeric">
                {trackCount(source.track_count)}{#if source.watch}
                  &nbsp;· {source.watching_active ? 'Watching' : 'Watch on'}
                {/if}
              </span>
            </div>
            <div class="source-actions">
              <Button
                variant="quiet"
                size="xs"
                disabled={busy}
                ariaLabel="Rescan {source.folder}"
                onclick={() => rescan(source.folder)}
              >
                Rescan
              </Button>
              <Button
                variant="quiet"
                size="xs"
                disabled={busy}
                ariaLabel="Remove {source.folder}"
                onclick={() => remove(source.id)}
              >
                Remove
              </Button>
            </div>
          </div>

          <div class="source-toggles">
            <label class="toggle-label">
              <input
                type="checkbox"
                checked={source.include_subfolders}
                disabled={busy}
                onchange={(event) => toggleIncludeSubfolders(source, event.currentTarget.checked)}
              />
              <span>Include subfolders</span>
            </label>
            <label class="toggle-label">
              <input
                type="checkbox"
                checked={source.watch}
                disabled={busy}
                onchange={(event) => toggleWatch(source, event.currentTarget.checked)}
              />
              <span>Watch for changes</span>
            </label>
          </div>

          {#if source.include_subfolders}
            <div class="subfolder-block">
              <button
                type="button"
                class="tree-disclosure"
                aria-expanded={openTrees.has(source.id)}
                onclick={() => toggleSourceTree(source.id)}
              >
                <Icon
                  name={openTrees.has(source.id) ? 'chevron-down' : 'chevron-right'}
                  size="xs"
                />
                <span>Subfolders</span>
                {#if source.excluded_subfolders.length > 0}
                  <span class="excluded-badge numeric">
                    {source.excluded_subfolders.length} off
                  </span>
                {/if}
              </button>

              {#if openTrees.has(source.id)}
                {@const rows = visibleNodes(source.id, null, 0, false)}
                {#if rows.length === 0}
                  <p class="tree-empty">No subfolders inside this folder.</p>
                {:else}
                  <ul class="folder-tree" aria-label="Subfolders of {source.folder}">
                    {#each rows as row (row.entry.path)}
                      <li
                        class="folder-node"
                        style:padding-left="calc({row.depth} * var(--space-4))"
                      >
                        {#if row.entry.has_subfolders}
                          <button
                            type="button"
                            class="expand-btn"
                            aria-label="{row.expanded ? 'Collapse' : 'Expand'} {row.entry.path}"
                            aria-expanded={row.expanded}
                            onclick={() => toggleSubfolderExpand(source.id, row.entry.path)}
                          >
                            <Icon
                              name={row.expanded ? 'chevron-down' : 'chevron-right'}
                              size="xs"
                            />
                          </button>
                        {:else}
                          <span class="expand-spacer" aria-hidden="true"></span>
                        {/if}
                        <label class={['folder-check', { muted: !row.entry.enabled }]}>
                          <input
                            type="checkbox"
                            aria-label="Include {row.entry.path}"
                            checked={row.entry.enabled}
                            disabled={busy || row.parentDisabled}
                            onchange={(event) =>
                              toggleSubfolderEnabled(
                                source,
                                row.entry.path,
                                event.currentTarget.checked,
                              )}
                          />
                          <span>{row.entry.name}</span>
                        </label>
                      </li>
                    {/each}
                  </ul>
                {/if}
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  <div class="ignored-block">
    <div class="ignored-head">
      <MonoLabel text="Ignored folder names" tone="secondary" />
      {#if defaultsChanged}
        <Button variant="quiet" size="xs" disabled={busy} onclick={resetIgnored}>
          Reset defaults
        </Button>
      {/if}
    </div>
    <p class="hint">
      Skipped at any depth, alongside hidden folders, <span class="numeric">.resonance-ignore</span>
      and Resonance's own folder.
    </p>
    {#if ignoredFolders.length > 0}
      <div class="chips" role="list" aria-label="Ignored folder names">
        {#each ignoredFolders as name (name)}
          <div role="listitem">
            <DataTag label={name} variant="removable" onRemove={() => removeIgnored(name)} />
          </div>
        {/each}
      </div>
    {/if}
    <form class="inline-form" onsubmit={addIgnored}>
      <div class="field-grow">
        <TextInput
          label="Folder name to ignore"
          hideLabel
          placeholder="Add folder name to ignore…"
          bind:value={newIgnored}
          disabled={busy}
        />
      </div>
      <Button variant="quiet" size="sm" type="submit" disabled={busy || !newIgnored.trim()}>
        Ignore
      </Button>
    </form>
  </div>
</div>

<style>
  .sources-settings {
    display: grid;
    gap: var(--space-3);
  }

  .inline-form {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .field-grow {
    flex: 1;
    min-width: 0;
  }

  .status,
  .error,
  .empty,
  .hint,
  .tree-empty {
    margin: 0;
    font-size: var(--text-sm);
  }

  .status,
  .hint,
  .empty,
  .tree-empty {
    color: var(--color-text-2);
  }

  .error {
    color: var(--color-text);
  }

  .source-list {
    display: grid;
    gap: var(--space-2-5);
  }

  .source-card {
    display: grid;
    gap: var(--space-2-5);
    padding: var(--space-3);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface-2);
  }

  .source-top {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .source-identity {
    display: grid;
    gap: var(--space-0-5);
    min-width: 0;
  }

  .source-path {
    color: var(--color-text);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    overflow-wrap: anywhere;
  }

  .source-meta {
    color: var(--color-muted);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  .source-actions {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: var(--space-1);
  }

  .source-toggles {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2) var(--space-4);
  }

  .toggle-label,
  .folder-check {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--color-text);
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .toggle-label input,
  .folder-check input {
    accent-color: var(--color-text);
    margin: 0;
  }

  .subfolder-block {
    display: grid;
    gap: var(--space-2);
    padding-top: var(--space-1);
    border-top: var(--border-width) solid var(--color-border);
  }

  .tree-disclosure {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1-5);
    justify-self: start;
    padding: var(--space-0-5) 0;
    border: 0;
    background: transparent;
    color: var(--color-text-2);
    font-family: var(--font-sans);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    cursor: pointer;
  }

  .tree-disclosure:hover {
    color: var(--color-text);
  }

  .excluded-badge {
    padding: 0 var(--space-1-5);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-pill);
    color: var(--color-muted);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  .folder-tree {
    display: grid;
    gap: var(--space-1);
    max-height: 14rem;
    margin: 0;
    padding: 0;
    list-style: none;
    overflow: auto;
  }

  .folder-node {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    min-height: var(--control-height-xs);
  }

  .expand-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.25rem;
    height: 1.25rem;
    padding: 0;
    border: 0;
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--color-muted);
    cursor: pointer;
  }

  .expand-btn:hover {
    color: var(--color-text);
  }

  .expand-spacer {
    display: inline-block;
    width: 1.25rem;
    flex-shrink: 0;
  }

  .folder-check.muted {
    color: var(--color-muted);
  }

  .ignored-block {
    display: grid;
    gap: var(--space-2);
    padding-top: var(--space-2);
  }

  .ignored-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1-5);
  }
</style>
