import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';

test('Settings takes the sidebar over with a member’s pages, and its address keeps and moves the one open', async ({
  page,
}) => {
  await login(page, 'member@dispatch.test');
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  const pages = page.getByRole('navigation', { name: 'Settings', exact: true });
  const link = (name: string) => pages.getByRole('link', { name, exact: true });
  const heading = page.getByRole('heading', { level: 1 });
  // Its pages, under their group's name, in place of the app's.
  await expect(pages.getByRole('group', { name: 'Personal', exact: true })).toBeVisible();
  await expect(pages.getByRole('link')).toHaveText(['Profile', 'Security', 'Theme']);
  await expect(page.getByRole('navigation', { name: 'Primary navigation' })).toHaveCount(0);
  await expect(link('Profile')).toHaveAttribute('aria-current', 'page');
  await expect(heading).toHaveText('Profile');
  await link('Theme').click();
  await expect(page).toHaveURL(/#settings\?tab=theme$/);
  await expect(heading).toHaveText('Theme');
  await page.reload();
  await expect(link('Theme')).toHaveAttribute('aria-current', 'page');
  // A link to another of its pages, followed while Settings is open, opens that page.
  await page.evaluate(() => {
    location.hash = location.hash.replace(/tab=theme$/, 'tab=security');
  });
  await expect(link('Security')).toHaveAttribute('aria-current', 'page');
  await expect(heading).toHaveText('Security');
  // One the member lacks opens the first.
  await page.evaluate(() => {
    location.hash = location.hash.replace(/tab=security$/, 'tab=data');
  });
  await expect(link('Profile')).toHaveAttribute('aria-current', 'page');
});

test('Back leaves Settings for the page it was opened from, however many of its pages were open since', async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  const app = page.getByRole('navigation', { name: 'Primary navigation' });
  // A page other than the one the DSP opened on, which Back would lead to anyway.
  const other = app.getByRole('link').nth(1);
  await other.click();
  await expect(other).toHaveAttribute('aria-current', 'page');
  const from = page.url();
  await app.getByRole('link', { name: 'Settings', exact: true }).click();
  const heading = page.getByRole('heading', { level: 1 });
  await page
    .getByRole('navigation', { name: 'Settings', exact: true })
    .getByRole('link', { name: 'Security', exact: true })
    .click();
  await expect(heading).toHaveText('Security');
  // A link to another of its pages, as a feature's page has, moves the address as well.
  await page.evaluate(() => {
    location.hash = location.hash.replace(/tab=security$/, 'tab=theme');
  });
  await expect(heading).toHaveText('Theme');
  await page.getByRole('link', { name: 'Back', exact: true }).click();
  await expect(page).toHaveURL(from);
  await expect(other).toHaveAttribute('aria-current', 'page');
});

test('searching Settings finds a page by what it holds, and opening it points out the section', async ({
  page,
}) => {
  await login(page, 'member@dispatch.test');
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Profile');
  // "/" moves to the search from anywhere on the page but a field.
  await page.keyboard.press('/');
  const search = page.getByRole('combobox', { name: 'Search settings', exact: true });
  await expect(search).toBeFocused();
  // Security is known by its password too.
  await page.keyboard.type('passw');
  const found = page.getByRole('listbox', { name: 'Found', exact: true }).getByRole('option');
  await expect(found).toHaveCount(1);
  await expect(found).toContainText('Security');
  await expect(found.locator('mark')).toHaveText('Passw');
  await page.keyboard.press('Enter');
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Security');
  // Its section is outlined, and the address forgets it, so opening the page again doesn't.
  const section = page.locator('[data-settings-section="accounts.security"]');
  await expect(section).toHaveAttribute('data-found');
  await expect(page).toHaveURL(/#settings\?tab=security$/);
  // Anything done next lets it go: Escape clears the search, and the pages are back.
  await page.keyboard.press('Escape');
  await expect(section).not.toHaveAttribute('data-found');
  await expect(search).toHaveValue('');
  await expect(
    page
      .getByRole('navigation', { name: 'Settings', exact: true })
      .getByRole('link', { name: 'Security', exact: true }),
  ).toHaveAttribute('aria-current', 'page');
});
