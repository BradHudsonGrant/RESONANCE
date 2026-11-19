<script lang="ts" module>
  /**
   * A command runs now, names the milestone that builds it, or is a note that cannot be chosen,
   * such as why the text is not a filter.
   */
  export type Command = { name: string; shortcut?: string } & (
    | { run: () => void; arrives?: undefined; note?: undefined }
    | { arrives: string; run?: undefined; note?: undefined }
    | { note: true; run?: undefined; arrives?: undefined }
  );
</script>

<script lang="ts">
  import { untrack } from 'svelte';

  import Button from '../design/Button.svelte';
  import DataTag from '../design/DataTag.svelte';
  import Icon from '../design/Icon.svelte';
  import SlashChip from '../design/SlashChip.svelte';
  import TextInput from '../design/TextInput.svelte';
  import type { DescribedFilter } from '../ipc/bindings/DescribedFilter';
  import Tooltip from '../design/Tooltip.svelte';

  interface Props {
    commands: Command[];
    /** The id of the field, so Ctrl+K anywhere can put the cursor in it. */
    inputId: string;
    search?: string;
    /** Reads a `/` filter in the core. */
    parseFilter?: (text: string) => Promise<DescribedFilter>;
    onsearch?: (text: string) => void;
    onfilter?: (filter: DescribedFilter) => void;
  }

  let { commands, inputId, search = '', parseFilter, onsearch, onfilter }: Props = $props();

  const examples = [
    { text: 'missing genre', description: 'Tracks with no genre' },
    { text: 'bitrate < 320', description: 'Files under 320 kbps' },
    { text: 'format flac', description: 'Only FLAC files' },
    { text: 'bpm 125-132', description: 'Tracks by tempo' },
  ];

  const futureChips = [
    { command: '/ key 8A', description: 'Compatible with A minor', arrives: 'M5' },
    { command: '/ similar', description: 'Find matching tracks', arrives: 'M6' },
    { command: '/ build set', description: 'Generate 90m progression', arrives: 'M10' },
  ];

  const shortcutBadge = /Mac/.test(navigator.userAgent) ? '⌘ K' : 'Ctrl K';
  const listId = $props.id();
  let query = $state('');
  let lastSearchProp = '';
  let focused = $state(false);
  let active = $state(0);

  $effect(() => {
    if (search !== lastSearchProp) {
      lastSearchProp = search;
      if (!query.trim().startsWith('/')) query = search;
    }
  });

  // The filter the slash text makes, read by the core. One read is on the wire at a time and
  // only the newest text waits behind it, with no timer.
  let parsed = $state<{ text: string; filter: DescribedFilter | null; error: string | null }>({
    text: '',
    filter: null,
    error: null,
  });
  let parsing = false;
  let parseWaiting: string | null = null;

  async function readFilter(text: string) {
    if (!parseFilter) return;
    if (parsing) {
      parseWaiting = text;
      return;
    }
    parsing = true;
    let next: string | null = text;
    while (next !== null) {
      const asked = next;
      parseWaiting = null;
      try {
        const filter = await parseFilter(asked);
        parsed = { text: asked, filter, error: null };
      } catch (error) {
        parsed = {
          text: asked,
          filter: null,
          error: error instanceof Error ? error.message : String(error),
        };
      }
      next = parseWaiting;
    }
    parsing = false;
  }

  const slashMode = $derived(query.trim().startsWith('/'));
  const slashText = $derived(query.trim().replace(/^\/\s*/, ''));
  const matching = $derived.by((): Command[] => {
    const trimmed = query.trim().toLowerCase();
    if (slashMode) {
      if (slashText === '') {
        return examples.map((example): Command => ({
          name: `/ ${example.text} — ${example.description}`,
          run: () => startExample(example.text),
        }));
      }
      const current = parsed.text === slashText ? parsed : null;
      if (current?.filter) {
        const filter = current.filter;
        return [{ name: `Filter: ${filter.label}`, run: () => onfilter?.(filter) }];
      }
      return [{ name: current?.error ?? 'Reading the filter…', note: true }];
    }
    if (!trimmed) return commands;
    return commands.filter((command) => command.name.toLowerCase().includes(trimmed));
  });
  const open = $derived(focused && (slashMode || query.trim() === '' || matching.length > 0));

  // A filter read after the text was typed turns the list's note into a command, so the
  // highlight moves onto it.
  $effect(() => {
    void matching;
    untrack(() => {
      if (!matching[active]?.run) active = nextRunnable(0, 1);
    });
  });

  /** The nearest command that can run, from `from` in the direction of `step`. */
  function nextRunnable(from: number, step: 1 | -1): number {
    const count = matching.length;
    for (let tried = 0; tried < count; tried++) {
      const index = (((from + step * tried) % count) + count) % count;
      if (matching[index]?.run) return index;
    }
    return -1;
  }

  function reset() {
    focused = true;
    active = nextRunnable(0, 1);
  }

  function oninput() {
    reset();
    if (!query.trim().startsWith('/')) {
      onsearch?.(query);
    } else if (slashText !== '') {
      void readFilter(slashText);
    }
  }

  function clearAndBlur() {
    focused = false;
    if (query.trim().startsWith('/')) {
      query = search;
    } else if (query !== '') {
      query = '';
      onsearch?.('');
    }
    document.getElementById(inputId)?.blur();
  }

  function choose(command: Command | undefined) {
    if (!command?.run) return;
    const wasSlash = slashMode;
    focused = false;
    if (wasSlash) {
      query = search;
    } else {
      query = '';
      onsearch?.('');
    }
    document.getElementById(inputId)?.blur();
    command.run();
  }

  /** Puts an example in the field with its value selected, ready to be typed over. */
  function startExample(text: string) {
    query = `/ ${text}`;
    focused = true;
    active = 0;
    void readFilter(text);
    const el = document.getElementById(inputId) as HTMLInputElement | null;
    el?.focus();
    const space = text.lastIndexOf(' ');
    el?.setSelectionRange(space >= 0 ? space + 3 : 2, query.length);
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      if (!open) return;
      event.preventDefault();
      const step = event.key === 'ArrowDown' ? 1 : -1;
      active = nextRunnable(active + step, step);
    } else if (event.key === 'Enter') {
      if (!open || active < 0) return;
      event.preventDefault();
      choose(matching[active]);
    } else if (event.key === 'Escape') {
      event.preventDefault();
      clearAndBlur();
    }
  }
</script>

<header class="command-bar">
  <div class="palette">
    <TextInput
      id={inputId}
      label="Commands"
      hideLabel
      size="lg"
      leadingIcon="search"
      {shortcutBadge}
      placeholder="Search your library, or type a command…"
      autocomplete="off"
      role="combobox"
      aria-expanded={open}
      aria-controls={listId}
      aria-activedescendant={open && active >= 0 ? `${listId}-${active}` : undefined}
      bind:value={query}
      onfocus={reset}
      onblur={() => (focused = false)}
      {oninput}
      {onkeydown}
    />
    {#if open}
      <ul class="list" id={listId} role="listbox" aria-label="Commands">
        {#each matching as command, index (command.name)}
          <li
            id="{listId}-{index}"
            class={['option', { active: index === active }]}
            role="option"
            aria-selected={index === active}
            aria-disabled={command.run ? undefined : 'true'}
            onmousedown={(event) => {
              event.preventDefault();
              choose(command);
            }}
            onmouseenter={() => {
              if (command.run) active = index;
            }}
          >
            <span>{command.name}</span>
            {#if command.arrives}
              <DataTag variant="rect" label="Arrives in {command.arrives}" />
            {:else if command.shortcut}
              <kbd class="numeric">{command.shortcut}</kbd>
            {/if}
          </li>
        {:else}
          <li class="none">No command matches.</li>
        {/each}
      </ul>
    {/if}
  </div>

  <div class="strip">
    <div class="chips">
      <!-- Only the first chip takes focus, so Tab does not stop on every placeholder. -->
      <SlashChip
        command="/ missing genre"
        description="Filter on any column"
        onclick={() => startExample('missing genre')}
      />
      {#each futureChips as chip (chip.command)}
        <Tooltip text="{chip.description} arrives in {chip.arrives}" side="below">
          <SlashChip command={chip.command} description={chip.description} disabled tabindex={-1} />
        </Tooltip>
      {/each}
    </div>
    <Tooltip text="More slash commands arrive in M5" side="left">
      <Button
        variant="icon"
        size="sm"
        aria-label="More slash commands"
        aria-disabled="true"
        tabindex={-1}
      >
        <Icon name="more" size="xs" />
      </Button>
    </Tooltip>
  </div>
</header>

<style>
  .command-bar {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: var(--space-3);
    padding: var(--space-3) var(--space-5) var(--space-3-5);
    border-bottom: var(--border-width) solid var(--color-border);
  }

  .palette,
  .strip {
    width: 100%;
    max-width: var(--command-bar-max-width);
  }

  .palette {
    position: relative;
  }

  .list {
    position: absolute;
    top: calc(100% + var(--space-1));
    right: 0;
    left: 0;
    z-index: 20;
    margin: 0;
    padding: var(--space-1);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-lg);
    background: var(--color-raised);
    list-style: none;
  }

  .option,
  .none {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: var(--row-height);
    padding: 0 var(--space-2-5);
    border-radius: var(--radius-sm);
  }

  .option {
    cursor: default;
  }

  .option.active {
    background: var(--color-surface-2);
  }

  .option[aria-disabled='true'],
  .none {
    color: var(--color-muted);
  }

  kbd {
    color: var(--color-text-2);
    font-size: var(--text-xs);
  }

  .strip {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
  }

  /* One row high: a chip that does not fit wraps onto the hidden second row. */
  .chips {
    display: flex;
    flex-wrap: wrap;
    column-gap: var(--space-3);
    min-width: 0;
    height: var(--control-height-xs);
    overflow: hidden;
  }
</style>
