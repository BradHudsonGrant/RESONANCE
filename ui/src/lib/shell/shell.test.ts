// @vitest-environment happy-dom
import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import App from '../../App.svelte';
import { getTheme, onThemeChange, setTheme } from '../design/theme';
import { core } from '../ipc/core';
import { fakeTrack } from '../library/fake-library';

HTMLElement.prototype.showPopover ??= () => {};
HTMLElement.prototype.hidePopover ??= () => {};
import ActivitySheet, { type ActivityClient } from './ActivitySheet.svelte';
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
  it('has live Activity and Settings buttons, and names M7 for audio engine status', async () => {
    const onactivity = vi.fn();
    render(TopBar, { onsettings: vi.fn(), onactivity });

    expect(screen.getByLabelText('Resonance')).toBeTruthy();
    const audio = screen.getByRole('button', { name: 'Audio engine status' });
    const activity = screen.getByRole('button', { name: 'Activity' });
    expect(audio.getAttribute('aria-disabled')).toBe('true');
    expect(activity.getAttribute('aria-disabled')).toBeNull();
    expect(activity.tabIndex).toBe(0);
    expect(activity.querySelector('.activity-dot')).toBeNull();

    const tooltips = tooltipTexts();
    expect(tooltips.some((text) => text?.includes('Audio engine status arrives in M7'))).toBe(true);
    expect(tooltips.some((text) => text?.trim() === 'Activity')).toBe(true);

    await fireEvent.click(activity);
    expect(onactivity).toHaveBeenCalledOnce();
  });

  it('shows the active-job indicator dot and count tooltip while jobs are active', () => {
    render(TopBar, { onsettings: vi.fn(), activeJobCount: 2, activityOpen: true });

    const activity = screen.getByRole('button', { name: 'Activity' });
    expect(activity.getAttribute('aria-pressed')).toBe('true');
    expect(activity.querySelector('.activity-dot')).toBeTruthy();
    expect(tooltipTexts().some((text) => text?.includes('Activity · 2 active'))).toBe(true);
  });

  it('opens Settings', async () => {
    const onsettings = vi.fn();
    render(TopBar, { onsettings });

    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }));
    expect(onsettings).toHaveBeenCalledOnce();
  });
});

describe('ActivitySheet', () => {
  it('renders empty active and recent states when idle', () => {
    render(ActivitySheet, { open: true });
    const dialog = screen.getByRole('dialog', { name: 'Activity' });

    expect(within(dialog).getByText('No active jobs.')).toBeTruthy();
    expect(within(dialog).getByText('No recent jobs.')).toBeTruthy();
  });

  it('displays active progress, Mix backoff state, errors, and wires Cancel, Retry and Clear finished', async () => {
    let snapshot = {
      mix_backoff_active: true,
      active: [
        {
          id: 11,
          kind: 'scan_source',
          tier: 'interactive' as const,
          status: 'running' as const,
          label: 'Scan Warehouse',
          done_items: 400,
          total_items: 1_000,
          error_message: null,
          paused_for_mix: false,
          paused: false,
          created_ms: 1_000,
          started_ms: 1_010,
          finished_ms: null,
        },
        {
          id: 12,
          kind: 'identify_library',
          tier: 'metadata' as const,
          status: 'queued' as const,
          label: 'Identify audio files',
          done_items: 0,
          total_items: 1_000,
          error_message: null,
          paused_for_mix: true,
          paused: false,
          created_ms: 1_020,
          started_ms: null,
          finished_ms: null,
        },
        {
          id: 13,
          kind: 'rename_files',
          tier: 'metadata' as const,
          status: 'running' as const,
          label: 'Rename 1,240 files',
          done_items: 300,
          total_items: 1_240,
          error_message: null,
          paused_for_mix: false,
          paused: true,
          created_ms: 1_030,
          started_ms: 1_040,
          finished_ms: null,
        },
      ],
      recent: [
        {
          id: 9,
          kind: 'scan_source',
          tier: 'interactive' as const,
          status: 'failed' as const,
          label: 'Scan UnpluggedDrive',
          done_items: 0,
          total_items: null,
          error_message: 'The folder could not be scanned.',
          paused_for_mix: false,
          paused: false,
          created_ms: 500,
          started_ms: 510,
          finished_ms: 520,
        },
        {
          id: 8,
          kind: 'identify_library',
          tier: 'metadata' as const,
          status: 'completed' as const,
          label: 'Identify audio files',
          done_items: 48,
          total_items: 48,
          error_message: null,
          paused_for_mix: false,
          paused: false,
          created_ms: 100,
          started_ms: 110,
          finished_ms: 300,
        },
      ],
    };

    const client: ActivityClient = {
      snapshot: vi.fn(async () => snapshot),
      cancel: vi.fn(async () => snapshot),
      pause: vi.fn(async () => snapshot),
      resume: vi.fn(async () => snapshot),
      retry: vi.fn(async () => snapshot),
      clearFinished: vi.fn(async () => {
        snapshot = { ...snapshot, recent: [] };
        return snapshot;
      }),
    };
    const onsnapshot = vi.fn();

    render(ActivitySheet, { open: true, snapshot, client, onsnapshot });
    const dialog = screen.getByRole('dialog', { name: 'Activity' });

    expect(
      within(dialog).getByText(
        'Background audio identification is paused while a Mix deck is playing.',
      ),
    ).toBeTruthy();

    const activeList = within(dialog).getByRole('list', { name: 'Active jobs' });
    const recentList = within(dialog).getByRole('list', { name: 'Recent jobs' });

    const scanCard = within(activeList).getByRole('listitem', { name: 'Scan Warehouse' });
    expect(within(scanCard).getByText('Running')).toBeTruthy();
    expect(within(scanCard).getByText('400 / 1,000')).toBeTruthy();
    const bar = within(scanCard).getByRole('progressbar', { name: 'Scan Warehouse progress' });
    expect(bar.getAttribute('aria-valuenow')).toBe('400');
    expect(bar.getAttribute('aria-valuemax')).toBe('1000');

    const idCard = within(activeList).getByRole('listitem', { name: 'Identify audio files' });
    expect(within(idCard).getByText('Paused · Mix live')).toBeTruthy();

    await fireEvent.click(within(scanCard).getByRole('button', { name: 'Cancel' }));
    expect(client.cancel).toHaveBeenCalledWith(11);

    await fireEvent.click(within(scanCard).getByRole('button', { name: 'Pause Scan Warehouse' }));
    expect(client.pause).toHaveBeenCalledWith(11);

    const renameCard = within(activeList).getByRole('listitem', { name: 'Rename 1,240 files' });
    expect(within(renameCard).getByText('Paused')).toBeTruthy();
    await fireEvent.click(
      within(renameCard).getByRole('button', { name: 'Resume Rename 1,240 files' }),
    );
    expect(client.resume).toHaveBeenCalledWith(13);

    const failedCard = within(recentList).getByRole('listitem', { name: 'Scan UnpluggedDrive' });
    expect(within(failedCard).getByText('Failed')).toBeTruthy();
    expect(within(failedCard).getByText('The folder could not be scanned.')).toBeTruthy();
    await fireEvent.click(within(failedCard).getByRole('button', { name: 'Retry' }));
    expect(client.retry).toHaveBeenCalledWith(9);

    await fireEvent.click(within(dialog).getByRole('button', { name: 'Clear finished' }));
    expect(client.clearFinished).toHaveBeenCalledOnce();
    expect(onsnapshot).toHaveBeenLastCalledWith(expect.objectContaining({ recent: [] }));
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

    const actionsBtn = screen.getByRole('button', { name: 'Actions for Warm Up' });
    await fireEvent.click(actionsBtn);
    const renameItem = screen.getByRole('menuitem', { name: 'Rename' });
    const deleteItem = screen.getByRole('menuitem', { name: 'Delete' });
    expect(document.activeElement).toBe(renameItem);
    await fireEvent.keyDown(window, { key: 'ArrowDown' });
    expect(document.activeElement).toBe(deleteItem);
    await fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByRole('menu')).toBeNull();
    expect(document.activeElement).toBe(actionsBtn);

    await fireEvent.click(actionsBtn);
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
  it('shows nothing playing when unloaded and keeps M7 mixer controls unavailable', () => {
    render(PlayerBar);

    expect(screen.getByText('Nothing playing')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Play' }).getAttribute('aria-disabled')).toBe('true');
    expect(screen.getByRole('slider', { name: 'Scrub' }).hasAttribute('disabled')).toBe(true);
    expect(screen.getByRole('button', { name: 'Mute' }).getAttribute('aria-disabled')).toBeNull();
    expect(screen.getByRole('slider', { name: 'Volume' }).hasAttribute('disabled')).toBe(false);

    for (const name of ['Curve', 'EQ', 'Headphone cue']) {
      const button = screen.getByRole('button', { name });
      expect(button.getAttribute('aria-disabled')).toBe('true');
      expect(button.tabIndex).toBe(-1);
    }
    for (const name of ['A Vol', 'B Vol']) {
      expect(screen.getByRole('slider', { name }).hasAttribute('disabled')).toBe(true);
    }

    const tooltips = tooltipTexts();
    for (const text of [
      'Select a track to preview',
      'Deck volume arrives in M7',
      'Crossfader curves arrive in M7',
      'EQ arrives in M7',
      'Headphone cue arrives in M7',
    ]) {
      expect(tooltips.some((tooltip) => tooltip?.includes(text))).toBe(true);
    }
  });

  it('shows loaded track metadata, time readouts and fires seek, volume and mute callbacks', async () => {
    const ontoggleplay = vi.fn();
    const onseek = vi.fn();
    const onvolume = vi.fn();
    const ontogglemute = vi.fn();

    render(PlayerBar, {
      trackId: 12,
      title: 'Midnight Loop',
      artist: 'Kofi Bell',
      playing: true,
      positionMs: 65_000,
      durationMs: 240_000,
      volume: 0.8,
      muted: false,
      canPlay: true,
      ontoggleplay,
      onseek,
      onvolume,
      ontogglemute,
    });

    expect(screen.getByText('Midnight Loop')).toBeTruthy();
    expect(screen.getByText('Kofi Bell')).toBeTruthy();
    expect(screen.getByText('1:05')).toBeTruthy();
    expect(screen.getByText('4:00')).toBeTruthy();

    const pauseBtn = screen.getByRole('button', { name: 'Pause' });
    expect(pauseBtn.getAttribute('aria-pressed')).toBe('true');
    await fireEvent.click(pauseBtn);
    expect(ontoggleplay).toHaveBeenCalledOnce();

    const scrub = screen.getByRole('slider', { name: 'Scrub' });
    expect(scrub.hasAttribute('disabled')).toBe(false);
    await fireEvent.input(scrub, { target: { value: '120000' } });
    expect(onseek).toHaveBeenCalledWith(120_000);

    const volumeSlider = screen.getByRole('slider', { name: 'Volume' });
    await fireEvent.input(volumeSlider, { target: { value: '55' } });
    expect(onvolume).toHaveBeenCalledWith(0.55);

    await fireEvent.click(screen.getByRole('button', { name: 'Mute' }));
    expect(ontogglemute).toHaveBeenCalledOnce();
  });

  it('surfaces preview errors as an alert in the transport area', () => {
    render(PlayerBar, {
      error: 'The file could not be read as audio.',
    });

    expect(screen.getByRole('alert').textContent).toBe('The file could not be read as audio.');
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
    expect(within(dialog).getByText('Providers')).toBeTruthy();
    expect(within(dialog).getByRole('group', { name: 'AcoustID application key' })).toBeTruthy();
    expect(
      within(dialog).getByRole('group', { name: 'Discogs personal access token' }),
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
  afterEach(() => {
    vi.restoreAllMocks();
  });

  /** The sidebar, docks and player mount a task after the first frame, as they do at startup. */
  async function renderApp() {
    render(App);
    await screen.findByRole('navigation', { name: 'Spaces' });
  }

  function shell(): HTMLElement {
    const element = document.querySelector<HTMLElement>('.app-shell');
    if (!element) throw new Error('the shell did not render');
    return element;
  }

  it('renders the sidebar, command bar, dock and player', async () => {
    await renderApp();

    expect(screen.getByRole('navigation', { name: 'Spaces' })).toBeTruthy();
    expect(screen.getByRole('combobox', { name: 'Commands' })).toBeTruthy();
    expect(screen.getByRole('region', { name: 'Transition' })).toBeTruthy();
    expect(screen.getByRole('contentinfo', { name: 'Player' })).toBeTruthy();
  });

  it('flips the sidebar with [ and the dock with D', async () => {
    await renderApp();

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
    await renderApp();

    expect(screen.getByRole('complementary', { name: 'Inspector' })).toBeTruthy();
    await fireEvent.keyDown(window, { key: 'i' });
    expect(screen.queryByRole('complementary', { name: 'Inspector' })).toBeNull();
    await fireEvent.keyDown(window, { key: 'i' });
    expect(screen.getByRole('complementary', { name: 'Inspector' })).toBeTruthy();
  });

  it('switches between Library and Mix with 1 and 4, and leaves the dock out of Mix', async () => {
    await renderApp();

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
    await renderApp();

    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }));
    for (const key of ['i', 'd', '[', '4']) await fireEvent.keyDown(window, { key });

    expect(shell().dataset.sidebar).toBeUndefined();
    expect(shell().dataset.dock).toBeUndefined();
    expect(shell().dataset.inspector).toBeUndefined();
    expect(screen.getByRole('button', { name: 'Library' }).getAttribute('aria-current')).toBe(
      'page',
    );
  });

  it('ignores single-key shell shortcuts while a menu is open', async () => {
    await renderApp();
    const menu = document.createElement('div');
    menu.setAttribute('role', 'menu');
    document.body.appendChild(menu);
    try {
      for (const key of ['i', 'd', '[', '4']) await fireEvent.keyDown(window, { key });
      expect(shell().dataset.sidebar).toBeUndefined();
      expect(shell().dataset.dock).toBeUndefined();
      expect(shell().dataset.inspector).toBeUndefined();
      expect(screen.getByRole('button', { name: 'Library' }).getAttribute('aria-current')).toBe(
        'page',
      );
    } finally {
      menu.remove();
    }
  });

  it('toggles preview playback with Space only outside interactive controls and pauses when entering Mix', async () => {
    const track = fakeTrack(0);
    let playing = false;
    let loaded = false;
    const callSpy = vi
      .spyOn(core, 'call')
      .mockImplementation(async (command: string): Promise<never> => {
        if (command === 'tracks') {
          return { total: 1, library_total: 1, offset: 0, tracks: [track] } as never;
        }
        if (command === 'collections') return [] as never;
        if (command === 'library_facets') {
          return {
            labels: [],
            bpm_min: null,
            bpm_max: null,
            year_min: null,
            year_max: null,
          } as never;
        }
        if (command === 'preview_status') {
          return {
            track_id: loaded ? track.id : null,
            title: loaded ? track.title : null,
            artist: loaded ? track.artist : null,
            playing,
            position_ms: 0,
            duration_ms: loaded ? 301_000 : 0,
            volume: 1,
            generation: loaded ? 1 : 0,
          } as never;
        }
        if (command === 'preview_load_and_play') {
          loaded = true;
          playing = true;
          return {
            track_id: track.id,
            title: track.title,
            artist: track.artist,
            playing: true,
            position_ms: 0,
            duration_ms: 301_000,
            volume: 1,
            generation: 1,
          } as never;
        }
        if (command === 'preview_toggle_play') {
          playing = !playing;
          return {
            track_id: track.id,
            title: track.title,
            artist: track.artist,
            playing,
            position_ms: 0,
            duration_ms: 301_000,
            volume: 1,
            generation: 1,
          } as never;
        }
        if (command === 'preview_pause') {
          playing = false;
          return {
            track_id: track.id,
            title: track.title,
            artist: track.artist,
            playing: false,
            position_ms: 0,
            duration_ms: 301_000,
            volume: 1,
            generation: 1,
          } as never;
        }
        if (command === 'mix_status') throw new Error('No Mix core in this test.');
        return undefined as never;
      });

    await renderApp();
    const grid = await screen.findByRole('grid', { name: 'Tracks' });
    grid.focus();
    await fireEvent.keyDown(grid, { key: 'ArrowDown' });

    // Space inside search box or button does not trigger preview commands.
    const searchInput = screen.getByRole('combobox', { name: 'Commands' });
    searchInput.focus();
    await fireEvent.keyDown(searchInput, { key: ' ' });
    const settingsBtn = screen.getByRole('button', { name: 'Settings' });
    settingsBtn.focus();
    await fireEvent.keyDown(settingsBtn, { key: ' ' });
    expect(callSpy.mock.calls.some(([cmd]) => cmd === 'preview_load_and_play')).toBe(false);

    // Space on grid loads and plays selected track, then toggles play/pause.
    grid.focus();
    await fireEvent.keyDown(window, { key: ' ' });
    await vi.waitFor(() =>
      expect(callSpy).toHaveBeenCalledWith('preview_load_and_play', { track_id: track.id }),
    );
    await vi.waitFor(() =>
      expect(within(screen.getByRole('contentinfo', { name: 'Player' })).getByText(track.title)),
    );

    await fireEvent.keyDown(window, { key: ' ' });
    await vi.waitFor(() => expect(callSpy).toHaveBeenCalledWith('preview_toggle_play'));
    await fireEvent.keyDown(window, { key: ' ' });
    await vi.waitFor(() =>
      expect(callSpy.mock.calls.filter(([cmd]) => cmd === 'preview_toggle_play')).toHaveLength(2),
    );

    // Switching to Mix pauses active preview playback.
    await fireEvent.keyDown(window, { key: '4' });
    await vi.waitFor(() => expect(callSpy).toHaveBeenCalledWith('preview_pause'));
  });
});
