// @vitest-environment happy-dom
import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import Specimen from '../../design-page/Specimen.svelte';
import { describe as describeOrigin } from './provenance';

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
});

describe('TextInput', () => {
  it('is found by its label and holds what is typed into it', async () => {
    specimen();
    const search = screen.getByLabelText<HTMLInputElement>('Search');

    await fireEvent.input(search, { target: { value: 'ada marsh' } });

    expect(search.value).toBe('ada marsh');
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

  it('has nothing to say about a measured value', () => {
    expect(describeOrigin({ kind: 'measured' })).toBeNull();
  });
});
