import { spawnSync } from 'node:child_process';
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
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

// A one-pixel PNG, made up here so no real cover art is involved.
const pixel = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==',
  'base64',
);

test('a picked cover replaces the dot wave in the inspector, served by the core', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1586, height: 992 });
  await page.goto('/');
  const table = page.getByRole('grid', { name: 'Tracks' });
  const inspector = page.getByRole('complementary', { name: 'Inspector' });
  const cover = inspector.locator('.cover .thumb');

  await table.getByText('Ada Marsh - Second Wind').click();
  await expect(inspector.getByRole('heading', { name: 'Ada Marsh - Second Wind' })).toBeVisible();
  await expect(cover).toHaveAttribute('data-artwork', 'wave');
  await expect(cover.locator('canvas')).toBeVisible();

  await inspector.locator('input[type="file"]').setInputFiles({
    name: 'cover.png',
    mimeType: 'image/png',
    buffer: pixel,
  });

  await expect(cover).toHaveAttribute('data-artwork', 'image');
  const image = cover.locator('img');
  await expect(image).toHaveAttribute('src', /^\/artwork\/[0-9a-f]{64}$/);
  await expect
    .poll(() => image.evaluate((element: HTMLImageElement) => element.naturalWidth))
    .toBe(1);
  const served = await page.request.get((await image.getAttribute('src')) ?? '');
  expect(served.status()).toBe(200);
  expect(served.headers()['content-type']).toBe('image/png');
  expect(Buffer.from(await served.body()).equals(pixel)).toBe(true);

  await table.getByText('Nia Okoro - Fourth Wall').click();
  await expect(inspector.getByRole('heading', { name: 'Nia Okoro - Fourth Wall' })).toBeVisible();
  await expect(cover).toHaveAttribute('data-artwork', 'wave');
  await expect(cover.locator('canvas')).toBeVisible();

  await inspector.locator('input[type="file"]').setInputFiles({
    name: 'cover.gif',
    mimeType: 'image/gif',
    buffer: Buffer.from('GIF89a'),
  });
  await expect(inspector.getByRole('alert')).toHaveText('The picture is not a JPEG or PNG');
  await expect(cover).toHaveAttribute('data-artwork', 'wave');

  await table.getByText('Ada Marsh - Second Wind').click();
  await expect(cover).toHaveAttribute('data-artwork', 'image');
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
  await drawer.getByRole('button', { name: 'Add a condition' }).click();
  await drawer.getByRole('combobox', { name: 'Column' }).selectOption('format');
  await drawer
    .getByRole('group', { name: 'Format values' })
    .getByRole('button', { name: 'FLAC' })
    .click();
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
  await drawer.getByRole('button', { name: 'Add a condition' }).click();
  await drawer.getByRole('combobox', { name: 'Column' }).selectOption('key');
  const keys = drawer.getByRole('group', { name: 'Key values' });
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

test('columns are chosen, moved, sized, wrapped and sorted from the keyboard, and kept', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1586, height: 992 });
  await page.goto('/');
  await expect(page.getByText('6 tracks', { exact: true })).toBeVisible({ timeout: 10_000 });

  const table = page.getByRole('grid', { name: 'Tracks' });
  const header = table.locator('.head [role="row"]');
  const firstRow = table.locator('.rows [role="row"]').first();
  const order = () =>
    table
      .locator('.head [data-column]')
      .evaluateAll((cells) => cells.map((cell) => (cell as HTMLElement).dataset.column));
  const headerBox = await header.boundingBox();
  const rowBox = await firstRow.boundingBox();

  const chooser = table.getByRole('button', { name: 'Choose columns' });
  await chooser.click();
  const menu = page.getByRole('menu', { name: 'Columns' });
  await expect(menu.getByRole('menuitemcheckbox', { name: 'Artist' })).toBeFocused();
  await page.keyboard.type('genre');
  await expect(menu.getByRole('menuitemcheckbox', { name: 'Artist' })).toBeHidden();
  await menu.getByRole('menuitemcheckbox', { name: 'Genre' }).click();
  await expect(table.getByRole('columnheader', { name: 'Genre' })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(menu).toBeHidden();
  await expect(chooser).toBeFocused();
  expect(await header.boundingBox()).toEqual(headerBox);
  expect(await firstRow.boundingBox()).toEqual(rowBox);
  await expect.poll(order).toEqual(['title', 'artist', 'bpm', 'key', 'time', 'genre']);

  const genre = table.getByRole('columnheader', { name: 'Genre' });
  await genre.getByRole('button', { name: 'Genre' }).focus();
  await page.keyboard.press('Alt+ArrowLeft');
  await expect.poll(order).toEqual(['title', 'artist', 'bpm', 'key', 'genre', 'time']);
  await expect(genre.getByRole('button', { name: 'Genre' })).toBeFocused();

  const width = table.getByRole('separator', { name: 'Width of Genre' });
  await expect(width).toHaveAttribute('aria-valuenow', '140');
  await width.focus();
  await page.keyboard.press('ArrowRight');
  await expect(width).toHaveAttribute('aria-valuenow', '148');
  await expect.poll(async () => Math.round((await genre.boundingBox())?.width ?? 0)).toBe(148);

  const bpm = table.getByRole('columnheader', { name: 'BPM' });
  await bpm.getByRole('button', { name: 'BPM' }).focus();
  await page.keyboard.press('Enter');
  await expect(bpm).toHaveAttribute('aria-sort', 'ascending');
  const keyButton = table.locator('[data-column="key"] .sort-btn');
  await keyButton.focus();
  await page.keyboard.press('Shift+Enter');
  await expect(keyButton).toHaveAccessibleName(/second sort/);
  await expect(bpm).toHaveAttribute('aria-sort', 'ascending');

  const titleCell = firstRow.getByRole('cell').nth(3);
  await expect
    .poll(() => titleCell.evaluate((cell) => getComputedStyle(cell).whiteSpace))
    .toBe('nowrap');
  await chooser.click();
  await menu.getByRole('menuitemcheckbox', { name: 'Wrap long text' }).click();
  await expect(menu.getByRole('menuitemcheckbox', { name: 'Wrap long text' })).toHaveAttribute(
    'aria-checked',
    'true',
  );
  await page.keyboard.press('Escape');
  await expect
    .poll(() => titleCell.evaluate((cell) => getComputedStyle(cell).whiteSpace))
    .toBe('normal');

  await page.reload();
  await expect(page.getByText('6 tracks', { exact: true })).toBeVisible({ timeout: 10_000 });
  await expect.poll(order).toEqual(['title', 'artist', 'bpm', 'key', 'genre', 'time']);
  await expect(bpm).toHaveAttribute('aria-sort', 'ascending');
  await expect(width).toHaveAttribute('aria-valuenow', '148');
  await expect
    .poll(() => titleCell.evaluate((cell) => getComputedStyle(cell).whiteSpace))
    .toBe('normal');

  await chooser.click();
  await menu.getByRole('menuitem', { name: 'Reset to default' }).click();
  await page.keyboard.press('Escape');
  await expect.poll(order).toEqual(['title', 'artist', 'bpm', 'key', 'time']);
  await expect(bpm).toHaveAttribute('aria-sort', 'none');
  await expect
    .poll(() => titleCell.evaluate((cell) => getComputedStyle(cell).whiteSpace))
    .toBe('nowrap');
});

test('the Waveform column draws identified tracks from their overviews, read as binary', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1586, height: 992 });
  const overviews: string[] = [];
  page.on('response', (response) => {
    if (response.url().includes('/waveforms?tracks='))
      overviews.push(response.headers()['content-type'] ?? '');
  });
  await page.goto('/');
  await expect(page.getByText('6 tracks', { exact: true })).toBeVisible({ timeout: 10_000 });

  const table = page.getByRole('grid', { name: 'Tracks' });
  const chooser = table.getByRole('button', { name: 'Choose columns' });
  await chooser.click();
  const menu = page.getByRole('menu', { name: 'Columns' });
  await page.keyboard.type('wave');
  await menu.getByRole('menuitemcheckbox', { name: 'Waveform' }).click();
  await page.keyboard.press('Escape');
  await expect(table.getByRole('columnheader', { name: 'Waveform' })).toBeVisible();

  // Pixels with any opacity, across every row's waveform. Files are identified in the
  // background, and a row that had no overview asks again as the job moves on.
  const litPixels = () =>
    table.getByRole('img', { name: /^Waveform of / }).evaluateAll((waves) =>
      waves.reduce((lit, wave) => {
        const canvas = wave.querySelector('canvas');
        const pixels = canvas
          ?.getContext('2d')
          ?.getImageData(0, 0, canvas.width, canvas.height).data;
        let count = 0;
        for (let at = 3; at < (pixels?.length ?? 0); at += 4) if (pixels![at]! > 0) count++;
        return lit + count;
      }, 0),
    );
  await expect.poll(litPixels, { timeout: 20_000 }).toBeGreaterThan(0);
  expect(overviews.length).toBeGreaterThan(0);
  expect(overviews.every((type) => type === 'application/octet-stream')).toBe(true);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(1586);

  await chooser.click();
  await menu.getByRole('menuitem', { name: 'Reset to default' }).click();
  await page.keyboard.press('Escape');
  await expect(table.getByRole('columnheader', { name: 'Waveform' })).toBeHidden();
});

test('missing-tag, header and command-bar filters, pinning and recommendation toggles filter without moving the table', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1586, height: 992 });
  await page.goto('/');
  await expect(page.getByText('6 tracks', { exact: true })).toBeVisible({ timeout: 10_000 });

  const table = page.getByRole('grid', { name: 'Tracks' });
  const header = table.locator('.head [role="row"]');
  const firstRow = table.locator('.rows [role="row"]').first();
  const bar = page.getByRole('toolbar', { name: 'Track filters' });
  const search = page.getByRole('combobox', { name: 'Commands' });
  const drawer = page.getByRole('dialog', { name: 'Filters' });
  const settings = page.getByRole('dialog', { name: 'Settings' });

  const headerBox = await header.boundingBox();
  const rowBox = await firstRow.boundingBox();

  const setFirstLightGenre = (value: string | null) =>
    page.evaluate(async (genre) => {
      const socket = new WebSocket(new URL('/core', location.href));
      await new Promise((ready) => (socket.onopen = ready));
      let next = 0;
      const call = (command: string, params: unknown) =>
        new Promise<{ response: { result: unknown } }>((resolve) => {
          const id = ++next;
          socket.addEventListener('message', (event) => {
            if (typeof event.data !== 'string') return;
            const reply = JSON.parse(event.data);
            if (reply.id === id) resolve(reply);
          });
          socket.send(JSON.stringify({ id, request: { command, params } }));
        });
      const result = await call('tracks', { offset: 0, limit: 5, query: { text: 'First Light' } });
      const ids = (result.response.result as { tracks: Array<{ id: number }> }).tracks.map(
        (track) => track.id,
      );
      await call('set_user_tag', {
        track_ids: ids,
        edits: [{ field: 'genre', value: genre }],
      });
      socket.close();
    }, value);

  await setFirstLightGenre('House');
  try {
    // 1. "missing genre" from the filter builder narrows the library and shows a token with its count.
    await page.getByRole('button', { name: 'Filters' }).click();
    await drawer.getByRole('button', { name: 'Add a condition' }).click();
    await drawer.getByRole('combobox', { name: 'Column' }).selectOption('genre');
    await drawer.getByRole('combobox', { name: 'Condition' }).selectOption('missing');
    await drawer.getByRole('button', { name: 'Done' }).click();
    await expect(drawer).toBeHidden();

    await expect(page.getByText('5 tracks', { exact: true })).toBeVisible();
    await expect(table.getByText('Ada Marsh - First Light')).toHaveCount(0);
    const noGenreToken = bar.getByRole('group', { name: 'Filter: No genre' });
    await expect(noGenreToken).toContainText('5');
    expect(await header.boundingBox()).toEqual(headerBox);
    expect(await firstRow.boundingBox()).toEqual(rowBox);

    // 2. Pin the filter from its token, remove the token, and verify the pinned chip stays when recommendations are off.
    await noGenreToken.getByRole('button', { name: /^No genre/ }).click();
    await page.getByRole('menuitem', { name: 'Pin to quick filters' }).click();
    await noGenreToken.getByRole('button', { name: 'Remove filter No genre' }).click();
    await expect(page.getByText('6 tracks', { exact: true })).toBeVisible();

    const pinnedChip = bar.getByRole('button', { name: 'No genre', exact: true });
    await expect(pinnedChip).toBeVisible();
    await expect(bar.getByRole('button', { name: /^No artwork.*recommended$/ })).toBeVisible();

    // 3. Turning recommendations off hides recommended chips while keeping pinned ones.
    await page.getByRole('button', { name: 'Settings' }).click();
    await settings.getByRole('checkbox', { name: 'Recommend quick filters' }).uncheck();
    await settings.getByRole('button', { name: 'Done' }).click();
    await expect(settings).toBeHidden();

    await expect(bar.getByRole('button', { name: /^No artwork.*recommended$/ })).toHaveCount(0);
    await expect(pinnedChip).toBeVisible();

    // Unpin the pinned chip from its context menu and turn recommendations back on.
    await pinnedChip.click({ button: 'right' });
    await page.getByRole('menuitem', { name: 'Remove' }).click();
    await expect(pinnedChip).toHaveCount(0);

    await page.getByRole('button', { name: 'Settings' }).click();
    await settings.getByRole('checkbox', { name: 'Recommend quick filters' }).check();
    await settings.getByRole('button', { name: 'Done' }).click();
    await expect(bar.getByRole('button', { name: /^No genre.*recommended$/ })).toBeVisible();

    // 4. Right-clicking a column header opens the builder on that column.
    await table.getByRole('columnheader', { name: 'Title' }).click({ button: 'right' });
    await page.getByRole('menuitem', { name: 'Filter by Title…' }).click();
    await expect(drawer).toBeVisible();
    await expect(drawer.getByRole('combobox', { name: 'Column' })).toHaveValue('title');
    await drawer.getByRole('textbox', { name: 'Title value' }).fill('First Light');
    await drawer.getByRole('button', { name: 'Done' }).click();

    await expect(page.getByText('1 track', { exact: true })).toBeVisible();
    await expect(table.getByText('Ada Marsh - First Light')).toBeVisible();
    await bar.getByRole('button', { name: /^Remove filter Title contains .First Light.$/ }).click();
    await expect(page.getByText('6 tracks', { exact: true })).toBeVisible();

    // 5. Typing a / filter in the CommandBar parses in the core and applies on Enter.
    await search.fill('/format flac');
    await expect(page.getByRole('option', { name: 'Filter: Format is FLAC' })).toBeVisible();
    await search.press('Enter');
    await expect(page.getByText('1 track', { exact: true })).toBeVisible();
    await expect(table.getByText('Ada Marsh - Second Wind')).toBeVisible();
    expect(await header.boundingBox()).toEqual(headerBox);
    expect(await firstRow.boundingBox()).toEqual(rowBox);
    await bar.getByRole('button', { name: 'Remove filter Format is FLAC' }).click();
    await expect(page.getByText('6 tracks', { exact: true })).toBeVisible();
  } finally {
    await setFirstLightGenre(null);
  }
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

test('auto-preview plays each row arrowed to from 30 % in, and is kept across a reload', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1586, height: 992 });
  await page.goto('/');
  await expect(page.getByText('6 tracks', { exact: true })).toBeVisible({ timeout: 10_000 });

  const table = page.getByRole('grid', { name: 'Tracks' });
  const inspector = page.getByRole('complementary', { name: 'Inspector' });
  const player = page.getByRole('contentinfo', { name: 'Player' });
  const toggle = player.getByRole('button', { name: 'Auto-preview' });
  const heading = inspector.getByRole('heading').first();

  // The test stays silent: the preview plays at zero volume.
  await player.getByRole('slider', { name: 'Volume' }).fill('0');
  await expect(player.getByRole('button', { name: 'Unmute' })).toBeVisible();

  await expect(toggle).toHaveAttribute('aria-pressed', 'false');
  const held = player.locator('.track-title');
  const heldTitle = (await held.count()) > 0 ? await held.textContent() : null;
  await table.focus();
  await page.keyboard.press('Home');
  const firstTitle = (await heading.textContent()) ?? '';
  await page.keyboard.press('ArrowDown');
  const secondTitle = (await heading.textContent()) ?? '';
  expect(secondTitle).not.toBe(firstTitle);
  // Off by default: moving does not change what the player holds.
  if (heldTitle === null) await expect(held).toHaveCount(0);
  else await expect(held).toHaveText(heldTitle);

  await page.keyboard.press('p');
  await expect(toggle).toHaveAttribute('aria-pressed', 'true');
  // Turning it on plays the row the user is on.
  await expect(player).toContainText(secondTitle);
  await expect(table.getByRole('button', { name: `Pause ${secondTitle}` })).toBeVisible();
  await expect(player.locator('.time.elapsed')).not.toHaveText('0:00');

  await page.keyboard.press('ArrowUp');
  await expect(player).toContainText(firstTitle);
  await expect(table.getByRole('button', { name: `Pause ${firstTitle}` })).toBeVisible();
  await expect(table.getByRole('button', { name: `Play ${secondTitle}` })).toBeAttached();

  // A held arrow key ends on the last row reached, with one track playing. An earlier test
  // deletes a file, and a missing row keeps the last track playing instead.
  for (let step = 0; step < 4; step += 1) await page.keyboard.press('ArrowDown');
  const landed = (await heading.textContent()) ?? '';
  const landedMissing = table.getByRole('button', { name: `Missing file for ${landed} — Relink` });
  if ((await landedMissing.count()) > 0) {
    await expect(player).not.toContainText(landed);
    await page.keyboard.press('ArrowUp');
    const playable = (await heading.textContent()) ?? '';
    await expect(player).toContainText(playable);
  } else {
    await expect(player).toContainText(landed);
  }
  await expect(table.getByRole('button', { name: /^Pause / })).toHaveCount(1);

  // Space still pauses and plays.
  await page.keyboard.press('Space');
  await expect(player.getByRole('button', { name: 'Play', exact: true })).toBeVisible();
  await page.keyboard.press('Space');
  await expect(player.getByRole('button', { name: 'Pause', exact: true })).toBeVisible();

  await page.reload();
  await expect(page.getByText('6 tracks', { exact: true })).toBeVisible({ timeout: 10_000 });
  await expect(toggle).toHaveAttribute('aria-pressed', 'true');
  await toggle.click();
  await expect(toggle).toHaveAttribute('aria-pressed', 'false');

  await player.getByRole('button', { name: 'Pause', exact: true }).click();
  await player.getByRole('slider', { name: 'Volume' }).fill('100');
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
  const table = page.getByRole('grid', { name: 'Tracks' });
  const search = page.getByRole('combobox', { name: 'Commands' });
  // The previous test removes a source, so the count settles at its own pace; this test only
  // needs its one track.
  await expect(table).toBeVisible({ timeout: 10_000 });
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
          // The core also pushes binary frames, which these calls never wait for.
          if (typeof event.data !== 'string') return;
          const reply = JSON.parse(event.data);
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
    await expect(page.getByText('1 track', { exact: true })).toBeVisible({ timeout: 10_000 });
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

test('a large rename in Activity pauses, holds its place, resumes and finishes', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1586, height: 992 });
  await page.goto('/');
  await expect(page.getByRole('grid', { name: 'Tracks' })).toBeVisible({ timeout: 10_000 });

  const batchFolder = join(musicFolder, '..', 'pause-batch');
  mkdirSync(batchFolder, { recursive: true });
  const frames = 2_205;
  for (let index = 0; index < 201; index++) {
    const wav = Buffer.alloc(44 + frames * 2);
    wav.write('RIFF', 0, 'ascii');
    wav.writeUInt32LE(36 + frames * 2, 4);
    wav.write('WAVEfmt ', 8, 'ascii');
    wav.writeUInt32LE(16, 16);
    wav.writeUInt16LE(1, 20);
    wav.writeUInt16LE(1, 22);
    wav.writeUInt32LE(44_100, 24);
    wav.writeUInt32LE(44_100 * 2, 28);
    wav.writeUInt16LE(2, 32);
    wav.writeUInt16LE(16, 34);
    wav.write('data', 36, 'ascii');
    wav.writeUInt32LE(frames * 2, 40);
    for (let i = 0; i < frames; i++) {
      wav.writeInt16LE((((i + 1) * (index + 3) * 97) & 0x3fff) - 0x2000, 44 + i * 2);
    }
    writeFileSync(join(batchFolder, `pausebatch ${String(index).padStart(3, '0')}.wav`), wav);
  }

  const activity = page.getByRole('dialog', { name: 'Activity' });
  try {
    // Scan, tag and queue the rename through the core, then pause it at once, the way the
    // Activity button does.
    const jobId = await page.evaluate(async (folder) => {
      const socket = new WebSocket(new URL('/core', location.href));
      await new Promise((ready) => (socket.onopen = ready));
      let next = 0;
      const call = <T>(command: string, params?: unknown) =>
        new Promise<T>((resolve, reject) => {
          const id = ++next;
          socket.addEventListener('message', (event) => {
            // The core also pushes binary frames, which these calls never wait for.
            if (typeof event.data !== 'string') return;
            const reply = JSON.parse(event.data);
            if (reply.id !== id) return;
            if (reply.error) reject(new Error(reply.error.message));
            else resolve(reply.response.result as T);
          });
          socket.send(JSON.stringify({ id, request: params ? { command, params } : { command } }));
        });
      await call('scan', { folder });
      const page = await call<{ tracks: Array<{ id: number; title: string }> }>('tracks', {
        offset: 0,
        limit: 500,
        query: { text: 'pausebatch' },
      });
      for (const track of page.tracks) {
        await call('set_user_tag', {
          track_ids: [track.id],
          edits: [
            { field: 'artist', value: 'Batch' },
            { field: 'title', value: track.title.replace('pausebatch', 'Cut') },
          ],
        });
      }
      const ids = page.tracks.map((track) => track.id);
      await call('apply_rename', {
        scope: { kind: 'tracks', track_ids: ids },
        template: '{artist} - {title}',
      });
      const snapshot = await call<{ active: Array<{ id: number; kind: string }> }>('jobs_snapshot');
      const job = snapshot.active.find((active) => active.kind === 'rename_files');
      if (!job) throw new Error('the rename finished before it could be paused');
      await call('pause_job', { job_id: job.id });
      socket.close();
      return job.id;
    }, batchFolder);

    await page
      .locator('header')
      .getByRole('button', { name: /^Activity/ })
      .click();
    const card = activity
      .getByRole('list', { name: 'Active jobs' })
      .getByRole('listitem', { name: 'Rename 201 files' });
    await expect(card).toContainText('Paused');
    await page.waitForTimeout(1_000);
    await expect(card).toContainText('Paused');
    expect(jobId).toBeGreaterThan(0);

    await card.getByRole('button', { name: 'Resume Rename 201 files' }).click();
    await expect(
      activity
        .getByRole('list', { name: 'Recent jobs' })
        .getByRole('listitem', { name: 'Rename 201 files' }),
    ).toContainText('Completed', { timeout: 20_000 });
    expect(existsSync(join(batchFolder, 'Batch - Cut 000.wav'))).toBe(true);
    expect(existsSync(join(batchFolder, 'pausebatch 000.wav'))).toBe(false);
  } finally {
    if (await activity.isVisible()) {
      await activity.getByRole('button', { name: 'Done' }).click();
    }
    await page.getByRole('button', { name: 'Settings', exact: true }).click();
    const settings = page.getByRole('dialog', { name: 'Settings' });
    await expect(settings.getByRole('button', { name: /Remove/ }).first()).toBeVisible();
    const batchCard = settings.getByRole('region', { name: /pause-batch/i });
    if ((await batchCard.count()) > 0) {
      await batchCard.getByRole('button', { name: /Remove/ }).click();
      await expect(batchCard).toHaveCount(0);
    }
    await settings.getByRole('button', { name: 'Done' }).click();
    rmSync(batchFolder, { recursive: true, force: true });
  }
});

test('a batch of 50 mixed files goes from untagged to tagged in Prep using the keyboard only', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1586, height: 992 });
  await page.goto('/');
  await expect(page.getByRole('combobox', { name: 'Commands' })).toBeVisible({ timeout: 10_000 });

  const prepFolder = join(musicFolder, '..', 'prep-batch-50');
  const releaseFolder = join(prepFolder, '[VEC001] Vector Recordings - Horizon EP (2024)');
  mkdirSync(releaseFolder, { recursive: true });

  const frames = 2_205;
  function makeWav(seed: number): Buffer {
    const wav = Buffer.alloc(44 + frames * 2);
    wav.write('RIFF', 0, 'ascii');
    wav.writeUInt32LE(36 + frames * 2, 4);
    wav.write('WAVEfmt ', 8, 'ascii');
    wav.writeUInt32LE(16, 16);
    wav.writeUInt16LE(1, 20);
    wav.writeUInt16LE(1, 22);
    wav.writeUInt32LE(44_100, 24);
    wav.writeUInt32LE(44_100 * 2, 28);
    wav.writeUInt16LE(2, 32);
    wav.writeUInt16LE(16, 34);
    wav.write('data', 36, 'ascii');
    wav.writeUInt32LE(frames * 2, 40);
    for (let i = 0; i < frames; i++) {
      wav.writeInt16LE((((i + 1) * (seed + 5) * 131) & 0x3fff) - 0x2000, 44 + i * 2);
    }
    return wav;
  }

  function makeAiff(seed: number): Buffer {
    const dataSize = frames * 2;
    const soundSize = dataSize + 8;
    const formSize = soundSize + 38;
    const aiff = Buffer.alloc(54 + dataSize);
    aiff.write('FORM', 0, 'ascii');
    aiff.writeUInt32BE(formSize, 4);
    aiff.write('AIFFCOMM', 8, 'ascii');
    aiff.writeUInt32BE(18, 16);
    aiff.writeUInt16BE(1, 20);
    aiff.writeUInt32BE(frames, 22);
    aiff.writeUInt16BE(16, 26);
    Buffer.from([0x40, 0x0e, 0xac, 0x44, 0, 0, 0, 0, 0, 0]).copy(aiff, 28);
    aiff.write('SSND', 38, 'ascii');
    aiff.writeUInt32BE(soundSize, 42);
    aiff.writeUInt32BE(0, 46);
    aiff.writeUInt32BE(0, 50);
    for (let i = 0; i < frames; i++) {
      aiff.writeInt16BE((((i + 1) * (seed + 17) * 157) & 0x3fff) - 0x2000, 54 + i * 2);
    }
    return aiff;
  }

  for (let index = 1; index <= 40; index++) {
    const num = String(index).padStart(2, '0');
    writeFileSync(
      join(releaseFolder, `${num} - Kaden - Horizon Cut ${num} (Extended Mix).wav`),
      makeWav(index),
    );
  }
  for (let index = 41; index <= 49; index++) {
    const num = String(index).padStart(2, '0');
    writeFileSync(
      join(releaseFolder, `${num} - Kaden - Horizon Cut ${num} (VIP Mix).aiff`),
      makeAiff(index),
    );
  }
  writeFileSync(join(releaseFolder, '50 - Kaden - Horizon Finale.wav'), makeWav(50));
  const files = readdirSync(releaseFolder).map((name) => join(releaseFolder, name));
  const id3ArtistFrame = Buffer.from([0x54, 0x50, 0x45, 0x31]);
  try {
    await page.evaluate(async (folder) => {
      const socket = new WebSocket(new URL('/core', location.href));
      await new Promise((resolve) => (socket.onopen = resolve));
      const scanned = new Promise<void>((resolve, reject) => {
        socket.onmessage = (event) => {
          if (typeof event.data !== 'string') return;
          const reply = JSON.parse(event.data);
          if (reply.id !== 1) return;
          if (reply.error) reject(new Error(reply.error.message));
          else resolve();
        };
      });
      socket.send(JSON.stringify({ id: 1, request: { command: 'scan', params: { folder } } }));
      try {
        await scanned;
      } finally {
        socket.close();
      }
    }, prepFolder);
    await page.reload();
    const library = page.getByRole('grid', { name: 'Tracks' });
    const search = page.getByRole('combobox', { name: 'Commands' });
    await search.fill('Horizon');
    await expect(page.getByText('50 tracks', { exact: true }).first()).toBeVisible();
    await library.getByRole('checkbox', { name: 'Select all tracks' }).press('Space');
    await page.getByRole('button', { name: 'Open in Prep' }).press('Enter');

    const prep = page.getByRole('region', { name: 'Prep', exact: true });
    const filter = prep.getByRole('combobox', { name: 'Filter tracks' });
    await filter.focus();
    await page.keyboard.press('Home');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    await expect(filter).toHaveValue('artist');
    // The first load of a view reads every track's filename first, which can take a few seconds.
    await expect(prep.locator('.column .count')).toHaveText('50', { timeout: 30_000 });
    await expect(prep.locator('.row:not(.placeholder)').first()).toBeVisible({ timeout: 30_000 });
    const selectAll = prep.getByRole('checkbox', { name: 'Select all tracks' });
    await selectAll.press('Space');
    await expect(prep.getByText('50 ticked')).toBeVisible();
    for (const [width, height] of [
      [1100, 700],
      [760, 480],
    ] as const) {
      await page.setViewportSize({ width, height });
      const firstTrack = prep.locator('.track-name').first();
      await expect(firstTrack).toBeInViewport();
      await expect(page.getByRole('combobox', { name: 'Commands' })).toBeHidden();
      if (width === 760) {
        await expect(prep.locator('.track-name').nth(2)).toBeInViewport();
        await page.screenshot({ path: test.info().outputPath('prep-list-760.png') });
        await firstTrack.press('Enter');
        await expect(prep.getByRole('button', { name: 'Back to tracks' })).toBeFocused();
      }
      const acceptArtist = prep.getByRole('button', { name: 'Accept Artist' });
      if (width === 760) {
        // A short window scrolls the review; Tab reaches the suggestion and brings it into view.
        for (
          let step = 0;
          step < 20 && !(await acceptArtist.evaluate((el) => el === document.activeElement));
          step++
        ) {
          await page.keyboard.press('Tab');
        }
        await expect(acceptArtist).toBeFocused();
      }
      await expect(acceptArtist).toBeInViewport();
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
        true,
      );
      await page.screenshot({ path: test.info().outputPath('prep-' + width + '.png') });
      if (width === 760) {
        await page.keyboard.press('Escape');
        await expect(firstTrack).toBeFocused();
      }
    }
    await page.setViewportSize({ width: 1586, height: 992 });
    await page.keyboard.press('Shift+A');
    await expect(prep.getByText('Saved to 50 files')).toBeVisible({ timeout: 30_000 });
    // Saved tracks keep their places in the list, now showing the artist they were missing.
    await expect(prep.locator('.row .artist').first()).toContainText('Kaden');
    expect(files.every((file) => readFileSync(file).includes(id3ArtistFrame))).toBe(true);
    await prep.getByRole('button', { name: 'Undo save' }).press('Enter');
    await expect(prep.getByText('Restored 50 files')).toBeVisible({ timeout: 30_000 });
    expect(files.every((file) => !readFileSync(file).includes(id3ArtistFrame))).toBe(true);
    // The tracks stayed in the list through the save and the undo, so they are still ticked.
    await expect(prep.getByText('50 ticked')).toBeVisible();
    // The list reloads after an undo, and the button stays disabled until it is back. A key
    // pressed on a disabled button goes to the page, where Enter searches the current track.
    const saveTicked = prep.getByRole('button', { name: 'Save ticked' });
    await expect(saveTicked).toBeEnabled();
    await saveTicked.press('Enter');
    await expect(prep.getByText('Saved to 50 files')).toBeVisible({ timeout: 30_000 });

    // Verify all 50 mixed files on disk now hold their tags and have zero unwritten changes left.
    const verified = await page.evaluate(async () => {
      const socket = new WebSocket(new URL('/core', location.href));
      await new Promise((ready) => (socket.onopen = ready));
      let next = 0;
      const call = <T>(command: string, params?: unknown) =>
        new Promise<T>((resolve, reject) => {
          const id = ++next;
          socket.addEventListener('message', (event) => {
            if (typeof event.data !== 'string') return;
            const reply = JSON.parse(event.data);
            if (reply.id !== id) return;
            if (reply.error) reject(new Error(reply.error.message));
            else resolve(reply.response.result as T);
          });
          socket.send(JSON.stringify({ id, request: params ? { command, params } : { command } }));
        });
      const result = await call<{
        tracks: Array<{ id: number; title: string; artist: string | null }>;
      }>('tracks', {
        offset: 0,
        limit: 100,
        query: { text: 'Horizon' },
      });
      const pendingWrites = await call<Array<unknown>>('preview_tag_writes', {
        track_ids: result.tracks.map((track) => track.id),
      });
      socket.close();
      return {
        count: result.tracks.length,
        allHaveKaden: result.tracks.every((track) => track.artist === 'Kaden'),
        pendingWrites: pendingWrites.length,
      };
    });
    expect(verified).toEqual({
      count: 50,
      allHaveKaden: true,
      pendingWrites: 0,
    });

    // Return to Library with 1.
    await page.keyboard.press('1');
    await expect(page.getByRole('grid', { name: 'Tracks' })).toBeVisible();
  } finally {
    await page.evaluate(async () => {
      const socket = new WebSocket(new URL('/core', location.href));
      await new Promise((ready) => (socket.onopen = ready));
      let next = 0;
      const call = <T>(command: string, params?: unknown) =>
        new Promise<T>((resolve, reject) => {
          const id = ++next;
          socket.addEventListener('message', (event) => {
            if (typeof event.data !== 'string') return;
            const reply = JSON.parse(event.data);
            if (reply.id !== id) return;
            if (reply.error) reject(new Error(reply.error.message));
            else resolve(reply.response.result as T);
          });
          socket.send(JSON.stringify({ id, request: params ? { command, params } : { command } }));
        });
      const sources = await call<{ sources: Array<{ id: number; folder: string }> }>('sources');
      const found = sources.sources.find((s) => s.folder.includes('prep-batch-50'));
      if (found) await call('remove_source', { source_id: found.id });
      socket.close();
    });
    rmSync(prepFolder, { recursive: true, force: true });
  }
});

test('an M3U8 playlist mirrors as a collection, follows a replace through a backed-up update, undoes it and exports', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1586, height: 992 });
  const sandbox = join(musicFolder, '..', 'playlist-sandbox');
  const music = join(sandbox, 'music');
  const sets = join(sandbox, 'sets');
  rmSync(sandbox, { recursive: true, force: true });
  mkdirSync(music, { recursive: true });
  mkdirSync(sets, { recursive: true });

  // A 20 s groove of pitched kicks over quiet noise, so the replace can line the files up.
  const rate = 44_100;
  const frames = rate * 20;
  const wav = Buffer.alloc(44 + frames * 2);
  wav.write('RIFF', 0, 'ascii');
  wav.writeUInt32LE(36 + frames * 2, 4);
  wav.write('WAVEfmt ', 8, 'ascii');
  wav.writeUInt32LE(16, 16);
  wav.writeUInt16LE(1, 20);
  wav.writeUInt16LE(1, 22);
  wav.writeUInt32LE(rate, 24);
  wav.writeUInt32LE(rate * 2, 28);
  wav.writeUInt16LE(2, 32);
  wav.writeUInt16LE(16, 34);
  wav.write('data', 36, 'ascii');
  wav.writeUInt32LE(frames * 2, 40);
  let seed = 7;
  for (let i = 0; i < frames; i++) {
    seed = (seed * 1_103_515_245 + 12_345) % 2_147_483_648;
    const beat = Math.floor(i / (rate / 2));
    const t = (i % (rate / 2)) / rate;
    const kick = Math.sin(2 * Math.PI * (55 + (beat % 7) * 20) * t) * Math.exp(-t * 18);
    const sample = 0.7 * kick + 0.05 * (seed / 2_147_483_648 - 0.5);
    wav.writeInt16LE(Math.round(sample * 32_000), 44 + i * 2);
  }
  const master = join(sandbox, 'master.wav');
  writeFileSync(master, wav);
  const ffmpeg = (...args: string[]) => {
    const run = spawnSync('ffmpeg', ['-y', '-hide_banner', '-loglevel', 'error', ...args]);
    expect(run.status, 'FFmpeg is installed for the replace checks').toBe(0);
  };
  ffmpeg('-i', master, '-c:a', 'libmp3lame', '-b:a', '128k', join(music, 'groove.mp3'));
  const flac = join(sandbox, 'groove.flac');
  ffmpeg('-i', master, '-c:a', 'flac', flac);
  rmSync(master);

  const playlist = join(sets, 'Friday.m3u8');
  const original =
    '#EXTM3U\r\n#EXTINF:20,Groove\r\n..\\music\\groove.mp3\r\n../music/not here.mp3\r\n';
  writeFileSync(playlist, original);

  await page.goto('/');
  await expect(page.getByRole('region', { name: 'Library' })).toBeVisible({ timeout: 10_000 });
  const call = (command: string, params?: unknown) =>
    page.evaluate(
      async ({ command, params }) => {
        const socket = new WebSocket(new URL('/core', location.href));
        await new Promise((ready) => (socket.onopen = ready));
        const result = await new Promise((resolve, reject) => {
          socket.addEventListener('message', (event) => {
            if (typeof event.data !== 'string') return;
            const reply = JSON.parse(event.data);
            if (reply.id !== 1) return;
            if (reply.error) reject(new Error(reply.error.message));
            else resolve(reply.response.result);
          });
          socket.send(
            JSON.stringify({ id: 1, request: params ? { command, params } : { command } }),
          );
        });
        socket.close();
        return result;
      },
      { command, params },
    );

  const sidebar = page.getByRole('navigation', { name: 'Spaces' });
  try {
    await call('scan', { folder: music });

    await sidebar.getByRole('button', { name: 'Import playlist' }).click();
    const importSheet = page.getByRole('dialog', { name: 'Import playlist' });
    await importSheet.getByRole('textbox', { name: 'Playlist file' }).fill(playlist);
    await importSheet.getByRole('button', { name: 'Import', exact: true }).click();
    await expect(importSheet).toBeHidden();
    const fridayRow = sidebar.getByRole('button', { name: /^Friday/ });
    await expect(fridayRow).toContainText('1');
    await expect(page.getByRole('heading', { name: 'Friday', level: 1 })).toBeVisible();
    await expect(page.getByRole('grid', { name: 'Tracks' }).getByText('groove')).toBeVisible();

    const collections = (await call('collections')) as Array<{
      id: number;
      name: string;
      mirror: boolean;
    }>;
    const friday = collections.find((collection) => collection.name === 'Friday');
    expect(friday?.mirror).toBe(true);
    const tracks = (await call('tracks', {
      offset: 0,
      limit: 10,
      query: { collection_id: friday?.id },
    })) as { tracks: Array<{ id: number }> };
    await call('apply_replacement', {
      track_id: tracks.tracks[0]?.id,
      path: flac,
      choice: 'replace',
    });
    expect(existsSync(join(music, 'groove.flac'))).toBe(true);

    await page.reload();
    const update = sidebar.getByRole('button', { name: /^Update playlists/ });
    await expect(update).toContainText('1');
    await update.click();
    const changesSheet = page.getByRole('dialog', { name: 'Update playlists' });
    const edits = changesSheet.getByRole('list', { name: 'Changes to Friday' });
    await expect(edits).toContainText('..\\music\\groove.mp3');
    await expect(edits).toContainText('..\\music\\groove.flac');
    await expect(changesSheet).toContainText('1 file moved');
    expect(readFileSync(playlist, 'utf8')).toBe(original);

    await changesSheet.getByRole('button', { name: 'Update Friday' }).click();
    await expect(changesSheet.getByRole('status')).toContainText('Updated Friday');
    const updated = readFileSync(playlist, 'utf8');
    expect(updated).toContain('\r\n..\\music\\groove.flac\r\n../music/not here.mp3\r\n');
    const backups = join(musicFolder, '..', 'data', 'backups', 'playlists');
    const saved = readdirSync(backups).filter((name) => name.endsWith('-Friday.m3u8'));
    expect(saved).toHaveLength(1);
    expect(readFileSync(join(backups, saved[0] ?? ''), 'utf8')).toBe(original);
    await changesSheet.getByRole('button', { name: 'Close' }).click();
    await expect(update).toBeHidden();

    await page
      .locator('header')
      .getByRole('button', { name: /^Activity/ })
      .click();
    const activity = page.getByRole('dialog', { name: 'Activity' });
    const history = activity.getByRole('list', { name: 'Changes to files' });
    await history.getByRole('button', { name: 'Undo Updated 1 playlist' }).click();
    await expect(
      history.getByRole('listitem').filter({ hasText: 'Updated 1 playlist' }).first(),
    ).toContainText('Undone');
    expect(readFileSync(playlist, 'utf8')).toBe(original);
    await activity.getByRole('button', { name: 'Done' }).click();
    await expect(update).toContainText('1');
    await update.click();
    await changesSheet.getByRole('button', { name: 'Update Friday' }).click();
    await expect(changesSheet.getByRole('status')).toContainText('Updated Friday');
    expect(readFileSync(playlist, 'utf8')).toBe(updated);
    await changesSheet.getByRole('button', { name: 'Close' }).click();
    await expect(update).toBeHidden();

    await sidebar.getByRole('button', { name: 'Actions for Friday' }).click();
    await page.getByRole('menuitem', { name: 'Export to M3U8' }).click();
    const exportSheet = page.getByRole('dialog', { name: 'Export to M3U8' });
    const exported = join(sets, 'Export.m3u8');
    await exportSheet.getByRole('textbox', { name: 'Save as' }).fill(exported);
    await exportSheet.getByRole('button', { name: 'Save' }).click();
    await expect(exportSheet.getByRole('status')).toContainText('Saved 1 track');
    const written = readFileSync(exported, 'utf8');
    expect(written.startsWith('#EXTM3U\n#EXTINF:20,')).toBe(true);
    expect(written).toContain('groove.flac\n');
    await exportSheet.getByRole('button', { name: 'Close' }).click();

    await sidebar.getByRole('button', { name: 'Actions for Friday' }).click();
    await page.getByRole('menuitem', { name: 'Stop following' }).click();
    await page
      .getByRole('dialog', { name: 'Stop following playlist' })
      .getByRole('button', { name: 'Stop following' })
      .click();
    await expect(fridayRow).toBeHidden();
    expect(existsSync(playlist)).toBe(true);
  } finally {
    const sources = (await call('sources')) as { sources: Array<{ id: number; folder: string }> };
    const found = sources.sources.find((source) => source.folder.includes('playlist-sandbox'));
    if (found) await call('remove_source', { source_id: found.id });
    rmSync(sandbox, { recursive: true, force: true });
  }
});
