// @vitest-environment happy-dom
import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import App from '../../App.svelte';
import { getTheme, onThemeChange, setTheme } from '../design/theme';
import CommandBar, { type Command } from './CommandBar.svelte';
import PlayerBar from './PlayerBar.svelte';
import SettingsSheet from './SettingsSheet.svelte';
import Sidebar, { spaces } from './Sidebar.svelte';
import TopBar from './TopBar.svelte';
import TransitionDock from './TransitionDock.svelte';

function tooltipTexts(): (string | null)[] {
  return screen.getAllByRole('tooltip', { hidden: true }).map((tooltip) => tooltip.textContent);
}

describe('theme', () => {
  beforeEach(() => {
    localStorage.clear();
    delete document.documentElement.dataset.theme;
  });

  it('follows the system until a theme is chosen', () => {
    expect(getTheme()).toBe('system');
    expect(document.documentElement.dataset.theme).toBeUndefined();
  });

  it('keeps a chosen theme and sets it on the page', () => {
    setTheme('light');
    expect(getTheme()).toBe('light');
    expect(document.documentElement.dataset.theme).toBe('light');
    expect(localStorage.getItem('resonance-theme')).toBe('light');

    setTheme('system');
    expect(getTheme()).toBe('system');
    expect(document.documentElement.dataset.theme).toBeUndefined();
  });

  it('tells canvases to repaint when a theme is chosen, until they stop listening', async () => {
    const repaint = vi.fn();
    const stop = onThemeChange(repaint);

    setTheme('dark');
    await vi.waitFor(() => expect(repaint).toHaveBeenCalledOnce());

    stop();
    setTheme('light');
    await new Promise((resolve) => setTimeout(resolve));
    expect(repaint).toHaveBeenCalledOnce();
  });
});

describe('TopBar', () => {
  it('has Settings, and names the milestone of each tool still to come', () => {
    render(TopBar, { onsettings: vi.fn() });

    expect(screen.getByLabelText('Resonance')).toBeTruthy();
    const audio = screen.getByRole('button', { name: 'Audio engine status' });
    const activity = screen.getByRole('button', { name: 'Activity' });
    expect(audio.getAttribute('aria-disabled')).toBe('true');
    expect(activity.getAttribute('aria-disabled')).toBe('true');
    expect(activity.tabIndex).toBe(-1);

    const tooltips = tooltipTexts();
    expect(tooltips.some((text) => text?.includes('Audio engine status arrives in M7'))).toBe(true);
    expect(tooltips.some((text) => text?.includes('Activity arrives in M2'))).toBe(true);
  });

  it('opens Settings', async () => {
    const onsettings = vi.fn();
    render(TopBar, { onsettings });

    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }));
    expect(onsettings).toHaveBeenCalledOnce();
  });
});

describe('Sidebar', () => {
  const props = {
    activeSpace: 'Library' as const,
    trackCount: 12846,
    onSelectSpace: vi.fn(),
    onCollapse: vi.fn(),
  };

  it('lists the five spaces in order, with the active one current', () => {
    render(Sidebar, props);

    expect(spaces.map((space) => space.name)).toEqual(['Library', 'Prep', 'Sets', 'Mix', 'Replay']);
    expect(screen.getByRole('button', { name: 'Library' }).getAttribute('aria-current')).toBe(
      'page',
    );
    expect(screen.getByText('12,846')).toBeTruthy();
  });

  it('names the milestone of each space still to come, and does not open it', async () => {
    const onSelectSpace = vi.fn();
    render(Sidebar, { ...props, onSelectSpace });

    for (const name of ['Prep', 'Sets', 'Replay']) {
      expect(screen.getByRole('button', { name }).getAttribute('aria-disabled')).toBe('true');
    }
    await fireEvent.click(screen.getByRole('button', { name: 'Prep' }));
    expect(onSelectSpace).not.toHaveBeenCalled();

    const tooltips = tooltipTexts();
    expect(tooltips.some((text) => text?.includes('Prep arrives in M3'))).toBe(true);
    expect(tooltips.some((text) => text?.includes('Sets arrives in M10'))).toBe(true);
    expect(tooltips.some((text) => text?.includes('Replay arrives in M9'))).toBe(true);
  });

  it('keeps only the first space still to come in the Tab order', () => {
    render(Sidebar, props);

    expect(screen.getByRole('button', { name: 'Prep' }).tabIndex).toBe(0);
    expect(screen.getByRole('button', { name: 'Sets' }).tabIndex).toBe(-1);
    expect(screen.getByRole('button', { name: 'Replay' }).tabIndex).toBe(-1);
    expect(screen.getByRole('button', { name: 'New collection' }).tabIndex).toBe(0);
  });

  it('opens a space that exists', async () => {
    const onSelectSpace = vi.fn();
    render(Sidebar, { ...props, onSelectSpace });

    await fireEvent.click(screen.getByRole('button', { name: 'Mix' }));
    expect(onSelectSpace).toHaveBeenCalledWith('Mix');
  });

  it('shows empty collections and active set placeholder before collections exist', () => {
    render(Sidebar, props);

    expect(screen.getByText('No collections yet.')).toBeTruthy();
    expect(screen.getByText('No set yet. Sets arrive in M10.')).toBeTruthy();
  });

  it('creates, selects, renames and confirms deleting collections', async () => {
    const onSelectSpace = vi.fn();
    const onSelectCollection = vi.fn();
    const onCreateCollection = vi.fn(async () => {});
    const onRenameCollection = vi.fn(async () => {});
    const onDeleteCollection = vi.fn(async () => {});
    const sample = [{ id: 1, name: 'Warm Up', position: 0, track_count: 12 }];

    const { rerender } = render(Sidebar, {
      ...props,
      collections: sample,
      activeCollectionId: null,
      onSelectSpace,
      onSelectCollection,
      onCreateCollection,
      onRenameCollection,
      onDeleteCollection,
    });

    await fireEvent.click(screen.getByRole('button', { name: 'New collection' }));
    const createField = screen.getByRole('textbox', { name: 'New collection name' });
    await fireEvent.input(createField, { target: { value: 'Peak Hour' } });
    await fireEvent.submit(createField.closest('form')!);
    expect(onCreateCollection).toHaveBeenCalledWith('Peak Hour');

    await fireEvent.click(screen.getByRole('button', { name: /^Warm Up/ }));
    expect(onSelectSpace).toHaveBeenCalledWith('Library');
    expect(onSelectCollection).toHaveBeenCalledWith(1);

    await fireEvent.click(screen.getByRole('button', { name: 'Library' }));
    expect(onSelectCollection).toHaveBeenLastCalledWith(null);

    await rerender({
      ...props,
      collections: sample,
      activeCollectionId: 1,
      onSelectSpace,
      onSelectCollection,
      onCreateCollection,
      onRenameCollection,
      onDeleteCollection,
    });
    expect(tooltipTexts().some((text) => text?.includes('Library · Warm Up'))).toBe(true);

    await fireEvent.click(screen.getByRole('button', { name: 'Actions for Warm Up' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Rename' }));
    const renameField = screen.getByRole('textbox', { name: 'Rename collection' });
    await fireEvent.input(renameField, { target: { value: 'Opener' } });
    await fireEvent.submit(renameField.closest('form')!);
    expect(onRenameCollection).toHaveBeenCalledWith(1, 'Opener');

    await fireEvent.click(screen.getByRole('button', { name: 'Actions for Warm Up' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Delete' }));
    expect(screen.getByText(/Tracks and files on disk will stay/)).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Delete collection' }));
    expect(onDeleteCollection).toHaveBeenCalledWith(1);
  });

  it('asks the shell to collapse or expand it', async () => {
    const onCollapse = vi.fn();
    render(Sidebar, { ...props, onCollapse });

    await fireEvent.click(screen.getByRole('button', { name: 'Collapse the sidebar' }));
    expect(onCollapse).toHaveBeenLastCalledWith(true);
    await fireEvent.click(screen.getByRole('button', { name: 'Expand the sidebar' }));
    expect(onCollapse).toHaveBeenLastCalledWith(false);
  });
});

describe('CommandBar', () => {
  it('invites searching the library or typing a command', () => {
    render(CommandBar, { commands: [], inputId: 'commands' });

    const input = screen.getByRole('combobox', { name: 'Commands' });
    expect(input.getAttribute('placeholder')).toBe('Search your library, or type a command…');
  });

  it('activates the BPM slash command chip and keeps future milestones marked', () => {
    render(CommandBar, { commands: [], inputId: 'commands' });

    const bpmChip = screen.getByRole('button', { name: '/ bpm 125-132 Filter by tempo' });
    expect(bpmChip.getAttribute('aria-disabled')).toBeNull();
    expect(bpmChip.tabIndex).toBe(0);

    const chips = [
      '/ key 8A Compatible with A minor',
      '/ similar Find matching tracks',
      '/ build set Generate 90m progression',
    ].map((name) => screen.getByRole('button', { name }));
    for (const chip of chips) {
      expect(chip.getAttribute('aria-disabled')).toBe('true');
      expect(chip.tabIndex).toBe(-1);
    }
    expect(screen.getByRole('button', { name: 'More slash commands' }).tabIndex).toBe(-1);

    const tooltips = tooltipTexts();
    for (const milestone of ['M5', 'M6', 'M10']) {
      expect(tooltips.some((text) => text?.includes(`arrives in ${milestone}`))).toBe(true);
    }
  });

  it('searches as you type without showing an empty command list, and runs / bpm commands', async () => {
    const onsearch = vi.fn();
    const onbpm = vi.fn();
    render(CommandBar, { commands: [], inputId: 'commands', onsearch, onbpm });

    const input = screen.getByRole('combobox', { name: 'Commands' }) as HTMLInputElement;
    await fireEvent.focus(input);
    await fireEvent.input(input, { target: { value: 'daft punk' } });
    expect(onsearch).toHaveBeenLastCalledWith('daft punk');
    expect(screen.queryByRole('listbox', { name: 'Commands' })).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: '/ bpm 125-132 Filter by tempo' }));
    expect(input.value).toBe('/ bpm 125-132');
    await fireEvent.keyDown(input, { key: 'Enter' });
    expect(onbpm).toHaveBeenLastCalledWith(125, 132);
  });

  it('runs the command chosen from the list', async () => {
    const scan = vi.fn();
    const commands: Command[] = [
      { name: 'Scan a music folder', run: scan },
      { name: 'Find similar tracks', arrives: 'M6' },
    ];
    render(CommandBar, { commands, inputId: 'commands' });

    await fireEvent.focus(screen.getByRole('combobox', { name: 'Commands' }));
    await fireEvent.mouseDown(screen.getByRole('option', { name: 'Scan a music folder' }));
    expect(scan).toHaveBeenCalledOnce();
  });
});

describe('TransitionDock', () => {
  it('names the milestone of each control still to come', () => {
    render(TransitionDock, { onCollapse: vi.fn() });

    expect(screen.getByText('A → B')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Mix Point' }).getAttribute('aria-disabled')).toBe(
      'true',
    );
    expect(screen.getByRole('button', { name: 'Blend' }).tabIndex).toBe(-1);

    const tooltips = tooltipTexts();
    expect(tooltips.some((text) => text?.includes('Mix points arrive in M10'))).toBe(true);
    expect(tooltips.some((text) => text?.includes('Blends arrive in M10'))).toBe(true);
  });

  it('shows two empty decks and where their crossfade goes', () => {
    render(TransitionDock, { onCollapse: vi.fn() });

    expect(screen.getByText('Deck A is empty')).toBeTruthy();
    expect(screen.getByText('Deck B is empty')).toBeTruthy();
    expect(
      screen.getByText('Two loaded decks show their crossfade here. Decks arrive in M7.'),
    ).toBeTruthy();
  });

  it('asks the shell to collapse or expand it', async () => {
    const onCollapse = vi.fn();
    render(TransitionDock, { onCollapse });

    await fireEvent.click(screen.getByRole('button', { name: 'Collapse the dock' }));
    expect(onCollapse).toHaveBeenLastCalledWith(true);
    await fireEvent.click(screen.getByRole('button', { name: 'Expand the dock' }));
    expect(onCollapse).toHaveBeenLastCalledWith(false);
  });
});

describe('PlayerBar', () => {
  it('shows nothing playing, and names the milestone of every control', () => {
    render(PlayerBar);

    expect(screen.getByText('Nothing playing')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Play' }).getAttribute('aria-disabled')).toBe('true');
    for (const name of ['Pause', 'Curve', 'EQ', 'Headphone cue']) {
      const button = screen.getByRole('button', { name });
      expect(button.getAttribute('aria-disabled')).toBe('true');
      expect(button.tabIndex).toBe(-1);
    }
    for (const name of ['Scrub', 'A Vol', 'B Vol']) {
      expect(screen.getByRole('slider', { name }).hasAttribute('disabled')).toBe(true);
    }

    const tooltips = tooltipTexts();
    for (const text of [
      'Preview playback arrives in M2',
      'Deck volume arrives in M7',
      'Crossfader curves arrive in M7',
      'EQ arrives in M7',
      'Headphone cue arrives in M7',
    ]) {
      expect(tooltips.some((tooltip) => tooltip?.includes(text))).toBe(true);
    }
  });
});

describe('SettingsSheet', () => {
  it('offers the three themes, library sources and the sections still to come', () => {
    render(SettingsSheet, { open: true });
    const dialog = screen.getByRole('dialog');

    for (const name of ['Dark', 'Light', 'System']) {
      expect(within(dialog).getByRole('tab', { name })).toBeTruthy();
    }
    expect(within(dialog).getByText('Library sources')).toBeTruthy();
    expect(
      within(dialog).getByText(
        'No music folders added yet. Files on disk are never changed by a scan.',
      ),
    ).toBeTruthy();
    expect(within(dialog).getByText('Ignored folder names')).toBeTruthy();
    for (const ignored of ['_replaced', '_wrong_match', '_trash', '_converted_originals']) {
      expect(within(dialog).getByText(ignored)).toBeTruthy();
    }
    expect(within(dialog).getByText('Provider keys')).toBeTruthy();
    expect(
      within(dialog).getByText('Keys for AcoustID, Discogs and paid recognisers arrive in M3.'),
    ).toBeTruthy();
    expect(
      within(dialog).getByText('Audio and MIDI device configuration arrives in M7.'),
    ).toBeTruthy();
  });

  it('switches the theme', async () => {
    render(SettingsSheet, { open: true });
    const dialog = screen.getByRole('dialog');

    await fireEvent.click(within(dialog).getByRole('tab', { name: 'Light' }));
    expect(getTheme()).toBe('light');
    expect(document.documentElement.dataset.theme).toBe('light');

    await fireEvent.click(within(dialog).getByRole('tab', { name: 'Dark' }));
    expect(getTheme()).toBe('dark');
    expect(document.documentElement.dataset.theme).toBe('dark');
  });

  it('closes on Done', async () => {
    render(SettingsSheet, { open: true });
    const dialog = screen.getByRole('dialog') as HTMLDialogElement;

    await fireEvent.click(within(dialog).getByRole('button', { name: 'Done' }));
    expect(dialog.open).toBe(false);
  });
});

describe('App shell', () => {
  function shell(): HTMLElement {
    const element = document.querySelector<HTMLElement>('.app-shell');
    if (!element) throw new Error('the shell did not render');
    return element;
  }

  it('renders the sidebar, command bar, dock and player', () => {
    render(App);

    expect(screen.getByRole('navigation', { name: 'Spaces' })).toBeTruthy();
    expect(screen.getByRole('combobox', { name: 'Commands' })).toBeTruthy();
    expect(screen.getByRole('region', { name: 'Transition' })).toBeTruthy();
    expect(screen.getByRole('contentinfo', { name: 'Player' })).toBeTruthy();
  });

  it('flips the sidebar with [ and the dock with D', async () => {
    render(App);

    await fireEvent.keyDown(window, { key: '[' });
    expect(shell().dataset.sidebar).toBe('collapsed');
    await fireEvent.keyDown(window, { key: '[' });
    expect(shell().dataset.sidebar).toBe('expanded');

    await fireEvent.keyDown(window, { key: 'd' });
    expect(shell().dataset.dock).toBe('collapsed');
    await fireEvent.keyDown(window, { key: 'd' });
    expect(shell().dataset.dock).toBe('expanded');
  });

  it('hides and shows the inspector with I', async () => {
    render(App);

    expect(screen.getByRole('complementary', { name: 'Inspector' })).toBeTruthy();
    await fireEvent.keyDown(window, { key: 'i' });
    expect(screen.queryByRole('complementary', { name: 'Inspector' })).toBeNull();
    await fireEvent.keyDown(window, { key: 'i' });
    expect(screen.getByRole('complementary', { name: 'Inspector' })).toBeTruthy();
  });

  it('switches between Library and Mix with 1 and 4, and leaves the dock out of Mix', async () => {
    render(App);

    await fireEvent.keyDown(window, { key: '4' });
    expect(screen.getByRole('button', { name: 'Mix' }).getAttribute('aria-current')).toBe('page');
    expect(screen.queryByRole('region', { name: 'Transition' })).toBeNull();

    await fireEvent.keyDown(window, { key: '1' });
    expect(screen.getByRole('button', { name: 'Library' }).getAttribute('aria-current')).toBe(
      'page',
    );
    expect(screen.getByRole('region', { name: 'Transition' })).toBeTruthy();
  });

  it('ignores shortcuts while a sheet is open', async () => {
    render(App);

    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }));
    for (const key of ['i', 'd', '[', '4']) await fireEvent.keyDown(window, { key });

    expect(shell().dataset.sidebar).toBeUndefined();
    expect(shell().dataset.dock).toBeUndefined();
    expect(shell().dataset.inspector).toBeUndefined();
    expect(screen.getByRole('button', { name: 'Library' }).getAttribute('aria-current')).toBe(
      'page',
    );
  });
});
