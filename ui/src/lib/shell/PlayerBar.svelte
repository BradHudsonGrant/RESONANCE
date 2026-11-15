<script lang="ts">
  import Button from '../design/Button.svelte';
  import Card from '../design/Card.svelte';
  import Icon from '../design/Icon.svelte';
  import ModularDivider from '../design/ModularDivider.svelte';
  import ScrubBar from '../design/ScrubBar.svelte';
  import Tooltip from '../design/Tooltip.svelte';

  // Preview playback arrives in M2 and the decks' mixer in M7. Until then every control is
  // unavailable and says which milestone brings it, and no level or position is shown. Only Play
  // takes focus, so Tab does not stop on every placeholder.
</script>

<footer class="player" aria-label="Player">
  <Card variant="surface" radius="xl">
    <div class="row">
      <div class="transport">
        <Tooltip text="Preview playback arrives in M2" shortcut="Space">
          <Button variant="circle" size="lg" aria-label="Play" aria-disabled="true">
            <Icon name="play" size="sm" filled />
          </Button>
        </Tooltip>
        <Tooltip text="Preview playback arrives in M2">
          <Button variant="icon" aria-label="Pause" aria-disabled="true" tabindex={-1}>
            <Icon name="pause" size="sm" />
          </Button>
        </Tooltip>
        <span class="state">Nothing playing</span>
      </div>

      <div class="scrub">
        <Tooltip text="Preview playback arrives in M2">
          <ScrubBar size="md" stretch disabled />
        </Tooltip>
      </div>

      <div class="mixer">
        <Tooltip text="Deck volume arrives in M7">
          <ScrubBar size="sm" label="A Vol" disabled />
        </Tooltip>
        <Tooltip text="Deck volume arrives in M7">
          <ScrubBar size="sm" label="B Vol" disabled />
        </Tooltip>
        <span class="label">Curve</span>
        <Tooltip text="Crossfader curves arrive in M7">
          <Button variant="select" size="sm" aria-label="Curve" aria-disabled="true" tabindex={-1}>
            —
          </Button>
        </Tooltip>
        <span class="label">EQ</span>
        <Tooltip text="EQ arrives in M7">
          <Button variant="select" size="sm" aria-label="EQ" aria-disabled="true" tabindex={-1}>
            —
          </Button>
        </Tooltip>
        <ModularDivider orientation="vertical" />
        <Tooltip text="Headphone cue arrives in M7" side="left">
          <Button variant="icon" aria-label="Headphone cue" aria-disabled="true" tabindex={-1}>
            <Icon name="headphones" size="md" />
          </Button>
        </Tooltip>
      </div>
    </div>
  </Card>
</footer>

<style>
  .player {
    container: player / inline-size;
    padding: 0 var(--player-inset-x) var(--player-inset-bottom);
  }

  .player :global(.card) {
    height: var(--player-height);
    padding: 0 var(--space-4);
  }

  .player :global(.content) {
    height: 100%;
  }

  .row {
    display: grid;
    grid-template-columns: auto minmax(var(--player-scrub-min-width), 1fr) auto;
    align-items: center;
    gap: var(--space-8);
    height: 100%;
  }

  .transport,
  .mixer {
    display: flex;
    align-items: center;
    gap: var(--space-4);
  }

  .transport {
    gap: var(--space-3);
  }

  .state {
    margin-left: var(--space-2);
    color: var(--color-muted);
    font-size: var(--text-sm);
    white-space: nowrap;
  }

  .scrub {
    display: flex;
    min-width: 0;
  }

  .scrub > :global(.anchor) {
    flex: 1;
    min-width: 0;
  }

  .label {
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }

  /* Below a 1360 px window the volume sliders shorten. Container queries cannot read tokens. */
  @container player (width < 1320px) {
    .mixer {
      --scrub-width-sm: var(--scrub-width-compact);
      gap: var(--space-3);
    }

    .row {
      gap: var(--space-5);
    }
  }
</style>
