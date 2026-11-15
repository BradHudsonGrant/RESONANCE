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
