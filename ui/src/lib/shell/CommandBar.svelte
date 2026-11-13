<script lang="ts" module>
  export interface Command {
    name: string;
    /** The keys that do the same thing anywhere in the app. */
    shortcut?: string;
    run: () => void;
  }
</script>

<script lang="ts">
  import TextInput from '../design/TextInput.svelte';

  interface Props {
    space: string;
    /** What the bar says about the space, such as how many tracks it holds. */
    summary: string;
    commands: Command[];
    /** The id of the field, so Ctrl+K anywhere can put the cursor in it. */
    inputId: string;
  }

  let { space, summary, commands, inputId }: Props = $props();

  const listId = $props.id();
  let query = $state('');
  let open = $state(false);
  let active = $state(0);

  const matching = $derived(
    commands.filter((command) => command.name.toLowerCase().includes(query.trim().toLowerCase())),
  );

  function close() {
    open = false;
    query = '';
    active = 0;
    document.getElementById(inputId)?.blur();
  }

  function run(command: Command) {
    close();
    command.run();
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      open = true;
      const step = event.key === 'ArrowDown' ? 1 : -1;
      active = (active + step + matching.length) % Math.max(matching.length, 1);
    } else if (event.key === 'Enter') {
      event.preventDefault();
      const command = matching[active];
      if (command) run(command);
    } else if (event.key === 'Escape') {
      event.preventDefault();
      close();
    }
  }
</script>

<header class="bar">
  <h1 class="space">{space}</h1>
  <span class="summary numeric">{summary}</span>
  <div class="palette">
    <TextInput
      id={inputId}
      label="Commands"
      hideLabel
      placeholder="Commands"
      autocomplete="off"
      style="padding-right: 3.5rem"
      role="combobox"
      aria-expanded={open}
      aria-controls={listId}
      aria-activedescendant={open && matching[active] ? `${listId}-${active}` : undefined}
      bind:value={query}
      onfocus={() => (open = true)}
      onblur={() => (open = false)}
      oninput={() => {
        open = true;
        active = 0;
      }}
      {onkeydown}
    />
    <kbd class="hint numeric" aria-hidden="true">Ctrl K</kbd>
    {#if open}
      <ul class="list" id={listId} role="listbox" aria-label="Commands">
        {#each matching as command, index (command.name)}
          <li
            id="{listId}-{index}"
            class={['option', { active: index === active }]}
            role="option"
            aria-selected={index === active}
            onmousedown={(event) => {
              event.preventDefault();
              run(command);
            }}
            onmouseenter={() => (active = index)}
          >
            <span>{command.name}</span>
            {#if command.shortcut}
              <kbd class="numeric">{command.shortcut}</kbd>
            {/if}
          </li>
        {:else}
          <li class="none">No command matches.</li>
        {/each}
      </ul>
    {/if}
  </div>
</header>

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: 44px;
    padding: 0 var(--space-3) 0 var(--space-4);
    border-bottom: 1px solid var(--color-border);
    background: var(--color-surface);
  }

  .space {
    font-size: var(--text-lg);
  }

  .summary {
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }

  .palette {
    position: relative;
    width: min(24rem, 45%);
    margin-left: auto;
  }

  .hint {
    position: absolute;
    top: 50%;
    right: var(--space-2);
    color: var(--color-muted);
    font-size: var(--text-xs);
    transform: translateY(-50%);
    pointer-events: none;
  }

  .list {
    position: absolute;
    top: calc(100% + var(--space-1));
    right: 0;
    left: 0;
    z-index: 20;
    margin: 0;
    padding: var(--space-1);
    border: 1px solid var(--color-border-control);
    border-radius: var(--radius-panel);
    background: var(--color-raised);
    list-style: none;
  }

  .option,
  .none {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: var(--row-height);
    padding: 0 var(--space-2);
    border-radius: var(--radius-control);
  }

  .option {
    cursor: default;
  }

  .active {
    background: var(--color-surface-2);
  }

  .none {
    color: var(--color-text-2);
  }

  kbd {
    color: var(--color-text-2);
    font-size: var(--text-xs);
  }
</style>
