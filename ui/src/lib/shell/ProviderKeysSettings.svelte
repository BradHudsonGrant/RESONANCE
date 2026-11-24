<script lang="ts" module>
  import type { IdentificationSettings } from '../ipc/bindings/IdentificationSettings';
  import type { ProviderKeyKind } from '../ipc/bindings/ProviderKeyKind';
  import type { ProviderKeyStatus } from '../ipc/bindings/ProviderKeyStatus';

  export interface ProviderKeysClient {
    status: () => Promise<ProviderKeyStatus[]>;
    save: (provider: ProviderKeyKind, value: string) => Promise<ProviderKeyStatus>;
    clear: (provider: ProviderKeyKind) => Promise<ProviderKeyStatus>;
    identificationSettings?: () => Promise<IdentificationSettings>;
    saveIdentificationSettings?: (
      settings: IdentificationSettings,
    ) => Promise<IdentificationSettings>;
  }
</script>

<script lang="ts">
  import Button from '../design/Button.svelte';
  import DataTag from '../design/DataTag.svelte';
  import Icon from '../design/Icon.svelte';
  import ModularDivider from '../design/ModularDivider.svelte';
  import TextInput from '../design/TextInput.svelte';
  import { openExternalUrl } from '../ipc/external';

  interface Props {
    client?: ProviderKeysClient;
    active?: boolean;
  }

  interface ProviderRow {
    provider: ProviderKeyKind;
    label: string;
    name: string;
    paid: boolean;
    hint?: string;
    registerUrl: string;
    registerLabel: string;
  }

  const ROWS: ProviderRow[] = [
    {
      provider: 'acoustid',
      label: 'AcoustID application key',
      name: 'AcoustID key',
      paid: false,
      hint: 'Use the Application API key from your AcoustID app registration, not a User API key',
      registerUrl: 'https://acoustid.org/my-applications',
      registerLabel: 'Register app on AcoustID',
    },
    {
      provider: 'discogs',
      label: 'Discogs personal access token',
      name: 'Discogs token',
      paid: false,
      registerUrl: 'https://www.discogs.com/settings/developers',
      registerLabel: 'Create token on Discogs',
    },
    {
      provider: 'audd',
      label: 'AudD API token',
      name: 'AudD token',
      paid: true,
      hint: 'Paid excerpt fallback only called when free steps are uncertain',
      registerUrl: 'https://dashboard.audd.io/',
      registerLabel: 'Open AudD dashboard',
    },
  ];

  let { client, active = true }: Props = $props();
  const paidId = $props.id();

  let configured = $state<Record<ProviderKeyKind, boolean>>({
    acoustid: false,
    discogs: false,
    audd: false,
  });
  let drafts = $state<Record<ProviderKeyKind, string>>({ acoustid: '', discogs: '', audd: '' });
  let verifyPreviews = $state(true);
  let paidOpen = $state(false);
  let busy = $state(false);
  let status = $state('');
  let error = $state('');

  const freeRows = ROWS.filter((row) => !row.paid);
  const paidRows = ROWS.filter((row) => row.paid);

  function apply(next: ProviderKeyStatus) {
    configured[next.provider] = next.configured;
  }

  $effect(() => {
    if (!client || !active) return;
    let cancelled = false;
    void client
      .status()
      .then((keys) => {
        if (cancelled) return;
        keys.forEach(apply);
        if (keys.some((key) => key.paid && key.configured)) paidOpen = true;
      })
      .catch((failure: unknown) => {
        if (!cancelled) error = failure instanceof Error ? failure.message : String(failure);
      });
    if (client.identificationSettings) {
      void client
        .identificationSettings()
        .then((settings) => {
          if (!cancelled) verifyPreviews = settings.verify_audio_previews;
        })
        .catch(() => {});
    }
    return () => {
      cancelled = true;
    };
  });

  async function toggleVerifyPreviews(checked: boolean) {
    if (!client?.saveIdentificationSettings || busy) return;
    const previous = verifyPreviews;
    verifyPreviews = checked;
    await run(async () => {
      try {
        const saved = await client.saveIdentificationSettings!({
          verify_audio_previews: checked,
        });
        verifyPreviews = saved.verify_audio_previews;
        return saved.verify_audio_previews
          ? 'Preview verification turned on.'
          : 'Preview verification turned off.';
      } catch (failure) {
        verifyPreviews = previous;
        throw failure;
      }
    });
  }

  async function openRegistration(url: string) {
    error = '';
    try {
      await openExternalUrl(url);
    } catch (failure) {
      error = failure instanceof Error ? failure.message : String(failure);
    }
  }

  async function run(action: () => Promise<string>) {
    if (!client || busy) return;
    busy = true;
    error = '';
    status = '';
    try {
      status = await action();
    } catch (failure) {
      error = failure instanceof Error ? failure.message : String(failure);
    } finally {
      busy = false;
    }
  }

  function save(event: SubmitEvent, row: ProviderRow) {
    event.preventDefault();
    const value = drafts[row.provider];
    if (!client || !value.trim()) return;
    void run(async () => {
      apply(await client.save(row.provider, value));
      drafts[row.provider] = '';
      return `${row.name} saved.`;
    });
  }

  function clear(row: ProviderRow) {
    if (!client) return;
    void run(async () => {
      apply(await client.clear(row.provider));
      drafts[row.provider] = '';
      return `${row.name} cleared.`;
    });
  }
</script>

{#snippet keyRow(row: ProviderRow)}
  <form
    class="key-row"
    role="group"
    aria-label={row.label}
    data-provider={row.provider}
    onsubmit={(event) => save(event, row)}
  >
    <div class="key-head">
      <span class="key-label">{row.label}</span>
      <DataTag label={configured[row.provider] ? 'Configured' : 'Not set'} variant="rect" />
    </div>
    <div class="key-controls">
      <div class="field-grow">
        <TextInput
          label={row.label}
          hideLabel
          size="md"
          type="password"
          autocomplete="off"
          spellcheck={false}
          placeholder={configured[row.provider] ? 'Paste a new key to replace it' : 'Paste key'}
          bind:value={drafts[row.provider]}
          disabled={busy || !client}
        />
      </div>
      <Button
        variant="primary"
        size="md"
        type="submit"
        disabled={busy || !client || !drafts[row.provider].trim()}
      >
        Save
      </Button>
      <Button
        variant="quiet"
        size="md"
        disabled={busy || !client || !configured[row.provider]}
        aria-label={`Clear ${row.name}`}
        onclick={() => clear(row)}
      >
        Clear
      </Button>
    </div>
    <div class="key-meta">
      {#if row.hint}
        <p class="key-hint">{row.hint}</p>
      {/if}
      <button
        type="button"
        class="register-link"
        onclick={() => void openRegistration(row.registerUrl)}
      >
        <Icon name="external-link" size="xs" />
        <span>{row.registerLabel}</span>
      </button>
    </div>
  </form>
{/snippet}

<div class="provider-keys">
  <p class="note">
    Keys stay in the system credential store, never in the library. Steps without a key are skipped.
  </p>

  {#each freeRows as row (row.provider)}
    {@render keyRow(row)}
  {/each}

  <button
    type="button"
    class="disclosure"
    aria-expanded={paidOpen}
    aria-controls={paidId}
    onclick={() => (paidOpen = !paidOpen)}
  >
    <Icon name={paidOpen ? 'chevron-down' : 'chevron-right'} size="xs" />
    <span>Paid recognisers</span>
  </button>
  <div id={paidId} class="paid" hidden={!paidOpen}>
    {#each paidRows as row (row.provider)}
      {@render keyRow(row)}
    {/each}
  </div>

  <ModularDivider />

  <div class="verify-block">
    <label class="verify-check">
      <input
        type="checkbox"
        checked={verifyPreviews}
        disabled={busy || !client?.saveIdentificationSettings}
        onchange={(event) => void toggleVerifyPreviews(event.currentTarget.checked)}
      />
      <span>Verify uncertain matches against 30 s audio previews</span>
    </label>
    <p class="note">
      Checks short streaming previews in memory without saving preview audio or artwork to disk.
    </p>
  </div>

  {#if status}
    <p class="status" role="status" aria-live="polite">{status}</p>
  {/if}
  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}
</div>

<style>
  .provider-keys,
  .paid {
    display: grid;
    gap: var(--space-3);
  }

  .paid[hidden] {
    display: none;
  }

  .note,
  .status,
  .error {
    margin: 0;
    font-size: var(--text-sm);
  }

  .note,
  .status {
    color: var(--color-text-2);
  }

  .error {
    color: var(--color-text);
  }

  .key-row {
    display: grid;
    gap: var(--space-1-5);
    margin: 0;
  }

  .key-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .key-label {
    min-width: 0;
    color: var(--color-text);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
  }

  .key-controls {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .field-grow {
    flex: 1 1 12rem;
    min-width: 0;
  }

  .disclosure {
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

  .disclosure:hover {
    color: var(--color-text);
  }

  .key-meta {
    display: grid;
    gap: var(--space-1);
    justify-items: start;
  }

  .key-hint {
    margin: 0;
    color: var(--color-text-2);
    font-size: var(--text-xs);
  }

  .register-link {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1-5);
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--color-text-2);
    font-family: var(--font-sans);
    font-size: var(--text-xs);
    cursor: pointer;
    transition: color var(--duration-fast) var(--ease-out);
  }

  .register-link:hover {
    color: var(--color-text);
  }

  .verify-block {
    display: grid;
    gap: var(--space-1-5);
  }

  .verify-check {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--color-text);
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .verify-check input {
    width: var(--checkbox-size);
    height: var(--checkbox-size);
    flex-shrink: 0;
    margin: 0;
    accent-color: var(--color-text);
  }

  @media (forced-colors: active) {
    .disclosure {
      color: ButtonText;
    }

    .register-link {
      color: LinkText;
    }
  }
</style>
