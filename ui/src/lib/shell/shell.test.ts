// @vitest-environment happy-dom
import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import App from '../../App.svelte';
import { getTheme, setTheme } from '../design/theme';
import CommandBar, { type Command } from './CommandBar.svelte';
import PlayerBar from './PlayerBar.svelte';
import SettingsSheet from './SettingsSheet.svelte';
import Sidebar, { spaces } from './Sidebar.svelte';
import TopBar from './TopBar.svelte';
import TransitionDock from './TransitionDock.svelte';

describe('theme store', () => {
  beforeEach(() => {
    localStorage.clear();
    delete document.documentElement.dataset.theme;
  });

  it('defaults to system theme and removes data-theme attribute', () => {
    setTheme('system');
    expect(getTheme()).toBe('system');
    expect(document.documentElement.dataset.theme).toBeUndefined();
    expect(localStorage.getItem('resonance-theme')).toBe('system');
  });

  it('sets dark theme and writes to data-theme attribute and localStorage', () => {
    setTheme('dark');
    expect(getTheme()).toBe('dark');
    expect(document.documentElement.dataset.theme).toBe('dark');
    expect(localStorage.getItem('resonance-theme')).toBe('dark');
  });

  it('sets light theme and writes to data-theme attribute and localStorage', () => {
    setTheme('light');
    expect(getTheme()).toBe('light');
    expect(document.documentElement.dataset.theme).toBe('light');
    expect(localStorage.getItem('resonance-theme')).toBe('light');
  });

  it('cleans up data-theme when switching back to system', () => {
    setTheme('dark');
    expect(document.documentElement.dataset.theme).toBe('dark');
    setTheme('system');
    expect(document.documentElement.dataset.theme).toBeUndefined();
  });
});

describe('TopBar', () => {
  it('renders wordmark and utility controls', () => {
    const onsettings = vi.fn();
    render(TopBar, { onsettings });

    expect(screen.getByLabelText('Resonance')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Audio engine status' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Activity' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Settings' })).toBeTruthy();
  });

  it('marks Audio and Activity buttons as disabled with milestone tooltips', () => {
    render(TopBar, { onsettings: vi.fn() });

    const audioBtn = screen.getByRole('button', { name: 'Audio engine status' });
    expect(audioBtn.getAttribute('aria-disabled')).toBe('true');

    const activityBtn = screen.getByRole('button', { name: 'Activity' });
    expect(activityBtn.getAttribute('aria-disabled')).toBe('true');

    const tooltips = screen.getAllByRole('tooltip', { hidden: true });
    expect(tooltips.some((t) => t.textContent?.includes('Audio engine status arrives in M7'))).toBe(
      true,
    );
    expect(tooltips.some((t) => t.textContent?.includes('Activity arrives in M2'))).toBe(true);
  });

  it('triggers onsettings when settings button is clicked', async () => {
    const onsettings = vi.fn();
    render(TopBar, { onsettings });

    const settingsBtn = screen.getByRole('button', { name: 'Settings' });
    await fireEvent.click(settingsBtn);

    expect(onsettings).toHaveBeenCalledOnce();
  });
});

describe('Sidebar', () => {
  it('renders all five spaces in order with primary active space', () => {
    render(Sidebar, {
      activeSpace: 'Library',
      collapsed: false,
      trackCount: 12846,
      onSelectSpace: vi.fn(),
      onToggleCollapse: vi.fn(),
    });

    const spaceNames = spaces.map((s) => s.name);
    expect(spaceNames).toEqual(['Library', 'Prep', 'Sets', 'Mix', 'Replay']);

    const libraryBtn = screen.getByRole('button', { name: 'Library' });
    expect(libraryBtn.getAttribute('aria-current')).toBe('page');
    expect(screen.getByText('12,846')).toBeTruthy();
  });

  it('disables unimplemented spaces with milestone arrival tooltips', async () => {
    const onSelectSpace = vi.fn();
    render(Sidebar, {
      activeSpace: 'Library',
      collapsed: false,
      trackCount: 100,
      onSelectSpace,
      onToggleCollapse: vi.fn(),
    });

    const prepBtn = screen.getByRole('button', { name: 'Prep' });
    const setsBtn = screen.getByRole('button', { name: 'Sets' });
    const replayBtn = screen.getByRole('button', { name: 'Replay' });

    expect(prepBtn.getAttribute('aria-disabled')).toBe('true');
    expect(setsBtn.getAttribute('aria-disabled')).toBe('true');
    expect(replayBtn.getAttribute('aria-disabled')).toBe('true');

    await fireEvent.click(prepBtn);
    expect(onSelectSpace).not.toHaveBeenCalled();

    const tooltips = screen.getAllByRole('tooltip', { hidden: true });
    expect(tooltips.some((t) => t.textContent?.includes('Prep arrives in M3'))).toBe(true);
    expect(tooltips.some((t) => t.textContent?.includes('Sets arrives in M10'))).toBe(true);
    expect(tooltips.some((t) => t.textContent?.includes('Replay arrives in M9'))).toBe(true);
  });

  it('navigates to enabled spaces when clicked', async () => {
    const onSelectSpace = vi.fn();
    render(Sidebar, {
      activeSpace: 'Library',
      collapsed: false,
      trackCount: 100,
      onSelectSpace,
      onToggleCollapse: vi.fn(),
    });

    const mixBtn = screen.getByRole('button', { name: 'Mix' });
    expect(mixBtn.getAttribute('aria-disabled')).toBeNull();

    await fireEvent.click(mixBtn);
    expect(onSelectSpace).toHaveBeenCalledWith('Mix');
  });

  it('renders collections sub-tree and active set placeholders when open', () => {
    render(Sidebar, {
      activeSpace: 'Library',
      collapsed: false,
      trackCount: 100,
      onSelectSpace: vi.fn(),
      onToggleCollapse: vi.fn(),
    });

    expect(screen.getByText('Collections')).toBeTruthy();
    expect(screen.getByText('None yet. Collections arrive in M2.')).toBeTruthy();
    expect(screen.getByText('Active set')).toBeTruthy();
    expect(screen.getByText('No set yet. Sets arrive in M10.')).toBeTruthy();
  });

  it('hides text, collections and active set when collapsed', () => {
    render(Sidebar, {
      activeSpace: 'Library',
      collapsed: true,
      trackCount: 100,
      onSelectSpace: vi.fn(),
      onToggleCollapse: vi.fn(),
    });

    expect(screen.queryByText('Collections')).toBeNull();
    expect(screen.queryByText('None yet. Collections arrive in M2.')).toBeNull();
    expect(screen.queryByText('Active set')).toBeNull();
    expect(screen.getByRole('button', { name: 'Expand the sidebar' })).toBeTruthy();
  });

  it('triggers onToggleCollapse when collapse button is clicked', async () => {
    const onToggleCollapse = vi.fn();
    render(Sidebar, {
      activeSpace: 'Library',
      collapsed: false,
      trackCount: 100,
      onSelectSpace: vi.fn(),
      onToggleCollapse,
    });

    const toggleBtn = screen.getByRole('button', { name: 'Collapse the sidebar' });
    await fireEvent.click(toggleBtn);

    expect(onToggleCollapse).toHaveBeenCalledOnce();
  });
});

describe('CommandBar', () => {
  it('renders search input with combobox role and shortcut badge', () => {
    render(CommandBar, {
      commands: [{ name: 'Scan a folder', run: vi.fn() }],
      inputId: 'test-cmd-input',
    });

    const input = screen.getByRole('combobox', { name: 'Commands' });
    expect(input).toBeTruthy();
    expect(input.getAttribute('placeholder')).toContain('Search your library');
  });

  it('renders disabled slash chips with milestone arrival tooltips', () => {
    render(CommandBar, {
      commands: [],
      inputId: 'test-cmd-input',
    });

    const bpmChip = screen.getByRole('button', { name: '/ bpm 125-132 Filter by tempo' });
    expect(bpmChip.getAttribute('aria-disabled')).toBe('true');

    const keyChip = screen.getByRole('button', { name: '/ key 8A Compatible with A minor' });
    expect(keyChip.getAttribute('aria-disabled')).toBe('true');

    const similarChip = screen.getByRole('button', { name: '/ similar Find matching tracks' });
    expect(similarChip.getAttribute('aria-disabled')).toBe('true');

    const buildChip = screen.getByRole('button', { name: '/ build set Generate 90m progression' });
    expect(buildChip.getAttribute('aria-disabled')).toBe('true');

    const tooltips = screen.getAllByRole('tooltip', { hidden: true });
    expect(tooltips.some((t) => t.textContent?.includes('arrives in M2'))).toBe(true);
    expect(tooltips.some((t) => t.textContent?.includes('arrives in M5'))).toBe(true);
    expect(tooltips.some((t) => t.textContent?.includes('arrives in M6'))).toBe(true);
    expect(tooltips.some((t) => t.textContent?.includes('arrives in M10'))).toBe(true);
  });

  it('filters commands and executes runnable commands on selection', async () => {
    const scanRun = vi.fn();
    const commands: Command[] = [
      { name: 'Scan a music folder', run: scanRun },
      { name: 'Future AI action', arrives: 'M6' },
    ];

    render(CommandBar, { commands, inputId: 'test-cmd-input' });

    const input = screen.getByRole('combobox', { name: 'Commands' });
    await fireEvent.focus(input);

    const scanOption = screen.getByRole('option', { name: 'Scan a music folder' });
    await fireEvent.mouseDown(scanOption);

    expect(scanRun).toHaveBeenCalledOnce();
  });
});

describe('TransitionDock', () => {
  it('renders header with transition pair and disabled controls', () => {
    render(TransitionDock, { collapsed: false, ontoggle: vi.fn() });

    expect(screen.getByText('A → B')).toBeTruthy();
    expect(screen.getByText('Transition')).toBeTruthy();

    const mixPoint = screen.getByRole('button', { name: 'Mix Point' });
    expect(mixPoint.getAttribute('aria-disabled')).toBe('true');

    const blend = screen.getByRole('button', { name: 'Blend' });
    expect(blend.getAttribute('aria-disabled')).toBe('true');

    const tooltips = screen.getAllByRole('tooltip', { hidden: true });
    expect(tooltips.some((t) => t.textContent?.includes('Mix points arrive in M10'))).toBe(true);
    expect(tooltips.some((t) => t.textContent?.includes('Blends arrive in M10'))).toBe(true);
  });

  it('renders Deck A, Deck B and crossfade placeholder when expanded', () => {
    render(TransitionDock, { collapsed: false, ontoggle: vi.fn() });

    expect(screen.getByText('Deck A is empty')).toBeTruthy();
    expect(screen.getByText('Deck B is empty')).toBeTruthy();
    expect(
      screen.getByText('Two loaded decks show their crossfade here. Decks arrive in M7.'),
    ).toBeTruthy();
  });

  it('hides dock body when collapsed and expands on toggle click', async () => {
    const ontoggle = vi.fn();
    const { rerender } = render(TransitionDock, { collapsed: true, ontoggle });

    expect(screen.queryByText('Deck A is empty')).toBeNull();
    const toggleBtn = screen.getByRole('button', { name: 'Expand the dock' });
    await fireEvent.click(toggleBtn);
    expect(ontoggle).toHaveBeenCalledOnce();

    rerender({ collapsed: false, ontoggle });
    expect(screen.getByText('Deck A is empty')).toBeTruthy();
  });
});

describe('PlayerBar', () => {
  it('renders all player and mixer controls with milestone arrival notices', () => {
    render(PlayerBar);

    expect(screen.getByRole('button', { name: 'Play' }).getAttribute('aria-disabled')).toBe('true');
    expect(screen.getByRole('button', { name: 'Pause' }).getAttribute('aria-disabled')).toBe(
      'true',
    );
    expect(screen.getByText('Nothing playing')).toBeTruthy();

    const sliders = screen.getAllByRole('slider');
    expect(sliders).toHaveLength(3);
    const timeline = screen.getByRole('slider', { name: 'Scrub' });
    expect(timeline.hasAttribute('disabled')).toBe(true);
    const volA = screen.getByRole('slider', { name: 'A Vol' });
    expect(volA.hasAttribute('disabled')).toBe(true);
    const volB = screen.getByRole('slider', { name: 'B Vol' });
    expect(volB.hasAttribute('disabled')).toBe(true);

    expect(screen.getByRole('button', { name: 'Curve' }).getAttribute('aria-disabled')).toBe(
      'true',
    );
    expect(screen.getByRole('button', { name: 'EQ' }).getAttribute('aria-disabled')).toBe('true');
    expect(
      screen.getByRole('button', { name: 'Headphone cue' }).getAttribute('aria-disabled'),
    ).toBe('true');

    const tooltips = screen.getAllByRole('tooltip', { hidden: true });
    expect(tooltips.some((t) => t.textContent?.includes('Preview playback arrives in M2'))).toBe(
      true,
    );
    expect(tooltips.some((t) => t.textContent?.includes('Deck volume arrives in M7'))).toBe(true);
    expect(
      tooltips.some(
        (t) =>
          t.textContent?.includes('Crossfader curves arrives in M7') ||
          t.textContent?.includes('Crossfader curves arrive in M7'),
      ),
    ).toBe(true);
    expect(tooltips.some((t) => t.textContent?.includes('EQ arrives in M7'))).toBe(true);
    expect(tooltips.some((t) => t.textContent?.includes('Headphone cue arrives in M7'))).toBe(true);
  });
});

describe('SettingsSheet', () => {
  it('renders theme switcher and future settings milestone placeholders', async () => {
    render(SettingsSheet, { open: true, onclose: vi.fn() });

    const dialog = screen.getByRole('dialog');
    expect(dialog).toBeTruthy();
    expect(within(dialog).getByText('Settings')).toBeTruthy();

    expect(within(dialog).getByRole('tab', { name: 'Dark' })).toBeTruthy();
    expect(within(dialog).getByRole('tab', { name: 'Light' })).toBeTruthy();
    expect(within(dialog).getByRole('tab', { name: 'System' })).toBeTruthy();

    expect(within(dialog).getByText('Library sources arrive in M2.')).toBeTruthy();
    expect(within(dialog).getByText('AI provider keys arrive in M3.')).toBeTruthy();
    expect(
      within(dialog).getByText('Audio and MIDI device configuration arrives in M7.'),
    ).toBeTruthy();
  });

  it('switches themes when tab is clicked', async () => {
    render(SettingsSheet, { open: true, onclose: vi.fn() });

    const dialog = screen.getByRole('dialog');
    const lightTab = within(dialog).getByRole('tab', { name: 'Light' });
    await fireEvent.click(lightTab);

    expect(getTheme()).toBe('light');
    expect(document.documentElement.dataset.theme).toBe('light');

    const darkTab = within(dialog).getByRole('tab', { name: 'Dark' });
    await fireEvent.click(darkTab);

    expect(getTheme()).toBe('dark');
    expect(document.documentElement.dataset.theme).toBe('dark');
  });

  it('calls onclose when Done is clicked', async () => {
    const onclose = vi.fn();
    render(SettingsSheet, { open: true, onclose });

    const doneBtn = screen.getByRole('button', { name: 'Done' });
    await fireEvent.click(doneBtn);

    expect(onclose).toHaveBeenCalledOnce();
  });
});

describe('App shell integration and keyboard shortcuts', () => {
  beforeEach(() => {
    window.innerWidth = 1586;
    window.innerHeight = 992;
  });

  it('renders full shell layout with all regions', () => {
    render(App);

    expect(screen.getAllByRole('banner')).toHaveLength(2); // TopBar and CommandBar
    expect(screen.getByRole('navigation', { name: 'Spaces' })).toBeTruthy(); // Sidebar
    expect(screen.getByRole('combobox', { name: 'Commands' })).toBeTruthy(); // CommandBar
    expect(screen.getByRole('region', { name: 'Transition' })).toBeTruthy(); // Dock
    expect(screen.getByRole('contentinfo', { name: 'Player' })).toBeTruthy(); // PlayerBar
  });

  it('auto-collapses sidebar and dock when window is smaller than breakpoints', () => {
    window.innerWidth = 1280;
    window.innerHeight = 720;
    render(App);

    expect(screen.getByRole('button', { name: 'Expand the sidebar' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Expand the dock' })).toBeTruthy();
  });

  it('toggles sidebar collapse with [ key', async () => {
    render(App);

    expect(screen.getByRole('button', { name: 'Collapse the sidebar' })).toBeTruthy();

    await fireEvent.keyDown(window, { key: '[' });
    expect(screen.getByRole('button', { name: 'Expand the sidebar' })).toBeTruthy();

    await fireEvent.keyDown(window, { key: '[' });
    expect(screen.getByRole('button', { name: 'Collapse the sidebar' })).toBeTruthy();
  });

  it('toggles dock collapse with D key', async () => {
    render(App);

    expect(screen.getByRole('button', { name: 'Collapse the dock' })).toBeTruthy();

    await fireEvent.keyDown(window, { key: 'd' });
    expect(screen.getByRole('button', { name: 'Expand the dock' })).toBeTruthy();

    await fireEvent.keyDown(window, { key: 'd' });
    expect(screen.getByRole('button', { name: 'Collapse the dock' })).toBeTruthy();
  });

  it('toggles inspector with I key', async () => {
    render(App);

    expect(screen.getByRole('complementary', { name: 'Inspector' })).toBeTruthy();

    await fireEvent.keyDown(window, { key: 'i' });
    expect(screen.queryByRole('complementary', { name: 'Inspector' })).toBeNull();

    await fireEvent.keyDown(window, { key: 'i' });
    expect(screen.getByRole('complementary', { name: 'Inspector' })).toBeTruthy();
  });

  it('switches between Library and Mix spaces with 1 and 4 keys', async () => {
    render(App);

    const libBtn = screen.getByRole('button', { name: 'Library' });
    expect(libBtn.getAttribute('aria-current')).toBe('page');

    await fireEvent.keyDown(window, { key: '4' });
    const mixBtn = screen.getByRole('button', { name: 'Mix' });
    expect(mixBtn.getAttribute('aria-current')).toBe('page');

    await fireEvent.keyDown(window, { key: '1' });
    expect(screen.getByRole('button', { name: 'Library' }).getAttribute('aria-current')).toBe(
      'page',
    );
  });
});
