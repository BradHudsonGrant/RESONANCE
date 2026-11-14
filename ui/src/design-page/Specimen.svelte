<script lang="ts">
  import Button from '../lib/design/Button.svelte';
  import Card from '../lib/design/Card.svelte';
  import DataTag from '../lib/design/DataTag.svelte';
  import FilterPill from '../lib/design/FilterPill.svelte';
  import FocusBrackets from '../lib/design/FocusBrackets.svelte';
  import GridDots from '../lib/design/GridDots.svelte';
  import Icon, { type IconName } from '../lib/design/Icon.svelte';
  import IndexMarks from '../lib/design/IndexMarks.svelte';
  import LevelDots from '../lib/design/LevelDots.svelte';
  import Mark from '../lib/design/Mark.svelte';
  import MeterBar from '../lib/design/MeterBar.svelte';
  import ModularDivider from '../lib/design/ModularDivider.svelte';
  import MonoLabel from '../lib/design/MonoLabel.svelte';
  import Panel from '../lib/design/Panel.svelte';
  import Provenance from '../lib/design/Provenance.svelte';
  import ScrubBar from '../lib/design/ScrubBar.svelte';
  import SegmentedTabs from '../lib/design/SegmentedTabs.svelte';
  import Sheet from '../lib/design/Sheet.svelte';
  import SlashChip from '../lib/design/SlashChip.svelte';
  import TableCell from '../lib/design/TableCell.svelte';
  import TableRow from '../lib/design/TableRow.svelte';
  import TextInput from '../lib/design/TextInput.svelte';
  import Thumb from '../lib/design/Thumb.svelte';
  import Tooltip from '../lib/design/Tooltip.svelte';
  import TraitBar from '../lib/design/TraitBar.svelte';
  import Wordmark from '../lib/design/Wordmark.svelte';
  import Waveform from '../lib/mix/Waveform.svelte';
  import Wave from '../lib/wave/Wave.svelte';

  let { theme }: { theme: string } = $props();

  const colours = [
    'background',
    'surface',
    'surface-2',
    'raised',
    'border',
    'border-control',
    'text',
    'text-2',
    'muted',
    'accent',
    'on-accent',
  ] as const;

  const iconNames: IconName[] = [
    'search',
    'library',
    'prep',
    'sets',
    'mix',
    'replay',
    'collections',
    'activity',
    'settings',
    'audio',
    'play',
    'pause',
    'star',
    'check',
    'unanalysed',
    'recent',
    'sliders',
    'plus',
    'close',
    'more',
    'info',
    'chevron-down',
    'chevron-right',
    'headphones',
    'sidebar-collapse',
    'arrow-right',
  ];

  const segmentedItems = [
    { id: 'all', label: 'All', count: 248 },
    { id: 'stems', label: 'Stems', count: 64 },
    { id: 'cues', label: 'Cues', count: 18 },
    { id: 'grid', label: 'Grid' },
    { id: 'locked', label: 'Locked', disabled: true },
  ] as const;

  let search = $state('');
  let commandQuery = $state('');
  let confirming = $state(false);
  let activeFilter = $state('all');
  let activeTab = $state('all');
  let scrubPosition = $state(60);
  let deckAVol = $state(100);
  let removableTags = $state(['Hypnotic', 'Peak Time', 'Industrial', 'Rolling Bass']);

  function removeTag(tag: string) {
    removableTags = removableTags.filter((item) => item !== tag);
  }
</script>

<div class="specimen">
  <header class="heading">
    <Wordmark />
    <span class="theme-name">{theme}</span>
  </header>

  <section aria-label="Brand mark" class="block">
    <div class="line">
      <Mark size="sm" />
      <Mark size="md" />
      <Mark size="lg" />
      <Mark size="lg" accentPeak />
      <Wordmark size="lg" />
    </div>
    <MonoLabel tone="secondary">MUSIC INTELLIGENCE<br />FOR DJS</MonoLabel>
  </section>

  <section aria-label="Colours" class="swatches">
    {#each colours as colour (colour)}
      <div class="swatch">
        <span class="chip" style:background="var(--color-{colour})"></span>
        <span class="numeric">{colour}</span>
      </div>
    {/each}
  </section>

  <section aria-label="Type" class="type">
    <h1 class="display-2xl">Library</h1>
    <h2 class="display-xl">Echo Chamber</h2>
    <h3 class="display-lg">Headings in Space Grotesk</h3>
    <p class="sans-base">Search your library, or type a command.</p>
    <p>The interface is set in Inter, at 13 px, dense by default.</p>
    <p class="secondary">Secondary text takes the Text 2 tone.</p>
    <p class="muted">Muted text is for what matters least, and still reads.</p>
    <p class="numeric">124.00 BPM &nbsp; 8A &nbsp; 05:32.418 &nbsp; -14.0 LUFS</p>
  </section>

  <section aria-label="Micro elements" class="block">
    <MonoLabel
      text="Audio traits"
      tone="secondary"
      infoTooltip="Measured from the audio signal"
      actionLabel="Reset"
    />

    <div class="meta-grid">
      <div class="meta-cell">
        <MonoLabel text="BPM" size="xs" tone="muted" />
        <span class="numeric">128.0</span>
      </div>
      <div class="meta-cell">
        <MonoLabel text="KEY" size="xs" tone="muted" />
        <span class="numeric">8A</span>
      </div>
      <div class="meta-cell">
        <MonoLabel text="TIME" size="xs" tone="muted" />
        <span class="numeric">07:02</span>
      </div>
      <div class="meta-cell">
        <IndexMarks index={1} />
        <span class="numeric">2026</span>
      </div>
    </div>

    <div class="line">
      <DataTag label="Analysed" variant="pill" leadingIcon="check" />
      <DataTag label="128.0 BPM · 8A" variant="pill" />
      <DataTag label="WAV 24-bit" variant="rect" />
      <DataTag label="-7.4 LUFS" variant="rect" />
      <DataTag label="95%" variant="score" />
      <DataTag label="Ctrl K" variant="keybadge" />
    </div>

    <div class="line">
      {#each removableTags as tag (tag)}
        <DataTag label={tag} variant="removable" onRemove={() => removeTag(tag)} />
      {/each}
    </div>

    <ModularDivider />

    <GridDots spacing={16}>
      <FocusBrackets active>
        <span class="secondary">Drop folder to scan</span>
      </FocusBrackets>
    </GridDots>
  </section>

  <section aria-label="Buttons" class="block">
    <div class="line">
      <Button variant="primary"><Icon name="prep" />Replace</Button>
      <Button>Identify</Button>
      <Button active>Active</Button>
      <Button variant="quiet">Cancel</Button>
      <Button disabled>Unavailable</Button>
      <Button variant="select">EQ Flat</Button>
      <Button variant="circle" size="lg" ariaLabel="Play preview">
        <Icon name="play" filled />
      </Button>
      <Button variant="circle" size="sm" ariaLabel="Add tag">
        <Icon name="plus" size="xs" />
      </Button>
      <Button variant="icon" size="sm" ariaLabel="Play suggestion">
        <Icon name="play" size="xs" filled />
      </Button>
      <Tooltip text="Search and commands" shortcut="Ctrl+K">
        <Button variant="icon" aria-label="Search and commands"><Icon name="search" /></Button>
      </Tooltip>
    </div>

    <div class="line">
      <FilterPill
        label="All Tracks"
        active={activeFilter === 'all'}
        count={12846}
        onclick={() => (activeFilter = 'all')}
      />
      <FilterPill
        label="Analysed"
        icon="check"
        active={activeFilter === 'analysed'}
        onclick={() => (activeFilter = 'analysed')}
      />
      <FilterPill
        label="Unanalysed"
        icon="unanalysed"
        active={activeFilter === 'unanalysed'}
        onclick={() => (activeFilter = 'unanalysed')}
      />
      <FilterPill
        label="Favourites"
        icon="star"
        active={activeFilter === 'favourites'}
        onclick={() => (activeFilter = 'favourites')}
      />
      <FilterPill label="Disabled" icon="recent" disabled />
    </div>

    <div class="line">
      <SlashChip command="/ bpm 125-132" description="Filter by tempo" />
      <SlashChip command="/ key 8A" description="Harmonic match" active />
      <SlashChip command="/ build set" description="Unavailable" disabled />
    </div>

    <div>
      <SegmentedTabs items={segmentedItems} bind:value={activeTab} ariaLabel="Track views" />
    </div>
  </section>

  <section aria-label="Inputs" class="block">
    <div class="inputs">
      <TextInput
        label="Command bar"
        hideLabel
        size="lg"
        leadingIcon="search"
        shortcutBadge="Ctrl K"
        placeholder="Search your library, or type a command..."
        bind:value={commandQuery}
      />
      <div class="input-pair">
        <TextInput label="Search" placeholder="Artist, title, label" bind:value={search} />
        <TextInput label="Source folder" value="D:\Music" disabled />
      </div>
    </div>

    <div class="line">
      <ScrubBar size="md" label="Timeline" bind:value={scrubPosition} valueText="04:15 / 07:02" />
      <ScrubBar size="sm" label="A Vol" bind:value={deckAVol} valueText="{deckAVol}%" />
      <ScrubBar size="sm" label="B Vol" value={50} valueText="50%" disabled />
    </div>
  </section>

  <section aria-label="Icons" class="line icons">
    {#each iconNames as name (name)}
      <Icon {name} />
    {/each}
  </section>

  <section aria-label="Audio meters and waveforms" class="block">
    <div class="line">
      <MeterBar value={null} />
      <MeterBar value={1} />
      <MeterBar value={3} />
      <MeterBar value={5} />
      <ModularDivider orientation="vertical" />
      <LevelDots label="DRUMS" level={4} />
      <LevelDots label="BASS" level={3} />
      <LevelDots label="VOCALS" level={1} />
      <LevelDots label="OTHER" level={2} />
    </div>

    <div class="traits">
      <TraitBar label="Danceability" value={0.82} />
      <TraitBar label="Energy" value={0.91} />
      <TraitBar label="Warmth" value={null} />
    </div>

    <div class="line">
      <Thumb size="sm" seed={101} alt="Track 101 thumbnail" />
      <Thumb size="md" seed={101} alt="Track 101 deck artwork" />
      <Thumb
        size="lg"
        seed={202}
        bands={{ low: 0.85, mid: 0.55, high: 0.35, tone: 0.42, spread: 0.28 }}
        alt="Track 202 inspector artwork"
      />
      <div class="sidebar-wave-preview">
        <Wave variant="sidebar" seed={303} paused />
      </div>
    </div>

    <div class="cards-grid">
      <Card variant="surface" radius="md">
        <div class="compact-track">
          <Thumb size="sm" seed={11} />
          <div class="compact-meta">
            <span class="compact-title">Echo Chamber</span>
            <span class="secondary">Kaden</span>
          </div>
          <DataTag label="128.0 · 8A" variant="pill" />
        </div>
      </Card>
      <Card variant="interactive" radius="lg" waveArt waveSeed={42} selected>
        <div class="collection-card">
          <span class="collection-title">Peak Time</span>
          <span class="secondary numeric">248 tracks</span>
        </div>
      </Card>
      <Card variant="dashed" radius="lg" ariaLabel="New collection">
        <span class="secondary">+ New Collection</span>
      </Card>
    </div>

    <div class="waveforms">
      <Waveform mode="inline" seed={1} progress={0.45} label="Inline track waveform" />
      <Waveform mode="curve" seed={2} durationSec={422} label="Energy curve waveform" />
      <Waveform
        mode="crossfade"
        seed={3}
        progress={0.55}
        mixPoint={0.6}
        playing
        label="Crossfade transition waveform"
      />
    </div>
  </section>

  <div role="table" aria-label="Tracks" class="table">
    <TableRow header>
      <TableCell header>Title</TableCell>
      <TableCell header>Artist</TableCell>
      <TableCell header numeric>BPM</TableCell>
      <TableCell header numeric>Key</TableCell>
    </TableRow>
    <TableRow>
      <TableCell>First Light</TableCell>
      <TableCell>Ada Marsh</TableCell>
      <TableCell numeric>
        <Provenance origin={{ kind: 'measured' }}>124.00</Provenance>
      </TableCell>
      <TableCell numeric>
        <Provenance origin={{ kind: 'confirmed' }}>8A</Provenance>
      </TableCell>
    </TableRow>
    <TableRow selected>
      <TableCell>Second Wind</TableCell>
      <TableCell>Ada Marsh</TableCell>
      <TableCell numeric>
        <Provenance
          origin={{
            kind: 'estimated',
            source: 'beat-this',
            version: '1.1.0',
            confidence: 0.93,
            low: false,
          }}
        >
          128.00
        </Provenance>
      </TableCell>
      <TableCell numeric>
        <Provenance origin={{ kind: 'measured' }}>11B</Provenance>
      </TableCell>
    </TableRow>
    <TableRow disabled>
      <TableCell>Third Rail</TableCell>
      <TableCell>Kofi Bell</TableCell>
      <TableCell numeric>
        <Provenance
          origin={{
            kind: 'estimated',
            source: 'beat-this',
            version: '1.1.0',
            confidence: 0.41,
            low: true,
          }}
        >
          87.50
        </Provenance>
      </TableCell>
      <TableCell numeric>
        <Provenance origin={{ kind: 'measured' }}>4A</Provenance>
      </TableCell>
    </TableRow>
  </div>

  <Panel title="Panel">
    <p class="secondary panel-text">
      Panels sit on the surface colour with a border and a 10 px corner. No shadow.
    </p>
    <Button onclick={() => (confirming = true)}>Open the confirm sheet</Button>
  </Panel>

  <Sheet bind:open={confirming} title="Replace this file?">
    <p>first-light.mp3 becomes first-light.flac. Two collections and one set point to it.</p>
    {#snippet actions()}
      <Button variant="quiet" onclick={() => (confirming = false)}>Cancel</Button>
      <Button variant="primary" onclick={() => (confirming = false)}>Replace</Button>
    {/snippet}
  </Sheet>
</div>

<style>
  .specimen {
    display: grid;
    gap: var(--space-6);
    max-width: var(--inspector-max-width);
    min-width: 0;
  }

  @media (min-width: 720px) {
    .specimen {
      max-width: none;
    }
  }

  .heading {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .theme-name {
    color: var(--color-text-2);
    font-size: var(--text-sm);
    text-transform: capitalize;
  }

  .block {
    display: grid;
    gap: var(--space-3);
    min-width: 0;
  }

  .swatches {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(var(--trait-label-width), 1fr));
    gap: var(--space-2);
    font-size: var(--text-sm);
  }

  .swatch {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .chip {
    width: var(--space-5);
    height: var(--space-5);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-sm);
  }

  .type {
    display: grid;
    gap: var(--space-1);
    min-width: 0;
  }

  .type p {
    margin: 0;
  }

  .display-2xl {
    font-size: var(--text-2xl);
    line-height: var(--leading-tight);
    letter-spacing: var(--tracking-tight);
  }

  .display-xl {
    font-size: var(--text-xl);
    font-weight: var(--weight-semibold);
    line-height: var(--leading-snug);
    letter-spacing: var(--tracking-tight);
  }

  .display-lg {
    font-size: var(--text-lg);
    line-height: var(--leading-heading);
    letter-spacing: var(--tracking-heading);
  }

  .sans-base {
    font-size: var(--text-base);
  }

  .secondary {
    color: var(--color-text-2);
  }

  .muted {
    color: var(--color-muted);
  }

  .line {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .meta-grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: var(--space-3);
  }

  .meta-cell {
    display: grid;
    gap: var(--space-0-5);
  }

  .cards-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(var(--card-wave-width), 1fr));
    gap: var(--space-3);
  }

  .icons {
    gap: var(--space-3);
    color: var(--color-text-2);
  }

  .inputs,
  .traits,
  .waveforms {
    display: grid;
    gap: var(--space-3);
  }

  .input-pair {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(var(--card-wave-width), 1fr));
    gap: var(--space-3);
  }

  .sidebar-wave-preview {
    width: var(--card-wave-width);
    height: var(--sidebar-wave-height);
    overflow: hidden;
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
  }

  .compact-track {
    display: flex;
    align-items: center;
    gap: var(--space-2-5);
  }

  .compact-meta {
    display: grid;
    flex: 1;
    min-width: 0;
  }

  .compact-title,
  .collection-title {
    font-family: var(--font-display);
    font-size: var(--text-base);
    font-weight: var(--weight-medium);
  }

  .collection-card {
    display: grid;
    gap: var(--space-1);
    min-height: var(--thumb-md);
    align-content: end;
  }

  .table {
    border-top: var(--border-width) solid var(--color-border);
  }

  .panel-text {
    margin: 0 0 var(--space-3);
  }
</style>
