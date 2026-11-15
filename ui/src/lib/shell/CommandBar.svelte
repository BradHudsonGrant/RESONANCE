<script lang="ts" module>
  /** A command runs now, or names the milestone that builds it and cannot be chosen. */
  export type Command = { name: string; shortcut?: string } & (
    { run: () => void; arrives?: undefined } | { arrives: string; run?: undefined }
  );
</script>

<script lang="ts">
  import Button from '../design/Button.svelte';
  import DataTag from '../design/DataTag.svelte';
  import Icon from '../design/Icon.svelte';
  import SlashChip from '../design/SlashChip.svelte';
  import TextInput from '../design/TextInput.svelte';
  import Tooltip from '../design/Tooltip.svelte';

  interface Props {
    commands: Command[];
    /** The id of the field, so Ctrl+K anywhere can put the cursor in it. */
    inputId: string;
    search?: string;
    bpmActive?: boolean;
    onsearch?: (text: string) => void;
    onbpm?: (min: number | null, max: number | null) => void;
  }

  let { commands, inputId, search = '', bpmActive = false, onsearch, onbpm }: Props = $props();

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

  function parseBpmCommand(text: string): Command | null {
    const trimmed = text.trim();
    const match = /^\/\s*bpm(?:\s+(.*))?$/i.exec(trimmed);
    if (!match) return null;
    const arg = (match[1] ?? '').trim();
    if (/^(clear|reset)$/i.test(arg)) {
      return {
        name: '/ bpm clear — Remove BPM filter',
        run: () => onbpm?.(null, null),
      };
    }
    const range = /^(\d+(?:\.\d+)?)\s*[-–]\s*(\d+(?:\.\d+)?)$/.exec(arg);
    if (range) {
      const a = Number(range[1]);
      const b = Number(range[2]);
      if (a > 0 && b > 0) {
        const min = Math.min(a, b);
        const max = Math.max(a, b);
        return {
          name: `/ bpm ${min}–${max} — Filter tracks by tempo`,
          run: () => onbpm?.(min, max),
        };
      }
    }
    const minOnly = /^(\d+(?:\.\d+)?)\s*[-–]$/.exec(arg);
    if (minOnly) {
      const min = Number(minOnly[1]);
      if (min > 0) {
        return {
          name: `/ bpm ${min}+ — Filter tracks at or above ${min} BPM`,
          run: () => onbpm?.(min, null),
        };
      }
    }
    const maxOnly = /^[-–]\s*(\d+(?:\.\d+)?)$/.exec(arg);
    if (maxOnly) {
      const max = Number(maxOnly[1]);
      if (max > 0) {
        return {
          name: `/ bpm ≤${max} — Filter tracks up to ${max} BPM`,
          run: () => onbpm?.(null, max),
        };
      }
    }
    const exact = /^(\d+(?:\.\d+)?)$/.exec(arg);
    if (exact) {
      const value = Number(exact[1]);
      if (value > 0) {
        return {
          name: `/ bpm ${value} — Filter tracks at ${value} BPM`,
          run: () => onbpm?.(value, value),
        };
      }
    }
    return {
      name: '/ bpm 125-132 — Filter tracks by tempo (e.g. 125-132)',
      run: () => onbpm?.(125, 132),
    };
  }

  const slashMode = $derived(query.trim().startsWith('/'));
  const bpmCommand = $derived(parseBpmCommand(query));
  const matching = $derived.by(() => {
    const trimmed = query.trim().toLowerCase();
    if (slashMode) {
      const list: Command[] = [];
      if (bpmCommand) {
        list.push(bpmCommand);
      } else if (trimmed === '/' || '/ bpm'.startsWith(trimmed)) {
        list.push({
          name: '/ bpm 125-132 — Filter tracks by tempo',
          run: () => onbpm?.(125, 132),
        });
      }
      if (trimmed !== '/' && bpmCommand && bpmActive) {
        list.push({
          name: '/ bpm clear — Remove BPM filter',
          run: () => onbpm?.(null, null),
        });
      }
      return list;
    }
    if (!trimmed) return commands;
    return commands.filter((command) => command.name.toLowerCase().includes(trimmed));
  });
  const open = $derived(focused && (slashMode || query.trim() === '' || matching.length > 0));

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

  function activateBpmChip() {
    query = '/ bpm 125-132';
    focused = true;
    active = 0;
    const el = document.getElementById(inputId) as HTMLInputElement | null;
    el?.focus();
    el?.setSelectionRange(6, query.length);
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
            aria-disabled={command.arrives ? 'true' : undefined}
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
        command="/ bpm 125-132"
        description="Filter by tempo"
        active={bpmActive}
        onclick={activateBpmChip}
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
