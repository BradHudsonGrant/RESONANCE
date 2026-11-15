// @vitest-environment happy-dom
import { readdirSync, readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import { fakeTrack } from '../library/fake-library';
import Deck from './Deck.svelte';
import Waveform from './Waveform.svelte';

HTMLElement.prototype.showPopover ??= () => {};
HTMLElement.prototype.hidePopover ??= () => {};

describe('Waveform', () => {
  it('renders timestamps from the required duration and never invents peaks when empty', () => {
    render(Waveform, {
      mode: 'curve',
      peaks: [],
      durationSec: 180,
      label: 'Empty energy curve',
    });

    const curve = screen.getByRole('img', { name: 'Empty energy curve' });
    expect(curve.textContent).toContain('0:00');
    expect(curve.textContent).toContain('1:30');
    expect(curve.textContent).toContain('3:00');
    expect(curve.textContent).not.toContain('7:02');
  });
});

describe('Deck', () => {
  it('renders the compact dock pod without live transport controls', () => {
    render(Deck, { deck: 'A', variant: 'dock' });

    expect(screen.getByText('Deck A is empty')).toBeTruthy();
    expect(screen.getByText('Decks arrive in M7')).toBeTruthy();
    expect(screen.queryByRole('combobox')).toBeNull();
  });

  it('renders the full deck card, selects a track and toggles playback by click or Space', async () => {
    const onload = vi.fn();
    const ontoggle = vi.fn();
    const tracks = [fakeTrack(0), fakeTrack(1)];

    render(Deck, {
      deck: 'A',
      variant: 'full',
      tracks,
      loadedPath: tracks[0]!.location,
      playing: false,
      shownSeconds: 42,
      durationSeconds: 301,
      canLoad: true,
      onload,
      ontoggle,
    });

    const region = screen.getByRole('region', { name: 'Deck A' });
    expect(within(region).getByText('Track 0')).toBeTruthy();
    expect(within(region).getByText('Ada Marsh')).toBeTruthy();
    expect(within(region).getByText('124.0 BPM')).toBeTruthy();
    expect(within(region).getByText('8A · Am')).toBeTruthy();
    expect(within(region).getByText('0:42 / 5:01')).toBeTruthy();

    const select = within(region).getByRole('combobox', { name: 'Track on Deck A' });
    await fireEvent.change(select, { target: { value: tracks[1]!.location } });
    expect(onload).toHaveBeenCalledWith('/music/1.flac');

    expect(region.querySelector('.brackets')?.classList.contains('active')).toBe(false);
    await fireEvent.focusIn(region);
    expect(region.querySelector('.brackets')?.classList.contains('active')).toBe(true);

    await fireEvent.keyDown(region, { key: ' ' });
    expect(ontoggle).toHaveBeenCalledOnce();

    await fireEvent.click(within(region).getByRole('button', { name: 'Play' }));
    expect(ontoggle).toHaveBeenCalledTimes(2);
  });

  it('uses tokens rather than raw hex colours in Library and Mix components', () => {
    for (const folder of ['src/lib/library', 'src/lib/mix']) {
      const dir = resolve(process.cwd(), folder);
      for (const file of readdirSync(dir).filter((name) => name.endsWith('.svelte'))) {
        const source = readFileSync(join(dir, file), 'utf8');
        expect(source, `${folder}/${file} should not contain hex colours`).not.toMatch(
          /#[0-9a-fA-F]{3,8}\b/,
        );
      }
    }
  });
});
