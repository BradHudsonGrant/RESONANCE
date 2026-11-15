<script lang="ts">
  import Button from '../design/Button.svelte';
  import Card from '../design/Card.svelte';
  import Icon from '../design/Icon.svelte';
  import Tooltip from '../design/Tooltip.svelte';

  // Loading a track onto a deck arrives with the decks in M7, so the card says the deck is empty.
  // Its actions take no focus: the dock's Mix Point already says what is still to come.
  let { deck }: { deck: 'A' | 'B' } = $props();
</script>

<div class="deck">
  <Card variant="surface" radius="lg">
    <div class="pod">
      <span class="badge" aria-hidden="true">{deck}</span>
      <span class="art" aria-hidden="true"></span>
      <div class="text">
        <span class="title">Deck {deck} is empty</span>
        <span class="note">Decks arrive in M7</span>
      </div>
      <Tooltip text="Deck {deck} actions arrive in M7" side="left">
        <Button
          variant="icon"
          size="sm"
          aria-label="Deck {deck} actions"
          aria-disabled="true"
          tabindex={-1}
        >
          <Icon name="more" size="xs" />
        </Button>
      </Tooltip>
    </div>
  </Card>
</div>

<style>
  .deck {
    min-width: 0;
  }

  .deck :global(.card) {
    height: var(--dock-card-height);
  }

  .pod {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
  }

  .badge {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    width: var(--control-height-md);
    height: var(--control-height-md);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface-2);
    font-family: var(--font-display);
    font-size: var(--text-lg);
    font-weight: var(--weight-semibold);
  }

  .art {
    flex-shrink: 0;
    width: var(--thumb-md);
    height: var(--thumb-md);
    border: var(--border-width) dashed var(--color-border);
    border-radius: var(--radius-md);
  }

  .text {
    display: grid;
    flex: 1;
    gap: var(--space-0-5);
    min-width: 0;
  }

  .title,
  .note {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .title {
    color: var(--color-text-2);
    font-family: var(--font-display);
    font-size: var(--text-base);
    font-weight: var(--weight-medium);
  }

  .note {
    color: var(--color-muted);
    font-size: var(--text-sm);
  }
</style>
