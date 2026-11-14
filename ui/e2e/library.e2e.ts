import { expect, test } from '@playwright/test';

import { musicFolder } from './core';

// The tests run serially against the same core instance.
test.describe.configure({ mode: 'serial' });

test('a scanned folder fills the table, and a selected track fills the inspector', async ({
  page,
}) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'No tracks yet' })).toBeVisible();

  await page.getByRole('button', { name: 'Add a music folder' }).click();
  const sheet = page.getByRole('dialog', { name: 'Scan a music folder' });
  await sheet.getByRole('textbox', { name: 'Music folder' }).fill(musicFolder);
  await sheet.getByRole('button', { name: 'Scan folder' }).click();
  await expect(sheet.getByRole('status')).toContainText('5 files: 5 added, 0 changed, 0 unchanged');
  await sheet.getByRole('button', { name: 'Close' }).click();

  const table = page.getByRole('grid', { name: 'Tracks' });
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
  await expect(inspector).toContainText('No artist tag');
  await expect(inspector).toContainText('Garage');
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

test('shell navigation, spaces, sidebar collapse and inspector toggling', async ({ page }) => {
  await page.goto('/');
  await expect(
    page.getByRole('grid', { name: 'Tracks' }).getByText('Ada Marsh - First Light'),
  ).toBeVisible();

  const nav = page.getByRole('navigation', { name: 'Spaces' });

  // Test space switching: Mix space
  await nav.getByRole('button', { name: 'Mix', exact: true }).click();
  await expect(nav.getByRole('button', { name: 'Mix', exact: true })).toHaveAttribute(
    'aria-current',
    'page',
  );

  // Test space switching: back to Library
  await nav.getByRole('button', { name: 'Library', exact: true }).click();
  await expect(nav.getByRole('button', { name: 'Library', exact: true })).toHaveAttribute(
    'aria-current',
    'page',
  );
  await expect(page.getByRole('grid', { name: 'Tracks' })).toBeVisible();

  // Test inspector toggle with keyboard shortcut 'i'
  const inspector = page.getByRole('complementary', { name: 'Inspector' });
  await expect(inspector).toBeVisible();
  await page.keyboard.press('i');
  await expect(inspector).not.toBeVisible();
  await page.keyboard.press('i');
  await expect(inspector).toBeVisible();

  // Test sidebar collapse toggle with button and shortcut '['
  const collapseBtn = page.getByRole('button', { name: 'Collapse the sidebar' });
  if (await collapseBtn.isVisible()) {
    await collapseBtn.click();
    await expect(page.getByRole('button', { name: 'Expand the sidebar' })).toBeVisible();
    await page.keyboard.press('[');
    await expect(page.getByRole('button', { name: 'Collapse the sidebar' })).toBeVisible();
  }
});

test('theme switching in Settings sheet updates data-theme and reflects in DOM', async ({
  page,
}) => {
  await page.goto('/');

  // Open settings from TopBar
  await page.getByRole('button', { name: 'Settings' }).click();
  const settings = page.getByRole('dialog', { name: 'Settings' });
  await expect(settings).toBeVisible();

  // Switch to Light theme
  await settings.getByRole('tab', { name: 'Light' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');

  // Switch to Dark theme
  await settings.getByRole('tab', { name: 'Dark' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');

  // Switch to System theme
  await settings.getByRole('tab', { name: 'System' }).click();
  await expect(page.locator('html')).not.toHaveAttribute('data-theme');

  // Verify milestone placeholders in Settings
  await expect(settings.getByText('Library sources arrive in M2.')).toBeVisible();
  await expect(settings.getByText('AI provider keys arrive in M3.')).toBeVisible();
  await expect(
    settings.getByText('Audio and MIDI device configuration arrives in M7.'),
  ).toBeVisible();

  // Close Settings drawer
  await settings.getByRole('button', { name: 'Done' }).click();
  await expect(settings).not.toBeVisible();
});
