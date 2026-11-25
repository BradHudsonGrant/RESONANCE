// @vitest-environment happy-dom
import { readdirSync, readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import DesignPage from '../../design-page/DesignPage.svelte';
import Specimen from '../../design-page/Specimen.svelte';
import { clearThumbCache, drawWaveThumb, hasCachedThumb, thumbCacheSize } from '../wave/renderer';
import { describe as describeOrigin, originFromSource } from './provenance';

// happy-dom has no popover API, so the top layer is left out here and checked in the browser tests.
HTMLElement.prototype.showPopover ??= () => {};
HTMLElement.prototype.hidePopover ??= () => {};

function specimen() {
  render(Specimen, { theme: 'dark' });
}

describe('Button', () => {
  it('is a button named by its label, and a disabled one cannot be pressed', () => {
    specimen();

    expect(screen.getByRole('button', { name: 'Identify' })).toHaveProperty('disabled', false);
    expect(screen.getByRole('button', { name: 'Unavailable' })).toHaveProperty('disabled', true);
  });

  it('does not submit a form it happens to sit in', () => {
    specimen();

    expect(screen.getByRole('button', { name: 'Identify' }).getAttribute('type')).toBe('button');
  });

  it('reports its pressed state when active and renders circular and select variants', () => {
    specimen();

    expect(screen.getByRole('button', { name: 'Active' }).getAttribute('aria-pressed')).toBe(
      'true',
    );
    expect(screen.getByRole('button', { name: 'Play preview' }).classList.contains('circle')).toBe(
      true,
    );
    expect(screen.getByRole('button', { name: 'EQ Flat' }).classList.contains('select')).toBe(true);
  });
});

describe('TextInput', () => {
  it('is found by its label and holds what is typed into it', async () => {
    specimen();
    const search = screen.getByLabelText<HTMLInputElement>('Search');

    await fireEvent.input(search, { target: { value: 'ada marsh' } });

    expect(search.value).toBe('ada marsh');
  });

  it('renders a command bar input with a leading icon and shortcut badge', () => {
    specimen();
    const command = screen.getByLabelText<HTMLInputElement>('Command bar');

    expect(command.placeholder).toContain('Search your library');
    expect(command.closest('.box')?.classList.contains('size-lg')).toBe(true);
    expect(command.closest('.box')?.textContent).toContain('Ctrl K');
  });
});

describe('FilterPill and SlashChip', () => {
  it('toggles active filter pills with aria-pressed and keeps disabled ones inert', async () => {
    specimen();
    const allTracks = screen.getByRole('button', { name: /All Tracks/ });
    const analysed = screen.getByRole('button', { name: 'Analysed' });
    const disabledPill = screen.getByRole('button', { name: 'Disabled' });

    expect(allTracks.getAttribute('aria-pressed')).toBe('true');
    expect(analysed.getAttribute('aria-pressed')).toBe('false');
    expect(disabledPill).toHaveProperty('disabled', true);

    await fireEvent.click(analysed);

    expect(analysed.getAttribute('aria-pressed')).toBe('true');
    expect(allTracks.getAttribute('aria-pressed')).toBe('false');
  });

  it('renders slash-command chips with their command badge and description', () => {
    specimen();
    const tempoChip = screen.getByRole('button', { name: '/ bpm 125-132 Filter by tempo' });
    const harmonicChip = screen.getByRole('button', { name: '/ key 8A Harmonic match' });

    expect(tempoChip).toHaveProperty('disabled', false);
    expect(harmonicChip.getAttribute('aria-pressed')).toBe('true');
  });
});

describe('SegmentedTabs', () => {
  it('exposes a tablist and moves selection with ArrowRight, ArrowLeft, Home and End', async () => {
    specimen();
    const tablist = screen.getByRole('tablist', { name: 'Track views' });
    const tabs = within(tablist).getAllByRole('tab');

    expect(tabs[0]?.getAttribute('aria-selected')).toBe('true');
    expect(tabs[4]).toHaveProperty('disabled', true);

    await fireEvent.keyDown(tabs[0]!, { key: 'ArrowRight' });
    expect(tabs[1]?.getAttribute('aria-selected')).toBe('true');

    await fireEvent.keyDown(tabs[1]!, { key: 'End' });
    expect(tabs[3]?.getAttribute('aria-selected')).toBe('true');

    await fireEvent.keyDown(tabs[3]!, { key: 'Home' });
    expect(tabs[0]?.getAttribute('aria-selected')).toBe('true');
  });
});

describe('Micro elements and DataTag', () => {
  it('formats zero-padded index marks, uppercase mono labels and removable tags', async () => {
    specimen();

    expect(screen.getAllByText('[01]')).toHaveLength(1);
    expect(screen.getByText('Audio traits')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Reset' })).toBeTruthy();

    expect(screen.getByText('Hypnotic')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Remove Hypnotic' }));
    expect(screen.queryByText('Hypnotic')).toBeNull();
  });

  it('renders modular dividers with separator roles', () => {
    specimen();
    const separators = screen.getAllByRole('separator');

    expect(separators).toHaveLength(2);
    expect(separators.some((sep) => sep.getAttribute('aria-orientation') === 'vertical')).toBe(
      true,
    );
    expect(document.querySelectorAll('.brackets')).toHaveLength(1);
    expect(document.querySelectorAll('.brackets .corner')).toHaveLength(0);
  });
});

describe('Audio meters, traits, scrub bar and waveforms', () => {
  it('exposes accessible meter roles for energy bars, stem dots and audio traits', () => {
    specimen();

    const energy = screen.getByRole('meter', { name: 'Energy 5 of 5' });
    expect(energy.getAttribute('aria-valuenow')).toBe('5');

    const unanalysed = screen.getByRole('meter', { name: 'Energy unanalysed' });
    expect(unanalysed.getAttribute('aria-valuenow')).toBeNull();

    const drums = screen.getByRole('meter', { name: 'DRUMS 4 of 4' });
    expect(drums.getAttribute('aria-valuenow')).toBe('4');

    const danceability = screen.getByRole('meter', { name: 'Danceability' });
    expect(danceability.getAttribute('aria-valuetext')).toBe('0.82');
  });

  it('updates ScrubBar value when its range slider moves', async () => {
    specimen();
    const volSlider = screen.getByRole<HTMLInputElement>('slider', { name: 'A Vol' });

    expect(volSlider.value).toBe('100');
    await fireEvent.input(volSlider, { target: { value: '75' } });
    expect(volSlider.value).toBe('75');
    expect(screen.getByText('75%')).toBeTruthy();
  });

  it('renders inline, curve and crossfade waveforms with accessible labels', () => {
    specimen();

    expect(screen.getByRole('img', { name: 'Inline track waveform' })).toBeTruthy();
    expect(screen.getByRole('img', { name: 'Energy curve waveform' })).toBeTruthy();
    expect(screen.getByRole('img', { name: 'Crossfade transition waveform' })).toBeTruthy();
  });
});

describe('Thumb and shared WaveThumb bitmap cache', () => {
  it('reuses cached bitmaps for the same track seed, size and theme without per-thumb WebGL contexts', () => {
    clearThumbCache();
    const first = document.createElement('canvas');
    const second = document.createElement('canvas');
    const third = document.createElement('canvas');

    const key1 = drawWaveThumb(first, {
      seed: 101,
      width: 60,
      height: 60,
      dpr: 1,
      background: [0.12, 0.12, 0.12],
      foreground: [0.95, 0.95, 0.94],
    });
    const key2 = drawWaveThumb(second, {
      seed: 101,
      width: 60,
      height: 60,
      dpr: 1,
      background: [0.12, 0.12, 0.12],
      foreground: [0.95, 0.95, 0.94],
    });
    const key3 = drawWaveThumb(third, {
      seed: 202,
      width: 60,
      height: 60,
      dpr: 1,
      background: [0.12, 0.12, 0.12],
      foreground: [0.95, 0.95, 0.94],
    });

    expect(key1).toBe(key2);
    expect(key1).not.toBe(key3);
    expect(hasCachedThumb(key1)).toBe(true);
    expect(thumbCacheSize()).toBe(2);
  });
});

describe('TableRow', () => {
  it('says which row is selected', () => {
    specimen();

    const selected = screen
      .getAllByRole('row')
      .filter((row) => row.getAttribute('aria-selected') === 'true');

    expect(selected).toHaveLength(1);
    expect(selected[0]?.textContent).toContain('Second Wind');
  });
});

describe('Sheet', () => {
  it('opens as a modal named by its title, and closes when it is answered', async () => {
    specimen();
    const sheet = screen.getByRole<HTMLDialogElement>('dialog', { hidden: true });
    expect(sheet.open).toBe(false);

    await fireEvent.click(screen.getByRole('button', { name: 'Open the confirm sheet' }));

    expect(sheet.open).toBe(true);
    const title = document.getElementById(sheet.getAttribute('aria-labelledby') ?? '');
    expect(title?.textContent).toBe('Replace this file?');

    await fireEvent.click(within(sheet).getByRole('button', { name: 'Cancel', hidden: true }));

    expect(sheet.open).toBe(false);
  });
});

describe('Tooltip', () => {
  it('shows its text and shortcut on hover and on focus, and hides on Escape', async () => {
    specimen();
    const button = screen.getByRole('button', { name: 'Search and commands' });
    const tooltip = screen
      .getAllByRole('tooltip', { hidden: true })
      .find((one) => one.textContent?.includes('Ctrl+K'));
    if (!tooltip) throw new Error('the search button has no tooltip');
    const anchor = tooltip.parentElement as HTMLElement;
    expect(tooltip.classList.contains('shown')).toBe(false);

    await fireEvent.mouseEnter(anchor);
    expect(tooltip.classList.contains('shown')).toBe(true);
    expect(anchor.getAttribute('aria-describedby')).toBe(tooltip.id);

    await fireEvent.mouseLeave(anchor);
    expect(tooltip.classList.contains('shown')).toBe(false);

    await fireEvent.focusIn(button);
    expect(tooltip.classList.contains('shown')).toBe(true);

    await fireEvent.keyDown(button, { key: 'Escape' });
    expect(tooltip.classList.contains('shown')).toBe(false);
  });
});

describe('Provenance', () => {
  it('marks an estimate with its source, version and confidence', () => {
    specimen();

    const mark = screen.getByRole('img', {
      name: 'Estimated by beat-this 1.1.0, 93% confident',
    });

    expect(mark.closest('.value')?.textContent).toContain('128.00');
    expect(mark.closest('.value')?.classList.contains('low')).toBe(false);
  });

  it('mutes a low-confidence estimate', () => {
    specimen();

    const mark = screen.getByRole('img', {
      name: 'Estimated by beat-this 1.1.0, 41% confident',
    });

    expect(mark.closest('.value')?.classList.contains('low')).toBe(true);
  });

  it('marks a value the user confirmed, and leaves a measured one plain', () => {
    specimen();

    expect(screen.getAllByRole('img', { name: 'Confirmed by you' })).toHaveLength(1);
    const measured = screen.getByText('124.00');
    expect(measured.querySelector('[role="img"]')).toBeNull();
  });

  it('has nothing to say about a measured value and maps source shorthand cleanly', () => {
    expect(describeOrigin({ kind: 'measured' })).toBeNull();
    expect(originFromSource('file')).toEqual({ kind: 'measured' });
    expect(originFromSource('user')).toEqual({ kind: 'confirmed' });
    expect(originFromSource('model', 'essentia', 0.45)).toMatchObject({
      kind: 'estimated',
      source: 'essentia',
      low: true,
    });
  });
});

describe('DesignPage and token discipline', () => {
  it('renders both dark and light theme specimens side by side', () => {
    render(DesignPage);

    expect(screen.getByRole('region', { name: 'dark theme' })).toBeTruthy();
    expect(screen.getByRole('region', { name: 'light theme' })).toBeTruthy();
  });

  it('uses tokens rather than raw hex colours or pixel lengths in design primitive styles', () => {
    const dir = resolve(process.cwd(), 'src/lib/design');
    const svelteFiles = readdirSync(dir).filter((file) => file.endsWith('.svelte'));

    for (const file of svelteFiles) {
      const source = readFileSync(join(dir, file), 'utf8');
      const styleMatch = source.match(/<style>([\s\S]*?)<\/style>/);
      const css = styleMatch?.[1] ?? '';

      expect(css, `${file} should not contain hex colours`).not.toMatch(/#[0-9a-fA-F]{3,8}\b/);
      expect(css, `${file} should not contain raw px sizes`).not.toMatch(/:\s*-?\d+(\.\d+)?px\b/);
    }
  });
});
