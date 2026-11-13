<script lang="ts">
  import Activity from '@lucide/svelte/icons/activity';
  import ChartLine from '@lucide/svelte/icons/chart-line';
  import Disc3 from '@lucide/svelte/icons/disc-3';
  import Inbox from '@lucide/svelte/icons/inbox';
  import Library from '@lucide/svelte/icons/library';
  import ListMusic from '@lucide/svelte/icons/list-music';
  import Replace from '@lucide/svelte/icons/replace';
  import Search from '@lucide/svelte/icons/search';
  import Settings from '@lucide/svelte/icons/settings';

  import Button from '../lib/design/Button.svelte';
  import Icon from '../lib/design/Icon.svelte';
  import Panel from '../lib/design/Panel.svelte';
  import Provenance from '../lib/design/Provenance.svelte';
  import Sheet from '../lib/design/Sheet.svelte';
  import TableCell from '../lib/design/TableCell.svelte';
  import TableRow from '../lib/design/TableRow.svelte';
  import TextInput from '../lib/design/TextInput.svelte';
  import Tooltip from '../lib/design/Tooltip.svelte';
  import Wordmark from '../lib/design/Wordmark.svelte';

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
  ];
  const icons = [Library, Inbox, ListMusic, Disc3, ChartLine, Activity, Settings, Search];

  let search = $state('');
  let confirming = $state(false);
</script>

<div class="specimen">
  <header class="heading">
    <Wordmark />
    <span class="theme-name">{theme}</span>
  </header>

  <section aria-label="Colours" class="swatches">
    {#each colours as colour (colour)}
      <div class="swatch">
        <span class="chip" style:background="var(--color-{colour})"></span>
        {colour}
      </div>
    {/each}
  </section>

  <section aria-label="Type" class="type">
    <h1>Headings in Space Grotesk</h1>
    <p>The interface is set in Inter, at 13 px, dense by default.</p>
    <p class="secondary">Secondary text takes the Text 2 tone.</p>
    <p class="muted">Muted text is for what matters least, and still reads.</p>
    <p class="numeric">124.00 BPM &nbsp; 8A &nbsp; 05:32.418 &nbsp; -14.0 LUFS</p>
  </section>

  <section aria-label="Buttons" class="line">
    <Button variant="primary"><Icon icon={Replace} />Replace</Button>
    <Button>Identify</Button>
    <Button variant="quiet">Cancel</Button>
    <Button disabled>Unavailable</Button>
    <Tooltip text="Search and commands" shortcut="Ctrl+K">
      <Button aria-label="Search and commands"><Icon icon={Search} /></Button>
    </Tooltip>
  </section>

  <section aria-label="Inputs" class="inputs">
    <TextInput label="Search" placeholder="Artist, title, label" bind:value={search} />
    <TextInput label="Source folder" value="D:\Music" disabled />
  </section>

  <section aria-label="Icons" class="line icons">
    {#each icons as icon (icon)}
      <Icon {icon} />
    {/each}
  </section>

  <div role="table" aria-label="Tracks" class="table">
    <TableRow>
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
    <TableRow>
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
      Panels sit on the surface colour with a border and a 6 px corner. No shadow.
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
    max-width: 640px;
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

  .swatches {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
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
    border: 1px solid var(--color-border-control);
    border-radius: var(--radius-control);
  }

  .type {
    display: grid;
    gap: var(--space-1);
    min-width: 0;
  }

  .type p {
    margin: 0;
  }

  .type h1 {
    font-size: var(--text-xl);
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

  .icons {
    gap: var(--space-4);
    color: var(--color-text-2);
  }

  .inputs {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: var(--space-3);
  }

  .table {
    border-top: 1px solid var(--color-border);
  }

  .panel-text {
    margin: 0 0 var(--space-3);
  }
</style>
