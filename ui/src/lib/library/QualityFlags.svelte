<script lang="ts">
  import type { TrackQualityFlag } from '../ipc/bindings/TrackQualityFlag';

  interface Props {
    flags: TrackQualityFlag[];
  }

  let { flags }: Props = $props();
</script>

<ul class="flags">
  {#each flags as flag (flag.kind + flag.reason)}
    <li class="flag">
      <span class="head">
        <span class="label">{flag.label}</span>
        <span class="certainty">{flag.certainty === 'confirmed' ? 'Confirmed' : 'Likely'}</span>
      </span>
      <span class="reason">{flag.reason}</span>
    </li>
  {/each}
</ul>

<style>
  .flags {
    display: grid;
    gap: var(--space-2-5);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .flag {
    display: grid;
    gap: var(--space-1);
    min-width: 0;
  }

  .head {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
  }

  .label {
    color: var(--color-text);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
  }

  .certainty,
  .reason {
    color: var(--color-text-2);
    font-size: var(--text-xs);
  }

  .reason {
    line-height: var(--leading-normal);
  }
</style>
