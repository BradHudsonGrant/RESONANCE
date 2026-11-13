<script lang="ts">
  import { tick } from 'svelte';

  import Button from '../src/lib/design/Button.svelte';
  import { measureTableScroll, type ScrollMeasurement } from '../src/lib/library/measure-scroll';

  let running = $state(false);
  let result = $state('');
  let output: HTMLTextAreaElement | undefined = $state();

  function settleTable(): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, 1_000));
  }

  async function measure() {
    if (running) return;
    running = true;
    result = '';
    const runs: ScrollMeasurement[] = [];
    const report = {
      startedAt: new Date().toISOString(),
      viewport: {
        width: window.innerWidth,
        height: window.innerHeight,
        devicePixelRatio: window.devicePixelRatio,
      },
      userAgent: navigator.userAgent,
      framesPerPass: 600,
      controlsAbsent: false,
      runs,
      error: '',
    };
    try {
      await tick();
      report.controlsAbsent = !document.querySelector('[aria-label="Library scroll benchmark"]');
      if (!document.querySelector('[role="grid"][aria-label="Tracks"]')) {
        throw new Error('Wait for the track table to open, then run the measurements again.');
      }
      for (let run = 0; run < 5; run += 1) {
        await settleTable();
        runs.push(await measureTableScroll());
      }
    } catch (error) {
      report.error = error instanceof Error ? error.message : String(error);
    } finally {
      result = JSON.stringify(report, null, 2);
      running = false;
      await tick();
      output?.focus();
    }
  }
</script>

{#if !running}
  <section class="benchmark" aria-label="Library scroll benchmark">
    <Button variant="primary" onclick={measure}>Run five scroll measurements</Button>
    <p>Keep this window visible and unchanged. Controls hide while the table scrolls.</p>
    {#if result}
      <textarea
        bind:this={output}
        aria-label="Scroll benchmark results"
        readonly
        rows="16"
        value={result}></textarea>
    {/if}
  </section>
{/if}

<style>
  .benchmark {
    position: fixed;
    z-index: 10;
    top: var(--space-3);
    right: var(--space-3);
    display: grid;
    gap: var(--space-3);
    width: min(36rem, calc(100vw - var(--space-6)));
    padding: var(--space-4);
    border: 1px solid var(--color-border-control);
    border-radius: var(--radius-panel);
    background: var(--color-raised);
    color: var(--color-text);
  }

  p {
    margin: 0;
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }

  textarea {
    width: 100%;
    padding: var(--space-2);
    border: 1px solid var(--color-border-control);
    border-radius: var(--radius-control);
    background: var(--color-surface);
    color: var(--color-text);
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    resize: vertical;
  }
</style>
