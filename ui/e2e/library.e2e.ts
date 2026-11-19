import { copyFileSync, existsSync, mkdirSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

import { expect, test } from '@playwright/test';

import { musicFolder } from './core';

// The later tests build on the library the first one scanned.
test.describe.configure({ mode: 'serial' });

test('a scanned folder fills the table, and a selected track fills the inspector', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1586, height: 992 });
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'No tracks yet' })).toBeVisible();

  await page.getByRole('button', { name: 'Add a music folder' }).click();
  const sheet = page.getByRole('dialog', { name: 'Scan a music folder' });
  await sheet.getByRole('textbox', { name: 'Music folder' }).fill(musicFolder);
  await sheet.getByRole('button', { name: 'Scan folder' }).click();
  await expect(sheet.getByRole('status')).toContainText('5 files: 5 added, 0 changed, 0 unchanged');
  await sheet.getByRole('button', { name: 'Close' }).click();

  await expect(page.getByText('5 tracks')).toBeVisible();
  const table = page.getByRole('grid', { name: 'Tracks' });
  for (const header of ['#', 'Title', 'Artist', 'BPM', 'Key', 'Time']) {
    await expect(table.getByRole('columnheader', { name: header })).toBeVisible();
  }
  for (const title of [
    'Ada Marsh - First Light',
    'Ada Marsh - Second Wind',
    'Kofi Bell - Third Rail',
    'Nia Okoro - Fourth Wall',
    'Nia Okoro - Fifth Floor',
  ]) {
    await expect(table.getByText(title)).toBeVisible();
  }

  await table.getByText('Kofi Bell - Third Rail').click();

  const inspector = page.getByRole('complementary', { name: 'Inspector' });
  await expect(inspector.getByRole('heading', { name: 'Kofi Bell - Third Rail' })).toBeVisible();
  await expect(
    inspector.getByRole('img', { name: 'Kofi Bell - Third Rail artwork' }),
  ).toBeVisible();
  await expect(inspector).toContainText('No artist tag');
  await expect(inspector).toContainText('Garage');

  const filters = page.getByRole('toolbar', { name: 'Track filters' });
  await filters.getByRole('button', { name: 'Analysed', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Analysed arrives in M5' })).toBeVisible();
  await filters.getByRole('button', { name: 'Analysed', exact: true }).click();
  await expect(table).toBeVisible();

  await page.setViewportSize({ width: 1100, height: 700 });
  for (const header of ['#', 'Title', 'Artist', 'BPM', 'Key', 'Time']) {
    await expect(table.getByRole('columnheader', { name: header })).toBeVisible();
  }
});

test('Mix renders the header, dual waveform lanes and both full decks in dark and light themes', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1586, height: 992 });
  await page.goto('/');
  await page.getByRole('button', { name: 'Mix', exact: true }).click();

  await expect(page.getByRole('heading', { name: 'Mix' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Audio device' })).toContainText('Default output');
  await expect(page.getByRole('button', { name: 'Record session' })).toBeVisible();

  const lanes = await page.locator('canvas.lanes').boundingBox();
  expect(lanes && Math.round(lanes.height)).toBe(221);

  for (const name of ['Deck A', 'Deck B'] as const) {
    const deck = page.getByRole('region', { name });
    await expect(deck).toContainText(`${name} is empty`);
    await expect(deck.getByRole('combobox', { name: `Track on ${name}` })).toBeEnabled();
    await expect(deck.getByRole('button', { name: 'Play' })).toBeDisabled();
  }

  await page.evaluate(() => {
    document.documentElement.dataset.theme = 'light';
  });
  await expect(page.getByRole('region', { name: 'Deck A' })).toBeVisible();
});

test('a folder that is not there is reported, and the library stays as it was', async ({
  page,
}) => {
  await page.goto('/');
  await expect(
    page.getByRole('grid', { name: 'Tracks' }).getByText('Ada Marsh - First Light'),
  ).toBeVisible();

  await page.getByRole('combobox', { name: 'Commands' }).fill('scan');
  await page.getByRole('option', { name: 'Scan a music folder' }).click();
  const sheet = page.getByRole('dialog', { name: 'Scan a music folder' });
  await sheet.getByRole('textbox', { name: 'Music folder' }).fill(`${musicFolder}-missing`);
  await sheet.getByRole('button', { name: 'Scan folder' }).click();

  await expect(sheet.getByRole('alert')).toContainText('-missing');
  await sheet.getByRole('button', { name: 'Close' }).click();
  await expect(
    page.getByRole('grid', { name: 'Tracks' }).getByText('Ada Marsh - First Light'),
  ).toBeVisible();
});

test('subfolder toggles update the library, and a watched source picks up new and renamed files', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1586, height: 992 });
  await page.goto('/');
  await expect(page.getByText('5 tracks')).toBeVisible();

  await page.getByRole('button', { name: 'Settings' }).click();
  const dialog = page.getByRole('dialog', { name: 'Settings' });
  const card = dialog.getByRole('region', { name: musicFolder });
  await expect(card).toContainText('5 tracks');
  await expect(card).toContainText('Watching');

  await card.getByRole('button', { name: 'Subfolders' }).click();
  await card.getByRole('checkbox', { name: 'Include Garage' }).click();
  await expect(card).toContainText('3 tracks');
  await expect(card).toContainText('1 off');
  await dialog.getByRole('button', { name: 'Done' }).click();

  await expect(page.getByText('3 tracks', { exact: true })).toBeVisible();
  const table = page.getByRole('grid', { name: 'Tracks' });
  await expect(table.getByText('Kofi Bell - Third Rail')).toHaveCount(0);
  await expect(table.getByText('Ada Marsh - First Light')).toBeVisible();

  const addedTrack = join(musicFolder, 'House', 'Zara Lin - Sixth Sense.wav');
  const originalGarage = join(musicFolder, 'Garage', 'Kofi Bell - Third Rail.aiff');
  const renamedGarage = join(musicFolder, 'Garage', 'Kofi Bell - Rail Yard.aiff');
  try {
    copyFileSync(join(musicFolder, 'House', 'Ada Marsh - First Light.wav'), addedTrack);
    await expect(page.getByText('4 tracks', { exact: true })).toBeVisible({ timeout: 10_000 });
    await expect(table.getByText('Zara Lin - Sixth Sense')).toBeVisible();

    await page.getByRole('button', { name: 'Settings' }).click();
    await card.getByRole('checkbox', { name: 'Include Garage' }).click();
    await expect(card).toContainText('6 tracks');
    await dialog.getByRole('button', { name: 'Done' }).click();

    await expect(page.getByText('6 tracks', { exact: true })).toBeVisible();
    await expect(table.getByText('Kofi Bell - Third Rail')).toBeVisible();

    renameSync(originalGarage, renamedGarage);
    await expect(table.getByText('Kofi Bell - Rail Yard')).toBeVisible({ timeout: 10_000 });
    await expect(table.getByText('Kofi Bell - Third Rail')).toHaveCount(0);
    await expect(page.getByText('6 tracks', { exact: true })).toBeVisible();
  } finally {
    if (existsSync(renamedGarage)) {
      renameSync(renamedGarage, originalGarage);
    }
    rmSync(addedTrack, { force: true });
  }
});

test('instant search, filter drawer, favourites and column sorting filter the real library', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1586, height: 992 });
  await page.goto('/');
  await expect(page.getByText('6 tracks', { exact: true })).toBeVisible({ timeout: 10_000 });

  const table = page.getByRole('grid', { name: 'Tracks' });
  const search = page.getByRole('combobox', { name: 'Commands' });

  await search.fill('Ada');
  await expect(page.getByText('2 tracks', { exact: true })).toBeVisible();
  await expect(table.getByText('Ada Marsh - First Light')).toBeVisible();
  await expect(table.getByText('Kofi Bell - Third Rail')).toHaveCount(0);

  await search.press('Escape');
  await expect(page.getByText('6 tracks', { exact: true })).toBeVisible();

  await search.fill('unmatchedphrase');
  await expect(page.getByRole('heading', { name: 'No tracks match' })).toBeVisible();
  await page.getByRole('button', { name: 'Clear filters' }).click();
  await expect(page.getByText('6 tracks', { exact: true })).toBeVisible();

  await table.getByText('Ada Marsh - First Light').click();
  const inspector = page.getByRole('complementary', { name: 'Inspector' });
  await inspector.getByRole('button', { name: 'Favourite track' }).click();
  await expect(inspector.getByRole('button', { name: 'Favourite track' })).toHaveAttribute(
    'aria-pressed',
    'true',
  );

  const pills = page.getByRole('toolbar', { name: 'Track filters' });
  await pills.getByRole('button', { name: 'Favourites' }).click();
  await expect(page.getByText('1 track', { exact: true })).toBeVisible();
  await expect(table.getByText('Ada Marsh - First Light')).toBeVisible();
  await pills.getByRole('button', { name: 'Favourites' }).click();
  await expect(page.getByText('6 tracks', { exact: true })).toBeVisible();

  await page.getByRole('button', { name: 'Filters' }).click();
  const drawer = page.getByRole('dialog', { name: 'Filters' });
  await drawer.getByRole('checkbox', { name: 'FLAC' }).check();
  await expect(page.getByText('1 track', { exact: true })).toBeVisible();
  await expect(table.getByText('Ada Marsh - Second Wind')).toBeVisible();
  await drawer.getByRole('button', { name: 'Clear' }).click();
  await expect(page.getByText('6 tracks', { exact: true })).toBeVisible();
  await drawer.getByRole('button', { name: 'Done' }).click();
  await expect(drawer).toBeHidden();

  const settings = page.getByRole('dialog', { name: 'Settings' });
  await page.getByRole('button', { name: 'Settings' }).click();
  await settings.getByRole('tab', { name: 'Open Key' }).click();
  await settings.getByRole('button', { name: 'Done' }).click();
  await page.reload();
  await expect(page.getByText('6 tracks', { exact: true })).toBeVisible({ timeout: 10_000 });
  await page.getByRole('button', { name: 'Filters' }).click();
  const keys = drawer.getByRole('group', { name: 'Keys' });
  await keys.getByRole('button', { name: '1m (Am)' }).click();
  await expect(keys.getByRole('button', { name: '1m (Am)' })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  await expect(page.getByRole('heading', { name: 'No tracks match' })).toBeVisible();
  await drawer.getByRole('button', { name: 'Clear' }).click();
  await expect(page.getByText('6 tracks', { exact: true })).toBeVisible();
  await drawer.getByRole('button', { name: 'Done' }).click();
  await page.getByRole('button', { name: 'Settings' }).click();
  await settings.getByRole('tab', { name: 'Camelot' }).click();
  await settings.getByRole('button', { name: 'Done' }).click();
  await expect(settings).toBeHidden();

  const titleHeader = table.getByRole('columnheader', { name: 'Title' });
  await titleHeader.getByRole('button', { name: 'Title' }).click();
  await expect(titleHeader).toHaveAttribute('aria-sort', 'ascending');
  await expect(table.locator('.body [role="row"]').first()).toContainText(
    'Ada Marsh - First Light',
  );

  await titleHeader.getByRole('button', { name: 'Title' }).click();
  await expect(titleHeader).toHaveAttribute('aria-sort', 'descending');
  await expect(table.locator('.body [role="row"]').first()).toContainText('Zara Lin - Sixth Sense');

  await titleHeader.getByRole('button', { name: 'Title' }).click();
  await expect(titleHeader).toHaveAttribute('aria-sort', 'none');
});

test('manual collections create, assign via multi-select, inspect, persist and delete safely', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1586, height: 992 });
  await page.goto('/');
  await expect(page.getByText('6 tracks', { exact: true })).toBeVisible({ timeout: 10_000 });

  const sidebar = page.getByRole('navigation', { name: 'Spaces' });
  const table = page.getByRole('grid', { name: 'Tracks' });
  const inspector = page.getByRole('complementary', { name: 'Inspector' });

  await sidebar.getByRole('button', { name: 'New collection' }).click();
  const createInput = sidebar.getByRole('textbox', { name: 'New collection name' });
  await createInput.fill('Warm Up');
  await createInput.press('Enter');

  const warmRow = sidebar.getByRole('button', { name: /^Warm Up/ });
  await expect(warmRow).toBeVisible();
  await warmRow.click();
  await expect(page.getByRole('heading', { name: 'Warm Up' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'This collection is empty' })).toBeVisible();
  await page.getByRole('button', { name: 'Show all tracks' }).click();
  await expect(page.getByText('6 tracks', { exact: true })).toBeVisible();

  await table.getByRole('checkbox', { name: 'Tick Ada Marsh - First Light' }).click();
  await table.getByRole('checkbox', { name: 'Tick Kofi Bell - Third Rail' }).click();
  const selection = page.getByRole('region', { name: 'Selected tracks' });
  await expect(selection.getByText('2 selected')).toBeVisible();
  await selection.getByRole('button', { name: 'Add to collection' }).click();
  await page.getByRole('menuitemcheckbox', { name: /Warm Up/ }).click();
  await expect(warmRow).toContainText('2');

  await table.getByText('Ada Marsh - First Light').click();
  await expect(inspector.getByRole('button', { name: 'Remove Warm Up' })).toBeVisible();

  await warmRow.click();
  await expect(page.getByText('2 tracks', { exact: true })).toBeVisible();

  const sidebarOverflow = await page.evaluate(() =>
    Array.from(document.querySelectorAll('nav *'))
      .filter((el) => el.scrollWidth > el.clientWidth + 1)
      .map((el) => el.textContent?.trim()),
  );
  expect(sidebarOverflow).toEqual([]);

  await table.getByText('Ada Marsh - First Light').click();
  await inspector.getByRole('button', { name: 'Remove Warm Up' }).click();
  await expect(page.getByText('1 track', { exact: true })).toBeVisible();
  await expect(warmRow).toContainText('1');

  await table.getByText('Kofi Bell - Third Rail').click();
  await expect(inspector.getByRole('button', { name: 'Remove Warm Up' })).toBeVisible();

  const selectedRow = table.locator('[role="row"][aria-selected="true"]');
  await selectedRow.hover();
  const playIcon = selectedRow.locator('.status-icon');
  const rowStar = selectedRow.getByRole('button', { name: 'Favourite Kofi Bell - Third Rail' });
  await expect(playIcon).toBeVisible();
  await expect(rowStar).toBeVisible();
  const [playBox, starBox] = await Promise.all([playIcon.boundingBox(), rowStar.boundingBox()]);
  expect(playBox && starBox && playBox.x + playBox.width < starBox.x).toBe(true);

  await page.setViewportSize({ width: 1100, height: 700 });
  const collapsedLib = sidebar.getByRole('button', { name: 'Library' });
  await expect
    .poll(() =>
      collapsedLib.evaluate((el) => ({
        bg: getComputedStyle(el).backgroundColor,
        barDisplay: getComputedStyle(el, '::before').display,
      })),
    )
    .toEqual({
      bg: expect.not.stringMatching(/^rgba\(0,\s*0,\s*0,\s*0\)$/),
      barDisplay: 'block',
    });
  await collapsedLib.hover();
  await expect(sidebar.getByText('Library · Warm Up')).toBeVisible();

  await page.setViewportSize({ width: 1586, height: 992 });
  await sidebar.getByRole('button', { name: 'Actions for Warm Up' }).click();
  await page.getByRole('menuitem', { name: 'Rename' }).click();
  const renameInput = sidebar.getByRole('textbox', { name: 'Rename collection' });
  await renameInput.fill('Late Night');
  await renameInput.press('Enter');
  await expect(page.getByRole('heading', { name: 'Late Night' })).toBeVisible();
  await expect(inspector.getByRole('button', { name: 'Remove Late Night' })).toBeVisible();

  await page.reload();
  await expect(page.getByText('6 tracks', { exact: true })).toBeVisible({ timeout: 10_000 });
  await sidebar.getByRole('button', { name: /^Late Night/ }).click();
  await expect(page.getByText('1 track', { exact: true })).toBeVisible();
  await expect(table.getByText('Kofi Bell - Third Rail')).toBeVisible();

  await sidebar.getByRole('button', { name: 'Actions for Late Night' }).click();
  await page.getByRole('menuitem', { name: 'Delete' }).click();
  const confirmSheet = page.getByRole('dialog', { name: 'Delete collection' });
  await expect(confirmSheet).toContainText('Tracks and files on disk will stay');
  await confirmSheet.getByRole('button', { name: 'Delete collection' }).click();

  await expect(sidebar.getByRole('button', { name: /^Late Night/ })).toHaveCount(0);
  await expect(page.getByRole('heading', { name: 'Library' })).toBeVisible();
  await expect(page.getByText('6 tracks', { exact: true })).toBeVisible();
});

test('selecting, selecting all and clearing never move the table or the selection buttons', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1586, height: 992 });
  await page.goto('/');
  await expect(page.getByText('6 tracks', { exact: true })).toBeVisible({ timeout: 10_000 });

  const table = page.getByRole('grid', { name: 'Tracks' });
  const selection = page.getByRole('region', { name: 'Selected tracks' });
  const header = table.locator('.head [role="row"]');
  const rows = table.locator('.rows [role="row"]');
  const clickedTick = rows.nth(2).getByRole('checkbox');

  const tableBoxes = async () => ({
    header: await header.boundingBox(),
    first: await rows.first().boundingBox(),
    clicked: await rows.nth(2).boundingBox(),
  });
  const buttonBoxes = async () =>
    Promise.all(
      ['Select all', 'Add to collection', 'Clear'].map((name) =>
        selection.getByRole('button', { name }).boundingBox(),
      ),
    );

  const before = await tableBoxes();
  expect(before.header && before.first && before.clicked).toBeTruthy();

  await clickedTick.click();
  await expect(selection.getByText('1 selected')).toBeVisible();
  expect(await tableBoxes()).toEqual(before);
  const buttonsBefore = await buttonBoxes();

  await selection.getByRole('button', { name: 'Select all' }).click();
  await expect(selection.getByText('6 selected')).toBeVisible();
  await expect(selection.getByRole('button', { name: 'Select all' })).toHaveAttribute(
    'aria-disabled',
    'true',
  );
  expect(await tableBoxes()).toEqual(before);
  expect(await buttonBoxes()).toEqual(buttonsBefore);

  await selection.getByRole('button', { name: 'Clear' }).click();
  await expect(selection).toBeHidden();
  await expect(page.getByRole('toolbar', { name: 'Track filters' })).toBeVisible();
  expect(await tableBoxes()).toEqual(before);
  await expect(table).toBeFocused();

  await clickedTick.click();
  await expect(selection.getByText('1 selected')).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(selection).toBeHidden();
  expect(await tableBoxes()).toEqual(before);
});

test('preview plays from row, Inspector, double-click and Spacebar with PlayerBar seek, volume and Mix pause', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1586, height: 992 });
  await page.goto('/');
  await expect(page.getByText('6 tracks', { exact: true })).toBeVisible({ timeout: 10_000 });

  const table = page.getByRole('grid', { name: 'Tracks' });
  const inspector = page.getByRole('complementary', { name: 'Inspector' });
  const player = page.getByRole('contentinfo', { name: 'Player' });

  await expect(player).toContainText('Nothing playing');

  // 1. Start preview from the row status button.
  await table.getByRole('button', { name: 'Play Ada Marsh - First Light' }).click();
  await expect(player).toContainText('Ada Marsh - First Light');
  await expect(player.getByRole('button', { name: 'Pause' })).toBeVisible();
  await expect(table.getByRole('button', { name: 'Pause Ada Marsh - First Light' })).toBeVisible();

  // Telemetry advances elapsed time.
  await expect(player.locator('.time.elapsed')).not.toHaveText('0:00', { timeout: 5_000 });

  // 2. Scrub seeks cleanly into the track.
  const scrub = player.getByRole('slider', { name: 'Scrub' });
  await scrub.fill('9000');
  await expect(player.locator('.time.elapsed')).toHaveText(/0:(09|1[0-5])/);

  // 3. Mute toggles off/on and Volume updates.
  await player.getByRole('button', { name: 'Mute' }).click();
  await expect(player.getByRole('button', { name: 'Unmute' })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  await player.getByRole('slider', { name: 'Volume' }).fill('65');
  await expect(player.getByRole('button', { name: 'Mute' })).toBeVisible();

  // 4. Double-clicking another track switches preview playback.
  await table.getByText('Ada Marsh - Second Wind').dblclick();
  await expect(player).toContainText('Ada Marsh - Second Wind');
  await expect(
    inspector.getByRole('button', { name: 'Pause Ada Marsh - Second Wind' }),
  ).toBeVisible();

  // 5. Inspector Play/Pause toggles playback.
  await inspector.getByRole('button', { name: 'Pause Ada Marsh - Second Wind' }).click();
  await expect(player.getByRole('button', { name: 'Play' })).toBeVisible();
  await inspector.getByRole('button', { name: 'Play Ada Marsh - Second Wind' }).click();
  await expect(player.getByRole('button', { name: 'Pause' })).toBeVisible();

  // 6. Spacebar is ignored inside the search input and open dialogs, and toggles on the grid.
  const search = page.getByRole('combobox', { name: 'Commands' });
  await search.focus();
  await page.keyboard.press('Space');
  await expect(player.getByRole('button', { name: 'Pause' })).toBeVisible();
  await search.fill('');

  await page.getByRole('button', { name: 'Settings' }).click();
  const settings = page.getByRole('dialog', { name: 'Settings' });
  await expect(settings).toBeVisible();
  await page.keyboard.press('Space');
  await settings.getByRole('button', { name: 'Done' }).click();
  await expect(player.getByRole('button', { name: 'Pause' })).toBeVisible();

  await table.focus();
  await page.keyboard.press('Space');
  await expect(player.getByRole('button', { name: 'Play' })).toBeVisible();
  await page.keyboard.press('Space');
  await expect(player.getByRole('button', { name: 'Pause' })).toBeVisible();

  // 7. Switching to Mix hides PlayerBar and pauses preview playback.
  await page.getByRole('button', { name: 'Mix', exact: true }).click();
  await expect(player).toHaveCount(0);
  await page.getByRole('button', { name: 'Library', exact: true }).click();
  await expect(player).toBeVisible();
  await expect(player.getByRole('button', { name: 'Play' })).toBeVisible();

  // 8. At 1100x700 the PlayerBar stays within the viewport without horizontal overflow.
  await page.setViewportSize({ width: 1100, height: 700 });
  const playerBox = await player.boundingBox();
  expect(playerBox && playerBox.x + playerBox.width <= 1100).toBe(true);
});

test('missing files stay in the library and collections, warn on mismatched audio, and relink to moved copies', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1586, height: 992 });
  await page.goto('/');
  await expect(page.getByText('6 tracks', { exact: true })).toBeVisible({ timeout: 10_000 });

  const sidebar = page.getByRole('navigation', { name: 'Spaces' });
  const table = page.getByRole('grid', { name: 'Tracks' });
  const inspector = page.getByRole('complementary', { name: 'Inspector' });

  // Assign Kofi Bell - Third Rail to a collection and favourites before its file disappears.
  await sidebar.getByRole('button', { name: 'New collection' }).click();
  const createInput = sidebar.getByRole('textbox', { name: 'New collection name' });
  await createInput.fill('Crate');
  await createInput.press('Enter');
  const crateRow = sidebar.getByRole('button', { name: /^Crate/ });
  await expect(crateRow).toBeVisible();

  await table.getByText('Kofi Bell - Third Rail').click();
  await inspector.getByRole('button', { name: 'Favourite track' }).click();
  await expect(inspector.getByRole('button', { name: 'Favourite track' })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  await inspector.getByRole('button', { name: 'Add to collection' }).click();
  await page.getByRole('menuitemcheckbox', { name: /Crate/ }).click();
  await expect(crateRow).toContainText('1');

  const originalGarage = join(musicFolder, 'Garage', 'Kofi Bell - Third Rail.aiff');
  const relocatedDir = join(musicFolder, '..', 'relocated');
  const relocatedGarage = join(relocatedDir, 'Kofi Bell - Third Rail Moved.aiff');
  const unrelatedWav = join(musicFolder, 'House', 'Ada Marsh - First Light.wav');

  mkdirSync(relocatedDir, { recursive: true });
  try {
    copyFileSync(originalGarage, relocatedGarage);
    rmSync(originalGarage, { force: true });

    // The watched source flags the file missing without removing the track or its collection membership.
    await expect(
      table.getByRole('button', { name: 'Missing file for Kofi Bell - Third Rail — Relink' }),
    ).toBeVisible({ timeout: 10_000 });
    await expect(page.getByText('6 tracks', { exact: true })).toBeVisible();
    await expect(crateRow).toContainText('1');
    await expect(inspector.getByText('Missing file')).toBeVisible();
    await expect(
      inspector.getByRole('button', { name: 'Play Kofi Bell - Third Rail' }),
    ).toBeDisabled();

    // Filter drawer isolates missing files cleanly.
    await page.getByRole('button', { name: 'Filters' }).click();
    const drawer = page.getByRole('dialog', { name: 'Filters' });
    await drawer.getByRole('checkbox', { name: 'Missing files only' }).check();
    await expect(table.getByText('Kofi Bell - Third Rail')).toBeVisible();
    await expect(table.getByText('Ada Marsh - First Light')).toHaveCount(0);
    await drawer.getByRole('button', { name: 'Clear' }).click();
    await drawer.getByRole('button', { name: 'Done' }).click();

    // Reselect Kofi Bell - Third Rail after clearing filters and inspect a mismatched recording.
    await table.getByText('Kofi Bell - Third Rail').click();
    await inspector.getByRole('button', { name: 'Relink…' }).click();
    const relinkDialog = page.getByRole('dialog', { name: 'Relink missing file' });
    await expect(relinkDialog).toBeVisible();

    await relinkDialog.getByRole('textbox', { name: 'Replacement file path' }).fill(unrelatedWav);
    await relinkDialog.getByRole('button', { name: 'Inspect' }).click();
    await expect(relinkDialog.getByRole('alert')).toContainText('does not match the missing track');
    await expect(relinkDialog.getByRole('button', { name: 'Relink selected file' })).toBeDisabled();

    // Searching the relocated folder finds the exact copy and relinks in one click.
    await relinkDialog
      .getByRole('textbox', { name: 'Folder to search for matching copies' })
      .fill(relocatedDir);
    await relinkDialog.getByRole('button', { name: 'Search folder' }).click();
    await expect(relinkDialog.getByText('Exact file copy')).toBeVisible();
    await relinkDialog
      .getByRole('button', { name: /Relink to .*Kofi Bell - Third Rail Moved\.aiff/i })
      .click();
    await expect(relinkDialog).toBeHidden();

    // Track count, collection membership and favourite state remain intact and missing clears.
    await expect(page.getByText('6 tracks', { exact: true })).toBeVisible();
    await expect(crateRow).toContainText('1');
    await expect(inspector.getByText('Missing file')).toHaveCount(0);
    await expect(inspector.getByRole('button', { name: 'Favourite track' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    await expect(inspector.getByRole('button', { name: 'Remove Crate' })).toBeVisible();
    await expect(inspector).toContainText('Kofi Bell - Third Rail Moved.aiff');
    await expect(
      inspector.getByRole('button', { name: 'Play Kofi Bell - Third Rail' }),
    ).toBeEnabled();

    // Clicking the selected row again deselects it, Ctrl+A selects all tracks, and Clear clears all.
    await table.getByText('Kofi Bell - Third Rail').click();
    await expect(inspector).toContainText('Select a track in the table');
    await page.keyboard.press('Control+a');
    const selection = page.getByRole('region', { name: 'Selected tracks' });
    await expect(selection.getByText('6 selected')).toBeVisible();
    await selection.getByRole('button', { name: 'Clear' }).click();
    await expect(selection).toBeHidden();
    await expect(table.locator('[role="row"][aria-selected="true"]')).toHaveCount(0);
  } finally {
    if (!existsSync(originalGarage) && existsSync(relocatedGarage)) {
      copyFileSync(relocatedGarage, originalGarage);
    }
    rmSync(relocatedDir, { recursive: true, force: true });
  }
});

test('Activity drawer tracks scan and identity jobs from TopBar and clears finished history', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1586, height: 992 });
  await page.goto('/');
  await expect(page.getByText('6 tracks', { exact: true })).toBeVisible({ timeout: 10_000 });

  const topBarActivity = page.locator('header').getByRole('button', { name: /^Activity/ });
  await topBarActivity.click();
  const activity = page.getByRole('dialog', { name: 'Activity' });
  await expect(activity).toBeVisible();
  const clearInitial = activity.getByRole('button', { name: 'Clear finished' });
  if ((await clearInitial.count()) > 0) {
    await clearInitial.click();
  }
  await expect(activity.getByText('No recent jobs.')).toBeVisible();
  await activity.getByRole('button', { name: 'Done' }).click();
  await expect(activity).toBeHidden();

  const activityFolder = join(musicFolder, '..', 'crate-batch');
  mkdirSync(activityFolder, { recursive: true });
  try {
    // Write a distinct PCM WAV so byte_hash reconciliation does not merge it into an earlier test's
    // deleted fixture copy, ensuring both scan_source and identify_library run.
    const frames = 88_200;
    const dataBytes = frames * 4;
    const wav = Buffer.alloc(44 + dataBytes);
    wav.write('RIFF', 0, 'ascii');
    wav.writeUInt32LE(36 + dataBytes, 4);
    wav.write('WAVEfmt ', 8, 'ascii');
    wav.writeUInt32LE(16, 16);
    wav.writeUInt16LE(1, 20);
    wav.writeUInt16LE(2, 22);
    wav.writeUInt32LE(44_100, 24);
    wav.writeUInt32LE(44_100 * 4, 28);
    wav.writeUInt16LE(4, 32);
    wav.writeUInt16LE(16, 34);
    wav.write('data', 36, 'ascii');
    wav.writeUInt32LE(dataBytes, 40);
    for (let i = 0; i < frames; i++) {
      const sample = ((i * 211) & 0x3fff) - 0x2000;
      wav.writeInt16LE(sample, 44 + i * 4);
      wav.writeInt16LE(-sample, 46 + i * 4);
    }
    writeFileSync(join(activityFolder, 'Sol Vane - Night Shift.wav'), wav);

    const commandBox = page.getByRole('combobox', { name: 'Commands' });
    await commandBox.focus();
    await page.getByRole('option', { name: 'Scan a music folder' }).click();
    const scanSheet = page.getByRole('dialog', { name: 'Scan a music folder' });
    await scanSheet.getByRole('textbox', { name: 'Music folder' }).fill(activityFolder);
    await scanSheet.getByRole('button', { name: 'Scan folder' }).click();
    await expect(scanSheet.getByRole('status')).toContainText('1 files:');
    await scanSheet.getByRole('button', { name: 'Close' }).click();

    await topBarActivity.click();
    await expect(activity).toBeVisible();
    await expect(activity.getByText('No active jobs.')).toBeVisible({ timeout: 10_000 });

    const recentList = activity.getByRole('list', { name: 'Recent jobs' });
    await expect(recentList.getByRole('listitem', { name: 'Scan crate-batch' })).toContainText(
      'Completed',
    );
    await expect(recentList.getByRole('listitem', { name: 'Identify audio files' })).toContainText(
      'Completed',
    );

    // Fits cleanly at the 760x480 minimum window size without horizontal overflow.
    await page.setViewportSize({ width: 760, height: 480 });
    const hasOverflow = await activity.evaluate((el) => el.scrollWidth > el.clientWidth + 1);
    expect(hasOverflow).toBe(false);

    await activity.getByRole('button', { name: 'Clear finished' }).click();
    await expect(activity.getByText('No recent jobs.')).toBeVisible();
    await activity.getByRole('button', { name: 'Done' }).click();
    await expect(activity).toBeHidden();
  } finally {
    await page.setViewportSize({ width: 1586, height: 992 });
    if (await activity.isVisible()) {
      await activity.getByRole('button', { name: 'Done' }).click();
      await expect(activity).toBeHidden();
    }
    await page.getByRole('button', { name: 'Settings', exact: true }).click();
    const settings = page.getByRole('dialog', { name: 'Settings' });
    const addedCard = settings.getByRole('region', { name: /crate-batch/i });
    if ((await addedCard.count()) > 0) {
      await addedCard.getByRole('button', { name: /Remove/ }).click();
      await expect(addedCard).toHaveCount(0);
    }
    await settings.getByRole('button', { name: 'Done' }).click();
    rmSync(activityFolder, { recursive: true, force: true });
  }
});

test('the Rename sheet previews names from tags, renames the selection on disk and undoes it', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1586, height: 992 });
  await page.goto('/');
  await expect(page.getByText('6 tracks', { exact: true })).toBeVisible({ timeout: 10_000 });

  const table = page.getByRole('grid', { name: 'Tracks' });
  const search = page.getByRole('combobox', { name: 'Commands' });
  const original = join(musicFolder, 'House', 'Ada Marsh - Second Wind.flac');
  const renamed = join(musicFolder, 'House', 'Ada Marsh - Second Wind (Dub).flac');

  // The fixtures carry no tags, so this track gets the user's values the way staging gives them.
  await page.evaluate(async () => {
    const socket = new WebSocket(new URL('/core', location.href));
    await new Promise((ready) => (socket.onopen = ready));
    let next = 0;
    const call = (command: string, params: unknown) =>
      new Promise<{ response: { result: unknown } }>((resolve) => {
        const id = ++next;
        socket.addEventListener('message', (event) => {
          const reply = JSON.parse(String(event.data));
          if (reply.id === id) resolve(reply);
        });
        socket.send(JSON.stringify({ id, request: { command, params } }));
      });
    const page = await call('tracks', { offset: 0, limit: 5, query: { text: 'Second Wind' } });
    const ids = (page.response.result as { tracks: Array<{ id: number }> }).tracks.map(
      (track) => track.id,
    );
    await call('set_user_tag', {
      track_ids: ids,
      edits: [
        { field: 'artist', value: 'Ada Marsh' },
        { field: 'title', value: 'Second Wind' },
        { field: 'mix', value: 'Dub' },
      ],
    });
    socket.close();
  });

  try {
    await search.fill('Second Wind');
    await expect(page.getByText('1 track', { exact: true })).toBeVisible();
    await table.locator('.rows [role="row"]').first().getByRole('checkbox').click();
    await page
      .getByRole('region', { name: 'Selected tracks' })
      .getByRole('button', { name: 'Rename', exact: true })
      .click();

    const sheet = page.getByRole('dialog', { name: 'Rename files' });
    await expect(sheet).toContainText('Selected tracks');
    await expect(sheet).toContainText('1 file');
    const rows = sheet.getByRole('list', { name: 'Preview of new file names' });
    await expect(rows).toContainText('Ada Marsh - Second Wind (Dub).flac');
    expect(existsSync(original)).toBe(true);

    const field = sheet.getByRole('textbox', { name: 'Filename template' });
    await field.fill('{artist} - {title} [{bpm}]');
    await expect(rows).toContainText('No change');
    await field.fill('{artist} - {genre}');
    await expect(sheet.getByRole('alert')).toContainText('{genre} is not a token');
    await field.fill('{artist} - {title} ({mix})');

    await sheet.getByRole('button', { name: 'Rename 1 file' }).click();
    await expect(sheet.getByRole('status')).toContainText('Renamed 1 file.');
    expect(existsSync(renamed)).toBe(true);
    expect(existsSync(original)).toBe(false);

    await sheet.getByRole('button', { name: 'Undo: Renamed 1 file' }).click();
    await expect(sheet.getByRole('status')).toContainText('Undone.');
    expect(existsSync(original)).toBe(true);
    expect(existsSync(renamed)).toBe(false);
    await sheet.getByRole('button', { name: 'Close' }).click();
  } finally {
    if (existsSync(renamed) && !existsSync(original)) renameSync(renamed, original);
    await search.fill('');
    await search.press('Escape');
  }
});
