import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';

test('Settings slides a member’s pages in for the app’s, and its address keeps and moves the one open', async ({
  page,
}) => {
  await login(page, 'member@dispatch.test');
  await page
    .getByRole('navigation', { name: 'Primary navigation' })
    .getByRole('button', { name: 'Settings', exact: true })
    .click();
  const pages = page.getByRole('navigation', { name: 'Settings', exact: true });
  const link = (name: string) => pages.getByRole('link', { name, exact: true });
  const heading = page.getByRole('heading', { level: 1 });
  // Its pages, under their group's name, in place of the app's, none open until one is opened.
  await expect(pages.getByRole('group', { name: 'Personal', exact: true })).toBeVisible();
  await expect(pages.getByRole('link')).toHaveText(['Profile', 'Security', 'Theme']);
  await expect(page.getByRole('navigation', { name: 'Primary navigation' })).toHaveCount(0);
  await expect(pages.locator('[aria-current="page"]')).toHaveCount(0);
  await link('Profile').click();
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

test("Settings and Back change the sidebar alone, and a page opened otherwise shows the sidebar it's under", async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  const app = page.getByRole('navigation', { name: 'Primary navigation' });
  const pages = page.getByRole('navigation', { name: 'Settings', exact: true });
  const settings = app.getByRole('button', { name: 'Settings', exact: true });
  const back = page.getByRole('button', { name: 'Back', exact: true });
  const heading = page.getByRole('heading', { level: 1 });
  // Opened over another page, Settings' pages slide in and that page stays open, as it does
  // once Back has the app's in again.
  const other = app.getByRole('link').nth(1);
  await other.click();
  await expect(other).toHaveAttribute('aria-current', 'page');
  const from = page.url();
  await settings.click();
  await expect(pages).toBeVisible();
  await expect(page).toHaveURL(from);
  await back.click();
  await expect(other).toHaveAttribute('aria-current', 'page');
  await expect(page).toHaveURL(from);
  // Back from a page of Settings keeps that page open, Settings the app's item highlighted.
  await settings.click();
  await pages.getByRole('link', { name: 'Security', exact: true }).click();
  await expect(heading).toHaveText('Security');
  await back.click();
  await expect(settings).toHaveAttribute('aria-current', 'page');
  await expect(heading).toHaveText('Security');
  // The browser's Back and Forward open pages, each with the sidebar it's under.
  await page.goBack();
  await expect(other).toHaveAttribute('aria-current', 'page');
  await page.goForward();
  await expect(pages.getByRole('link', { name: 'Security', exact: true })).toHaveAttribute(
    'aria-current',
    'page',
  );
});

test('searching Settings finds a page by what it holds, and opening it points out the section', async ({
  page,
}) => {
  await login(page, 'member@dispatch.test');
  await page
    .getByRole('navigation', { name: 'Primary navigation' })
    .getByRole('button', { name: 'Settings', exact: true })
    .click();
  const search = page.getByRole('combobox', { name: 'Search settings', exact: true });
  await expect(search).toBeVisible();
  // "/" moves to the search from anywhere on the page but a field.
  await page.keyboard.press('/');
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
