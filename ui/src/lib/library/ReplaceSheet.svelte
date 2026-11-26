<script lang="ts">
  import { untrack } from 'svelte';

  import Button from '../design/Button.svelte';
  import ModularDivider from '../design/ModularDivider.svelte';
  import MonoLabel from '../design/MonoLabel.svelte';
  import SegmentedTabs from '../design/SegmentedTabs.svelte';
  import Sheet from '../design/Sheet.svelte';
  import type { ConvertedFile } from '../ipc/bindings/ConvertedFile';
  import type { ConvertPresetName } from '../ipc/bindings/ConvertPresetName';
  import type { FfmpegStatus } from '../ipc/bindings/FfmpegStatus';
  import type { LibraryTrack } from '../ipc/bindings/LibraryTrack';
  import type { ReplaceChoiceName } from '../ipc/bindings/ReplaceChoiceName';
  import type { ReplaceComparison } from '../ipc/bindings/ReplaceComparison';
  import type { ReplaceFileFacts } from '../ipc/bindings/ReplaceFileFacts';
  import type { ReplaceResult } from '../ipc/bindings/ReplaceResult';
  import { duration, level, sampleRate } from './format';
  import { filenameTemplate } from './rename-template.svelte';

  export interface ReplaceClient {
    compare: (trackId: number, path: string, template: string | null) => Promise<ReplaceComparison>;
    apply: (
      trackId: number,
      path: string,
      template: string | null,
      choice: ReplaceChoiceName,
    ) => Promise<ReplaceResult>;
    ffmpegStatus: () => Promise<FfmpegStatus>;
    convert: (trackId: number, preset: ConvertPresetName) => Promise<ConvertedFile>;
    repair: (trackId: number) => Promise<ConvertedFile>;
    pickAudioFile?: () => Promise<string | null>;
  }

  interface Props {
    open: boolean;
    track: LibraryTrack | null;
    /** A file dropped on the track, compared as soon as the sheet opens. */
    path?: string | null;
    client: ReplaceClient;
    onreplaced?: (result: ReplaceResult) => void;
  }

  let { open = $bindable(false), track, path = null, client, onreplaced }: Props = $props();

  let newPath = $state('');
  let comparing = $state(false);
  let comparison = $state<ComparisonFor | null>(null);
  let choice = $state<ReplaceChoiceName>('replace');
  let applying = $state(false);
  let making = $state(false);
  let ffmpeg = $state<FfmpegStatus | null>(null);
  let preset = $state<ConvertPresetName>('mp3_cbr_320');
  let failure = $state<string | null>(null);

  interface ComparisonFor {
    path: string;
    result: ReplaceComparison;
  }

  let session = 0;
  const busy = $derived(comparing || applying || making);
  const shown = $derived(comparison?.result ?? null);

  const choices = [
    { id: 'replace', label: 'Replace' },
    { id: 'add_separate_track', label: 'Add as a separate track' },
  ] as const;

  $effect(() => {
    if (!open) return;
    const dropped = path;
    untrack(() => {
      session += 1;
      comparison = null;
      failure = null;
      newPath = dropped ?? '';
      if (ffmpeg === null) void loadFfmpeg();
      if (dropped) void compare(dropped);
    });
  });

  function message(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }

  async function loadFfmpeg() {
    try {
      ffmpeg = await client.ffmpegStatus();
    } catch {
      ffmpeg = { program: null, version: null, presets: [] };
    }
  }

  async function compare(candidate: string) {
    if (!track || !candidate.trim()) return;
    const ours = session;
    comparing = true;
    failure = null;
    comparison = null;
    try {
      const result = await client.compare(track.id, candidate.trim(), filenameTemplate.current);
      if (ours !== session) return;
      comparison = { path: candidate.trim(), result };
      choice = result.default_choice;
    } catch (error) {
      if (ours === session) failure = message(error);
    } finally {
      if (ours === session) comparing = false;
    }
  }

  async function browse() {
    if (!client.pickAudioFile || busy) return;
    const picked = await client.pickAudioFile();
    if (picked) {
      newPath = picked;
      await compare(picked);
    }
  }

  async function make(kind: 'convert' | 'repair') {
    if (!track || busy) return;
    const ours = session;
    making = true;
    failure = null;
    comparison = null;
    try {
      const made =
        kind === 'convert' ? await client.convert(track.id, preset) : await client.repair(track.id);
      if (ours !== session) return;
      newPath = made.path;
      making = false;
      await compare(made.path);
    } catch (error) {
      if (ours === session) failure = message(error);
    } finally {
      if (ours === session) making = false;
    }
  }

  async function apply() {
    if (!track || !comparison || busy) return;
    applying = true;
    failure = null;
    try {
      const result = await client.apply(
        track.id,
        comparison.path,
        filenameTemplate.current,
        choice,
      );
      open = false;
      onreplaced?.(result);
    } catch (error) {
      failure = message(error);
    } finally {
      applying = false;
    }
  }

  const integer = new Intl.NumberFormat('en-GB');
  const twoPlaces = new Intl.NumberFormat('en-GB', {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });

  interface Row {
    label: string;
    value: (facts: ReplaceFileFacts) => string;
    numeric: boolean;
  }

  const rows: Row[] = [
    { label: 'Format', value: (f) => f.format.toUpperCase(), numeric: false },
    {
      label: 'Bitrate',
      value: (f) => (f.bitrate_kbps ? `${integer.format(f.bitrate_kbps)} kbps` : ''),
      numeric: true,
    },
    {
      label: 'Sample rate',
      value: (f) => (f.sample_rate ? sampleRate(f.sample_rate) : ''),
      numeric: true,
    },
    {
      label: 'Bit depth',
      value: (f) => (f.bit_depth ? `${f.bit_depth}-bit` : ''),
      numeric: true,
    },
    { label: 'Length', value: (f) => duration(f.duration_ms), numeric: true },
    { label: 'Loudness', value: (f) => level(f.integrated_lufs, 'LUFS'), numeric: true },
    { label: 'True peak', value: (f) => level(f.true_peak_dbtp, 'dBTP', true), numeric: true },
    {
      label: 'Spectral cutoff',
      value: (f) =>
        f.spectral_cutoff_hz ? `${twoPlaces.format(f.spectral_cutoff_hz / 1000)} kHz` : 'None',
      numeric: true,
    },
    {
      label: 'Quality',
      value: (f) => f.flags[0] ?? 'No problems found',
      numeric: false,
    },
  ];

  function fileName(full: string): string {
    return full.split(/[\\/]/).pop() ?? full;
  }

  const offset = $derived(
    shown?.offset_ms == null ? 'Not lined up' : `${twoPlaces.format(shown.offset_ms)} ms`,
  );
  const speed = $derived(
    shown?.tempo_ratio == null
      ? '—'
      : shown.tempo_ratio === 1
        ? '1'
        : shown.tempo_ratio.toLocaleString('en-GB', { maximumFractionDigits: 5 }),
  );
  const similarity = $derived(
    shown?.fingerprint_similarity == null
      ? '—'
      : `${Math.round(shown.fingerprint_similarity * 100)}%`,
  );
  const placedName = $derived(
    shown ? (choice === 'replace' ? shown.replace_name : shown.separate_name) : '',
  );
  const cues = $derived(
    shown && shown.cues_to_carry > 0
      ? `${integer.format(shown.cues_to_carry)} ${shown.cues_to_carry === 1 ? 'cue moves' : 'cues move'} onto the new file, marked carried over, check`
      : '',
  );
</script>

<Sheet bind:open title="Replace file">
  {#if track}
    <div class="replace-body">
      <div class="target">
        <span class="target-title">{track.title}</span>
        {#if track.artist}<span class="target-artist">{track.artist}</span>{/if}
      </div>

      <section class="section" aria-label="New file">
        <MonoLabel text="New file" tone="secondary" />
        <form
          class="path-row"
          onsubmit={(event) => {
            event.preventDefault();
            void compare(newPath);
          }}
        >
          <input
            type="text"
            class="path-input"
            aria-label="New file path"
            placeholder="Path to the better file"
            bind:value={newPath}
            disabled={busy}
          />
          {#if client.pickAudioFile}
            <Button size="sm" disabled={busy} onclick={() => void browse()}>Browse file…</Button>
          {/if}
          <Button type="submit" size="sm" disabled={busy || !newPath.trim()}>
            {comparing ? 'Comparing…' : 'Compare'}
          </Button>
        </form>

        <div class="make-row" aria-label="Make one from this file">
          {#if ffmpeg?.version}
            <select class="preset" aria-label="Convert preset" bind:value={preset} disabled={busy}>
              {#each ffmpeg.presets as item (item.preset)}
                <option value={item.preset}>{item.label}</option>
              {/each}
            </select>
            <Button size="sm" disabled={busy} onclick={() => void make('convert')}>
              {making ? 'Working…' : 'Convert'}
            </Button>
            <Button size="sm" disabled={busy} onclick={() => void make('repair')}>Repair</Button>
          {:else if ffmpeg}
            <p class="note">
              Convert and Repair need FFmpeg, which was not found. Settings shows how to add it.
            </p>
          {/if}
        </div>
      </section>

      {#if shown}
        <ModularDivider />

        <section class="section" aria-label="Comparison">
          <p class="verdict" role="status">{shown.verdict_label}</p>
          <table class="compare">
            <thead>
              <tr>
                <th scope="col" aria-label="Field"></th>
                <th scope="col">Current</th>
                <th scope="col">New</th>
              </tr>
            </thead>
            <tbody>
              {#each rows as row (row.label)}
                <tr>
                  <th scope="row">{row.label}</th>
                  <td class={{ numeric: row.numeric }}>{row.value(shown.old) || '—'}</td>
                  <td class={{ numeric: row.numeric }}>{row.value(shown.new) || '—'}</td>
                </tr>
              {/each}
            </tbody>
          </table>

          <dl class="match">
            <div>
              <dt>Relation</dt>
              <dd>{shown.relation}</dd>
            </div>
            <div>
              <dt>Fingerprint match</dt>
              <dd class="numeric">{similarity}</dd>
            </div>
            <div>
              <dt>Offset</dt>
              <dd class="numeric">{offset}</dd>
            </div>
            <div>
              <dt>Speed ratio</dt>
              <dd class="numeric">{speed}</dd>
            </div>
          </dl>
        </section>

        <ModularDivider />

        <section class="section" aria-label="What Apply does">
          <SegmentedTabs
            items={choices}
            bind:value={choice}
            ariaLabel="What to do with the new file"
          />
          <p class="note">
            {#if choice === 'replace'}
              The new file goes beside the old one as <span class="name">{placedName}</span>. Tags
              merge, collections stay, and the old file goes to the bin.
            {:else}
              The new file goes beside the old one as <span class="name">{placedName}</span>, as a
              track of its own. The current track is left as it is.
            {/if}
          </p>
          {#if choice === 'replace' && cues}
            <p class="note">{cues}</p>
          {/if}
          {#if choice === 'replace' && shown.verdict === 'different_version'}
            <p class="note" role="alert">
              These do not line up as one recording, so cues may land in the wrong places.
            </p>
          {/if}
          <p class="note muted">From {fileName(comparison?.path ?? '')}</p>
        </section>
      {/if}

      {#if failure}
        <p class="error" role="alert">{failure}</p>
      {/if}
    </div>
  {/if}

  {#snippet actions()}
    <Button variant="quiet" disabled={applying} onclick={() => (open = false)}>Close</Button>
    {#if shown}
      <Button variant="primary" disabled={busy} onclick={() => void apply()}>
        {applying ? 'Applying…' : choice === 'replace' ? 'Replace' : 'Add track'}
      </Button>
    {/if}
  {/snippet}
</Sheet>

<style>
  .replace-body {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: var(--space-3-5);
    min-width: 0;
  }

  .target {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--space-2);
    min-width: 0;
  }

  .target-title {
    color: var(--color-text);
    font-size: var(--text-base);
    font-weight: var(--weight-medium);
  }

  .target-artist {
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }

  .section {
    display: grid;
    gap: var(--space-2-5);
    min-width: 0;
  }

  .path-row,
  .make-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2-5);
    min-width: 0;
  }

  .path-input,
  .preset {
    flex: 1;
    min-width: 0;
    height: var(--control-height-sm);
    padding: 0 var(--space-2-5);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    font-size: var(--text-sm);
  }

  .path-input:focus-visible,
  .preset:focus-visible {
    border-color: var(--color-text);
  }

  .verdict {
    margin: 0;
    color: var(--color-text);
    font-size: var(--text-base);
    font-weight: var(--weight-medium);
  }

  .compare {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--text-sm);
    table-layout: fixed;
  }

  .compare th,
  .compare td {
    padding: var(--space-1-5) var(--space-2);
    border-bottom: var(--border-width) solid var(--color-border);
    overflow-wrap: anywhere;
    text-align: left;
  }

  .compare thead th {
    color: var(--color-muted);
    font-weight: var(--weight-regular);
  }

  .compare tbody th {
    color: var(--color-text-2);
    font-weight: var(--weight-regular);
  }

  .compare td {
    color: var(--color-text);
  }

  .compare .numeric,
  .match .numeric {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
  }

  .match {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(7rem, 1fr));
    gap: var(--space-2) var(--space-4);
    margin: 0;
  }

  .match div {
    display: grid;
    gap: var(--space-1);
    min-width: 0;
  }

  .match dt {
    color: var(--color-muted);
    font-size: var(--text-xs);
  }

  .match dd {
    margin: 0;
    color: var(--color-text);
    font-size: var(--text-sm);
  }

  .note {
    margin: 0;
    color: var(--color-text-2);
    font-size: var(--text-sm);
    line-height: var(--leading-normal);
  }

  .muted {
    color: var(--color-muted);
  }

  .name {
    color: var(--color-text);
  }

  .error {
    margin: 0;
    padding-left: var(--space-2-5);
    border-left: var(--selection-bar-width) solid var(--color-accent);
    color: var(--color-text);
    font-size: var(--text-sm);
  }

  @media (forced-colors: active) {
    .path-input,
    .preset {
      border-color: CanvasText;
    }

    .compare th,
    .compare td {
      border-bottom-color: CanvasText;
    }
  }
</style>
