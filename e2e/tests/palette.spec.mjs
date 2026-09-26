import { test, expect } from '@playwright/test';

// Options of the palette's listbox (the page has other options, in selects).
const results = (page) => page.getByRole('listbox', { name: 'Keys' }).getByRole('option');

const openPalette = async (page) => {
  await page.keyboard.press('Control+k');
  await expect(page.locator('#command-palette')).toHaveAttribute('open');
};

test('palette is an ARIA combobox that tracks the selected option', async ({ page }) => {
  await page.goto('/');
  await openPalette(page);
  const input = page.getByRole('combobox', { name: 'Search keys' });
  await expect(input).toBeFocused();
  await expect(input).toHaveAttribute('aria-controls', 'command-results');
  await expect(page.getByRole('listbox', { name: 'Keys' })).toBeVisible();
  const options = results(page);
  await expect(options.first()).toBeVisible();
  await expect(input).toHaveAttribute('aria-expanded', 'true');

  const first = await options.nth(0).getAttribute('id');
  const second = await options.nth(1).getAttribute('id');
  await expect(input).toHaveAttribute('aria-activedescendant', first);
  await page.keyboard.press('ArrowDown');
  await expect(input).toHaveAttribute('aria-activedescendant', second);
  await expect(options.nth(1)).toHaveAttribute('aria-selected', 'true');
  await expect(options.nth(0)).toHaveAttribute('aria-selected', 'false');
  // Focus never leaves the input.
  await expect(input).toBeFocused();
});

test('reopening the palette does not follow a stale result', async ({ page }) => {
  await page.goto('/');
  await openPalette(page);
  const input = page.getByRole('combobox', { name: 'Search keys' });
  await input.fill('tailscale');
  await expect(results(page)).toHaveCount(1);
  // Close by clicking the backdrop, which leaves the field and results as
  // they were (Escape would clear the field and search again).
  await page.mouse.click(5, 5);
  await expect(page.locator('#command-palette')).not.toHaveAttribute('open');

  // Hold back the next /search so the old results would still be showing.
  let release;
  const gate = new Promise((resolve) => { release = resolve; });
  await page.route((url) => url.pathname === '/search', async (route) => { await gate; await route.continue(); });

  await openPalette(page);
  await expect(input).toHaveValue('');
  await expect(results(page)).toHaveCount(0);
  await expect(input).toHaveAttribute('aria-expanded', 'false');
  await expect(input).not.toHaveAttribute('aria-activedescendant', /.+/);
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL('/');

  release();
  await expect(results(page).first()).toBeVisible();
});
