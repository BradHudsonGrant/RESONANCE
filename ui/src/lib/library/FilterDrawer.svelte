<script lang="ts" module>
  import type { FilterCondition } from '../ipc/bindings/FilterCondition';
  import type { FilterConditionName } from '../ipc/bindings/FilterConditionName';
  import type { FilterField } from '../ipc/bindings/FilterField';
  import type { FilterTerm } from '../ipc/bindings/FilterTerm';

  /** One condition as the builder edits it: values as typed, in the units the field shows. */
  export interface ConditionDraft {
    column: string;
    op: FilterConditionName;
    text: string;
    values: string[];
    min: string;
    max: string;
  }

  /** One AND term being edited: one condition, or an "any of these" group. */
  export interface TermDraft {
    id: number;
    group: boolean;
    members: ConditionDraft[];
  }

  /** How many stored units one shown unit is, for the fields that show another unit. */
  function unitScale(column: string): number {
    if (column === 'file_size') return 1_000_000;
    if (column === 'sample_rate') return 1000;
    return 1;
  }

  /** A number as typed: `5:30` for a time, a date for a date field. */
  export function storedNumber(field: FilterField, raw: string): number | null {
    const text = raw.trim();
    if (!text) return null;
    if (field.kind === 'date') {
      const ms = Date.parse(`${text}T00:00:00Z`);
      return Number.isFinite(ms) ? ms : null;
    }
    if (field.column === 'time' && text.includes(':')) {
      const [minutes, seconds] = text.split(':');
      const total = Number(minutes) * 60 + Number(seconds);
      return Number.isFinite(total) ? total : null;
    }
    const value = Number(text);
    return Number.isFinite(value) ? value * unitScale(field.column) : null;
  }

  export function shownNumber(field: FilterField, value: number | null | undefined): string {
    if (value == null) return '';
    if (field.kind === 'date') return new Date(value).toISOString().slice(0, 10);
    if (field.column === 'time') {
      const total = Math.round(value);
      return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, '0')}`;
    }
    return String(value / unitScale(field.column));
  }

  /** The condition a draft makes, or nothing while it is incomplete. */
  export function draftCondition(
    field: FilterField,
    draft: ConditionDraft,
  ): FilterCondition | null {
    const text = draft.text.trim();
    const list = (field.kind === 'choice' ? draft.values : draft.text.split(','))
      .map((value) => value.trim())
      .filter(Boolean);
    switch (draft.op) {
      case 'missing':
      case 'present':
        return { op: draft.op };
      case 'contains':
      case 'is':
      case 'is_not':
      case 'starts_with':
        return text ? { op: draft.op, value: text } : null;
      case 'any_of':
      case 'none_of':
        return list.length > 0 ? { op: draft.op, values: list } : null;
      case 'range': {
        const min = storedNumber(field, draft.min);
        const max = storedNumber(field, draft.max);
        return min == null && max == null ? null : { op: 'range', min, max };
      }
      case 'below':
      case 'above': {
        const value = storedNumber(field, draft.text);
        return value == null ? null : { op: draft.op, value };
      }
    }
  }

  export function emptyDraft(field: FilterField): ConditionDraft {
    return {
      column: field.column,
      op: field.conditions[0] ?? 'missing',
      text: '',
      values: [],
      min: '',
      max: '',
    };
  }

  /** A term from the query as the builder shows it. */
  export function draftOf(id: number, term: FilterTerm, fields: FilterField[]): TermDraft {
    return {
      id,
      group: term.any_of.length > 1,
      members: term.any_of.map((filter) => {
        const field = fields.find((item) => item.column === filter.column);
        const condition = filter.condition;
        const draft: ConditionDraft = {
          column: filter.column,
          op: condition.op,
          text: '',
          values: [],
          min: '',
          max: '',
        };
        if (!field) return draft;
        if ('value' in condition) {
          draft.text =
            typeof condition.value === 'number'
              ? shownNumber(field, condition.value)
              : condition.value;
        }
        if ('values' in condition) {
          if (field.kind === 'choice') draft.values = condition.values;
          else draft.text = condition.values.join(', ');
        }
        if (condition.op === 'range') {
          draft.min = shownNumber(field, condition.min);
          draft.max = shownNumber(field, condition.max);
        }
        return draft;
      }),
    };
  }
</script>

<script lang="ts">
  import { untrack } from 'svelte';

  import Button from '../design/Button.svelte';
  import Icon from '../design/Icon.svelte';
  import ModularDivider from '../design/ModularDivider.svelte';
  import MonoLabel from '../design/MonoLabel.svelte';
  import Sheet from '../design/Sheet.svelte';
  import { KEY_WHEEL } from '../ipc/bindings/KeyWheel';
  import type { LibraryFacets } from '../ipc/bindings/LibraryFacets';
  import type { TableColumnGroup } from '../ipc/bindings/TableColumnGroup';
  import type { TrackQuery } from '../ipc/bindings/TrackQuery';
  import { termsOf, withTerms } from './filters.svelte';
  import { keyCode, keySecond } from './key-notation.svelte';

  interface Props {
    open: boolean;
    query: TrackQuery;
    fields: FilterField[];
    facets: LibraryFacets | null;
    /** A column to start a new condition on when the drawer opens. */
    startColumn?: string | null;
    onchange: (next: TrackQuery) => void;
    /** Terms added while the drawer was open, counted once when it closes. */
    onadded?: (terms: FilterTerm[]) => void;
    onclear: () => void;
  }

  let {
    open = $bindable(false),
    query,
    fields,
    facets,
    startColumn = null,
    onchange,
    onadded,
    onclear,
  }: Props = $props();

  const groups: { id: TableColumnGroup; label: string }[] = [
    { id: 'track', label: 'Track' },
    { id: 'release', label: 'Release' },
    { id: 'audio', label: 'Audio' },
    { id: 'file', label: 'File' },
    { id: 'tags', label: 'Tags' },
  ];

  const opWords: Record<FilterConditionName, string> = {
    contains: 'contains',
    is: 'is',
    is_not: 'is not',
    starts_with: 'starts with',
    any_of: 'is any of',
    none_of: 'is none of',
    range: 'is between',
    below: 'is under',
    above: 'is over',
    missing: 'is missing',
    present: 'is present',
  };

  const baseId = $props.id();
  let drafts = $state<TermDraft[]>([]);
  let nextId = 0;
  let openedWith: string[] = [];
  let wasOpen = false;

  const byColumn = $derived(new Map(fields.map((field) => [field.column, field])));
  const activeCount = $derived(termsOf(query).length + (query.missing_only ? 1 : 0));

  function opWord(field: FilterField, op: FilterConditionName): string {
    if (field.kind === 'date' && op === 'below') return 'is before';
    if (field.kind === 'date' && op === 'above') return 'is after';
    return opWords[op];
  }

  function unit(field: FilterField): string {
    switch (field.column) {
      case 'bitrate':
        return 'kbps';
      case 'sample_rate':
        return 'kHz';
      case 'file_size':
        return 'MB';
      case 'bit_depth':
        return 'bit';
      case 'time':
        return 'm:ss';
      default:
        return '';
    }
  }

  // Opening reads the query's terms into drafts, and closing counts the terms that are new.
  $effect(() => {
    const isOpen = open;
    untrack(() => {
      if (isOpen && !wasOpen) {
        const terms = termsOf(query);
        openedWith = terms.map((term) => JSON.stringify(term));
        drafts = terms.map((term) => draftOf(nextId++, term, fields));
        const start = startColumn ? byColumn.get(startColumn) : undefined;
        if (start)
          drafts = [...drafts, { id: nextId++, group: false, members: [emptyDraft(start)] }];
      } else if (!isOpen && wasOpen) {
        const added = termsOf(query).filter((term) => !openedWith.includes(JSON.stringify(term)));
        if (added.length > 0) onadded?.(added);
      }
      wasOpen = isOpen;
    });
  });

  function termOf(draft: TermDraft): FilterTerm | null {
    const members = [];
    for (const member of draft.members) {
      const field = byColumn.get(member.column);
      const condition = field ? draftCondition(field, member) : null;
      if (!condition) return null;
      members.push({ column: member.column, condition });
    }
    return members.length > 0 ? { any_of: members } : null;
  }

  /** Puts every complete draft on the table. Incomplete ones wait here. */
  function apply() {
    const terms = drafts.map(termOf).filter((term): term is FilterTerm => term !== null);
    const next = withTerms(query, terms);
    if (JSON.stringify(next.filters ?? null) !== JSON.stringify(query.filters ?? null)) {
      onchange(next);
    }
  }

  function update(draftId: number, index: number, patch: Partial<ConditionDraft>) {
    drafts = drafts.map((draft) =>
      draft.id !== draftId
        ? draft
        : {
            ...draft,
            members: draft.members.map((member, at) =>
              at === index ? { ...member, ...patch } : member,
            ),
          },
    );
    apply();
  }

  function changeColumn(draftId: number, index: number, column: string) {
    const field = byColumn.get(column);
    if (field) update(draftId, index, emptyDraft(field));
  }

  function changeOp(draftId: number, index: number, op: FilterConditionName) {
    update(draftId, index, { op });
  }

  function toggleValue(draftId: number, index: number, member: ConditionDraft, value: string) {
    const values = member.values.includes(value)
      ? member.values.filter((item) => item !== value)
      : [...member.values, value];
    update(draftId, index, { values });
  }

  function addCondition(group: boolean) {
    const first = fields[0];
    if (!first) return;
    drafts = [
      ...drafts,
      {
        id: nextId++,
        group,
        members: group ? [emptyDraft(first), emptyDraft(first)] : [emptyDraft(first)],
      },
    ];
  }

  function addToGroup(draftId: number) {
    const first = fields[0];
    if (!first) return;
    drafts = drafts.map((draft) =>
      draft.id === draftId ? { ...draft, members: [...draft.members, emptyDraft(first)] } : draft,
    );
  }

  function removeMember(draftId: number, index: number) {
    drafts = drafts
      .map((draft) =>
        draft.id === draftId
          ? { ...draft, members: draft.members.filter((_, at) => at !== index) }
          : draft,
      )
      .filter((draft) => draft.members.length > 0);
    apply();
  }

  function clearAll() {
    drafts = [];
    onclear();
  }
</script>

{#snippet memberEditor(draft: TermDraft, member: ConditionDraft, index: number)}
  {@const field = byColumn.get(member.column)}
  {@const rowId = `${baseId}-${draft.id}-${index}`}
  <div class="condition" role="group" aria-label="Condition {index + 1}">
    <div class="condition-head">
      <select
        class="field select"
        aria-label="Column"
        value={member.column}
        onchange={(event) => changeColumn(draft.id, index, event.currentTarget.value)}
      >
        {#each groups as group (group.id)}
          {@const inGroup = fields.filter((item) => item.group === group.id)}
          {#if inGroup.length > 0}
            <optgroup label={group.label}>
              {#each inGroup as item (item.column)}
                <option value={item.column}>{item.label}</option>
              {/each}
            </optgroup>
          {/if}
        {/each}
      </select>
      <button
        type="button"
        class="remove"
        aria-label="Remove this condition"
        onclick={() => removeMember(draft.id, index)}
      >
        <Icon name="close" size="xs" />
      </button>
    </div>
    {#if field}
      <select
        class="field select"
        aria-label="Condition"
        value={member.op}
        onchange={(event) =>
          changeOp(draft.id, index, event.currentTarget.value as FilterConditionName)}
      >
        {#each field.conditions as op (op)}
          <option value={op}>{opWord(field, op)}</option>
        {/each}
      </select>
      {#if member.op === 'missing' || member.op === 'present'}
        <!-- Nothing to type. -->
      {:else if field.kind === 'choice'}
        <div
          class={['choices', { keys: field.column === 'key' }]}
          role="group"
          aria-label="{field.label} values"
        >
          {#each field.choices as choice (choice.value)}
            {@const wheel = KEY_WHEEL.find((item) => item.camelot === choice.value)}
            {@const pressed = member.values.includes(choice.value)}
            <button
              type="button"
              class={['choice', { pressed }]}
              aria-pressed={pressed}
              aria-label={wheel ? `${keyCode(wheel)} (${keySecond(wheel)})` : undefined}
              title={wheel ? `${keyCode(wheel)} · ${keySecond(wheel)}` : undefined}
              onclick={() => toggleValue(draft.id, index, member, choice.value)}
            >
              <!-- Keys and formats are codes; quality flags are words. -->
              <span class={{ numeric: field.column !== 'quality' }}
                >{wheel ? keyCode(wheel) : choice.label}</span
              >
            </button>
          {/each}
        </div>
      {:else if member.op === 'range'}
        <div class="range-pair">
          <label class="range-field" for="{rowId}-min">
            <span class="range-caption">From {unit(field)}</span>
            <input
              id="{rowId}-min"
              class="field numeric"
              type={field.kind === 'date' ? 'date' : 'text'}
              inputmode="decimal"
              value={member.min}
              oninput={(event) => update(draft.id, index, { min: event.currentTarget.value })}
            />
          </label>
          <label class="range-field" for="{rowId}-max">
            <span class="range-caption">To {unit(field)}</span>
            <input
              id="{rowId}-max"
              class="field numeric"
              type={field.kind === 'date' ? 'date' : 'text'}
              inputmode="decimal"
              value={member.max}
              oninput={(event) => update(draft.id, index, { max: event.currentTarget.value })}
            />
          </label>
        </div>
      {:else}
        {@const numeric = member.op === 'below' || member.op === 'above'}
        <input
          class={['field', { numeric }]}
          type={numeric && field.kind === 'date' ? 'date' : 'text'}
          inputmode={numeric ? 'decimal' : undefined}
          aria-label={member.op === 'any_of' || member.op === 'none_of'
            ? `${field.label} values, separated by commas`
            : `${field.label} value`}
          placeholder={member.op === 'any_of' || member.op === 'none_of'
            ? 'House, Techno'
            : numeric
              ? unit(field)
              : ''}
          value={member.text}
          oninput={(event) => update(draft.id, index, { text: event.currentTarget.value })}
        />
      {/if}
    {/if}
  </div>
{/snippet}

<Sheet bind:open variant="drawer" title="Filters">
  <div class="drawer-content">
    {#each drafts as draft, at (draft.id)}
      {#if at > 0}<ModularDivider />{/if}
      <section class="term" aria-label={draft.group ? 'Any of these' : 'Condition'}>
        {#if draft.group}
          <MonoLabel text="Any of these" tone="secondary" />
        {/if}
        {#each draft.members as member, index (index)}
          {#if index > 0}<span class="or" aria-hidden="true">or</span>{/if}
          {@render memberEditor(draft, member, index)}
        {/each}
        {#if draft.group}
          <Button variant="quiet" size="sm" onclick={() => addToGroup(draft.id)}>
            Add another
          </Button>
        {/if}
      </section>
    {:else}
      <p class="empty">No filters yet. Add a condition on any column.</p>
    {/each}

    <div class="adders">
      <Button size="sm" onclick={() => addCondition(false)}>
        <Icon name="plus" size="xs" />
        Add a condition
      </Button>
      <Button variant="quiet" size="sm" onclick={() => addCondition(true)}>
        Add an any-of group
      </Button>
    </div>

    <ModularDivider />

    <section class="term">
      <div class="section-head">
        <MonoLabel text="File status" tone="secondary" />
        {#if facets != null}
          <span class="bounds numeric">{facets.missing_count} missing</span>
        {/if}
      </div>
      <label class="check">
        <input
          type="checkbox"
          checked={Boolean(query.missing_only)}
          onchange={() => onchange({ ...query, missing_only: query.missing_only ? null : true })}
        />
        <span>Missing files only</span>
      </label>
    </section>
  </div>

  {#snippet actions()}
    <Button variant="quiet" disabled={activeCount === 0 && drafts.length === 0} onclick={clearAll}>
      Clear
    </Button>
    <Button variant="primary" onclick={() => (open = false)}>Done</Button>
  {/snippet}
</Sheet>

<style>
  .drawer-content {
    display: grid;
    gap: var(--space-4);
  }

  .term {
    display: grid;
    gap: var(--space-3);
  }

  .condition {
    display: grid;
    gap: var(--space-2-5);
  }

  .condition-head {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--space-2);
  }

  .or {
    color: var(--color-muted);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  .section-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .bounds,
  .empty {
    color: var(--color-muted);
    font-size: var(--text-xs);
  }

  .empty {
    margin: 0;
    font-size: var(--text-sm);
  }

  .adders {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .range-pair {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-2);
  }

  .range-field {
    display: grid;
    gap: var(--space-1);
  }

  .range-caption {
    color: var(--color-text-2);
    font-size: var(--text-xs);
  }

  .field {
    width: 100%;
    height: var(--control-height-md);
    padding: 0 var(--space-2-5);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    font-size: var(--text-md);
  }

  .field:focus-visible {
    border-color: var(--color-text);
  }

  .remove {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--control-height-sm);
    height: var(--control-height-sm);
    padding: 0;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text-2);
    cursor: pointer;
  }

  .remove:hover {
    background: var(--color-surface-2);
    color: var(--color-text);
  }

  .choices {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .choices.keys {
    display: grid;
    grid-template-columns: repeat(6, minmax(0, 1fr));
  }

  .choice {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 3.25rem;
    min-height: var(--control-height-sm);
    padding: 0 var(--space-2-5);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-text-2);
    font-size: var(--text-sm);
    cursor: pointer;
    transition:
      background-color var(--duration-fast) var(--ease-out),
      border-color var(--duration-fast) var(--ease-out),
      color var(--duration-fast) var(--ease-out);
  }

  .choices.keys .choice {
    min-width: 0;
  }

  .choice:hover {
    background: var(--color-surface-2);
    color: var(--color-text);
  }

  .choice.pressed {
    border-color: var(--color-text);
    background: var(--color-text);
    color: var(--color-background);
  }

  .check {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1-5);
    color: var(--color-text);
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .check input {
    width: var(--checkbox-size);
    height: var(--checkbox-size);
    margin: 0;
    accent-color: var(--color-text);
  }

  @media (forced-colors: active) {
    .choice.pressed {
      border-color: Highlight;
      background: Highlight;
      color: HighlightText;
      forced-color-adjust: none;
    }
  }
</style>
