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
  }

  let { commands, inputId }: Props = $props();

  const slashCommands = [
    { command: '/ bpm 125-132', description: 'Filter by tempo', arrives: 'M2' },
    { command: '/ key 8A', description: 'Compatible with A minor', arrives: 'M5' },
    { command: '/ similar', description: 'Find matching tracks', arrives: 'M6' },
    { command: '/ build set', description: 'Generate 90m progression', arrives: 'M10' },
  ];

  const shortcutBadge = /Mac/.test(navigator.userAgent) ? '⌘ K' : 'Ctrl K';
  const listId = $props.id();
  let query = $state('');
  let open = $state(false);
  let active = $state(0);

  const matching = $derived(
    commands.filter((command) => command.name.toLowerCase().includes(query.trim().toLowerCase())),
  );

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
    open = true;
    active = nextRunnable(0, 1);
  }

  function close() {
    open = false;
    query = '';
    document.getElementById(inputId)?.blur();
  }

  function choose(command: Command | undefined) {
    if (!command?.run) return;
    close();
    command.run();
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      const step = event.key === 'ArrowDown' ? 1 : -1;
      if (open) active = nextRunnable(active + step, step);
      else reset();
    } else if (event.key === 'Enter') {
      event.preventDefault();
      choose(matching[active]);
    } else if (event.key === 'Escape') {
      event.preventDefault();
      close();
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
      placeholder="Type a command…"
      autocomplete="off"
      role="combobox"
      aria-expanded={open}
      aria-controls={listId}
      aria-activedescendant={open && active >= 0 ? `${listId}-${active}` : undefined}
      bind:value={query}
      onfocus={reset}
      onblur={() => (open = false)}
      oninput={reset}
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
      {#each slashCommands as chip, index (chip.command)}
        <Tooltip text="{chip.description} arrives in {chip.arrives}" side="below">
          <SlashChip
            command={chip.command}
            description={chip.description}
            disabled
            tabindex={index === 0 ? undefined : -1}
          />
        </Tooltip>
      {/each}
    </div>
    <Tooltip text="More slash commands arrive in M2" side="left">
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
