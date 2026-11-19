import { expect, type Locator, type Page, test } from '@playwright/test';

test.describe.configure({ mode: 'serial' });

/** Mean brightness, 0 to 1, of the lower quarter of an element as it is painted on screen. */
async function brightness(page: Page, element: Locator): Promise<number> {
  const png = await element.screenshot();
  return page.evaluate(async (data) => {
    const image = new Image();
    image.src = `data:image/png;base64,${data}`;
    await image.decode();
    const canvas = new OffscreenCanvas(image.width, image.height);
    const context = canvas.getContext('2d');
    if (!context) throw new Error('no 2D context');
    context.drawImage(image, 0, 0);
    const top = Math.floor(image.height * 0.75);
    const pixels = context.getImageData(0, top, image.width, image.height - top).data;
    let sum = 0;
    for (const [index, value] of pixels.entries()) if (index % 4 !== 3) sum += value;
    return sum / (pixels.length / 4) / 765;
  }, png.toString('base64'));
}

async function open(page: Page, width: number, height: number) {
  await page.setViewportSize({ width, height });
  await page.goto('/');
  await expect(page.getByRole('navigation', { name: 'Spaces' })).toBeVisible();
}

async function width(element: Locator): Promise<number> {
  const box = await element.boundingBox();
  if (!box) throw new Error('the element is not laid out');
  return Math.round(box.width);
}

const layouts = [
  { width: 1100, height: 700, sidebar: 52, inspector: null, dock: 'Expand the dock' },
  { width: 1280, height: 720, sidebar: 52, inspector: 340, dock: 'Expand the dock' },
  { width: 1400, height: 900, sidebar: 256, inspector: 371, dock: 'Collapse the dock' },
  { width: 1586, height: 992, sidebar: 256, inspector: 420, dock: 'Collapse the dock' },
  { width: 2560, height: 1440, sidebar: 256, inspector: 440, dock: 'Collapse the dock' },
  { width: 5120, height: 2160, sidebar: 256, inspector: 440, dock: 'Collapse the dock' },
];

for (const layout of layouts) {
  test(`at ${layout.width} by ${layout.height} the shell keeps to the layout rules in both themes`, async ({
    page,
  }) => {
    await open(page, layout.width, layout.height);

    for (const theme of ['dark', 'light'] as const) {
      await page.evaluate((next) => {
        document.documentElement.dataset.theme = next;
      }, theme);

      const sidebar = page.getByRole('navigation', { name: 'Spaces' });
      const inspector = page.getByRole('complementary', { name: 'Inspector' });
      expect(await width(sidebar)).toBe(layout.sidebar);
      await expect(page.getByRole('button', { name: layout.dock })).toBeVisible();
      await expect(page.getByRole('dialog')).toHaveCount(0);
      if (layout.inspector === null) await expect(inspector).toBeHidden();
      else expect(await width(inspector)).toBe(layout.inspector);

      const search = await page.getByRole('combobox', { name: 'Commands' }).boundingBox();
      const content = await page.getByRole('main').boundingBox();
      expect(search && content && search.x + search.width <= content.x + content.width).toBe(true);

      const sideways = await page.evaluate(() =>
        [document.documentElement, ...document.querySelectorAll('nav *')].filter(
          (element) => element.scrollWidth > element.clientWidth + 1,
        ),
      );
      expect(sideways).toHaveLength(0);
    }
  });
}

test('sidebar tooltips show whole, beside the sidebar and in the collapsed rail', async ({
  page,
}) => {
  await open(page, 1586, 992);
  await page.getByRole('button', { name: 'Sets' }).hover();
  const sets = page.getByRole('tooltip', { name: 'Sets arrives in M10' });
  await expect(sets).toBeVisible();
  await expect(sets).toBeInViewport({ ratio: 1 });
  expect(await sets.evaluate((tip) => tip.matches(':popover-open'))).toBe(true);

  await open(page, 1280, 720);
  const rail = page.getByRole('button', { name: 'Mix', exact: true });
  await rail.hover();
  const mix = page.getByRole('tooltip', { name: 'Mix 4' });
  await expect(mix).toBeInViewport({ ratio: 1 });
  const [tip, button] = await Promise.all([mix.boundingBox(), rail.boundingBox()]);
  expect(tip && button && tip.x >= button.x + button.width).toBe(true);
});

test('the sidebar, dock and inspector follow their buttons and shortcuts', async ({ page }) => {
  await open(page, 1586, 992);
  const sidebar = page.getByRole('navigation', { name: 'Spaces' });

  await page.getByRole('button', { name: 'Collapse the sidebar' }).click();
  await expect(page.getByRole('button', { name: 'Expand the sidebar' })).toBeVisible();
  expect(await width(sidebar)).toBe(52);
  await page.keyboard.press('[');
  await expect(page.getByRole('button', { name: 'Collapse the sidebar' })).toBeVisible();
  expect(await width(sidebar)).toBe(256);

  await page.keyboard.press('d');
  await expect(page.getByRole('button', { name: 'Expand the dock' })).toBeVisible();
  await page.getByRole('button', { name: 'Expand the dock' }).click();
  await expect(page.getByRole('button', { name: 'Collapse the dock' })).toBeVisible();

  const inspector = page.getByRole('complementary', { name: 'Inspector' });
  await page.keyboard.press('i');
  await expect(inspector).toBeHidden();
  await page.keyboard.press('i');
  await expect(inspector).toBeVisible();
  await inspector.getByRole('button', { name: 'Hide the inspector' }).click();
  await expect(inspector).toBeHidden();
});

test('below 1280 wide the inspector is a sheet that opens only when asked', async ({ page }) => {
  await open(page, 1100, 700);
  await expect(page.getByRole('dialog')).toHaveCount(0);

  await page.keyboard.press('i');
  const sheet = page.getByRole('dialog', { name: 'Inspector' });
  await expect(sheet).toBeVisible();
  await expect(sheet.getByRole('complementary', { name: 'Inspector' })).toHaveCount(1);
  await expect(sheet.getByRole('button', { name: 'Hide the inspector' })).toHaveCount(1);
  await page.keyboard.press('Escape');
  await expect(sheet).toBeHidden();

  await page.setViewportSize({ width: 1586, height: 992 });
  await expect(page.getByRole('complementary', { name: 'Inspector' })).toBeVisible();
});

test('shortcuts do nothing behind the Settings sheet', async ({ page }) => {
  await open(page, 1586, 992);
  await page.getByRole('button', { name: 'Settings' }).click();
  const settings = page.getByRole('dialog', { name: 'Settings' });
  await expect(settings).toBeVisible();
  for (const key of ['i', 'd', '[', '4']) await page.keyboard.press(key);
  await settings.getByRole('button', { name: 'Done' }).click();

  await expect(page.getByRole('complementary', { name: 'Inspector' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Collapse the dock' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Collapse the sidebar' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Library', exact: true })).toHaveAttribute(
    'aria-current',
    'page',
  );
});

test('Mix leaves out the dock and player until the decks arrive in M7', async ({ page }) => {
  await open(page, 1586, 992);
  await page.getByRole('button', { name: 'Mix', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Mix', exact: true })).toHaveAttribute(
    'aria-current',
    'page',
  );
  await expect(page.getByRole('region', { name: 'Transition' })).toHaveCount(0);
  await expect(page.getByRole('contentinfo', { name: 'Player' })).toHaveCount(0);
});

test('a theme chosen in Settings repaints the canvas art', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'dark', reducedMotion: 'reduce' });
  await open(page, 1586, 992);
  const wave = page.locator('nav canvas');
  const settings = page.getByRole('dialog', { name: 'Settings' });

  async function choose(theme: string) {
    await page.getByRole('button', { name: 'Settings' }).click();
    await settings.getByRole('tab', { name: theme }).click();
    await settings.getByRole('button', { name: 'Done' }).click();
    await expect(settings).toBeHidden();
  }

  await choose('Light');
  await expect.poll(() => brightness(page, wave)).toBeGreaterThan(0.6);
  await choose('Dark');
  await expect.poll(() => brightness(page, wave)).toBeLessThan(0.4);
  await choose('System');
  await expect(page.locator('html')).not.toHaveAttribute('data-theme');
});

test('with forced colours the disabled spaces and scrub tracks stay distinct', async ({ page }) => {
  await page.emulateMedia({ forcedColors: 'active' });
  await open(page, 1586, 992);

  const colour = (name: string) =>
    page
      .getByRole('button', { name, exact: true })
      .evaluate((button) => getComputedStyle(button).color);
  expect(await colour('Sets')).not.toBe(await colour('Mix'));

  const track = await page
    .getByRole('slider', { name: 'Scrub' })
    .evaluate(
      (input) =>
        getComputedStyle(input.previousElementSibling?.previousElementSibling as Element)
          .borderTopStyle,
    );
  expect(track).toBe('solid');
});

test('a provider key saved in Settings shows as configured, then clears', async ({ page }) => {
  await open(page, 1586, 992);
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const settings = page.getByRole('dialog', { name: 'Settings' });
  const acoustid = settings.getByRole('group', { name: 'AcoustID application key' });
  await expect(acoustid.getByText('Not set')).toBeVisible();

  const field = acoustid.getByLabel('AcoustID application key');
  await expect(field).toHaveAttribute('type', 'password');
  await field.fill('e2e-test-key');
  await field.press('Enter');
  await expect(acoustid.getByText('Configured')).toBeVisible();
  await expect(field).toHaveValue('');

  await settings.getByRole('button', { name: 'Done' }).click();
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await expect(acoustid.getByText('Configured')).toBeVisible();

  await acoustid.getByRole('button', { name: 'Clear AcoustID key' }).click();
  await expect(acoustid.getByText('Not set')).toBeVisible();
  await settings.getByRole('button', { name: 'Done' }).click();
});
