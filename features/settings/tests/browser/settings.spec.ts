import { test, expect, login } from '../../../../core/shell/tests/support/fixtures.js';

test('Settings shows a member the pages they have, and its address keeps and moves the one open', async ({
  page,
}) => {
  await login(page, 'member@dispatch.test');
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  const tabs = page.getByRole('tablist', { name: 'Settings', exact: true });
  const tab = (name: string) => tabs.getByRole('tab', { name, exact: true });
  await expect(tabs.getByRole('tab')).toHaveText(['Profile', 'Security', 'Theme']);
  await expect(tab('Profile')).toHaveAttribute('aria-selected', 'true');
  await tab('Theme').click();
  await expect(page).toHaveURL(/#settings\?tab=theme$/);
  await page.reload();
  await expect(tab('Theme')).toHaveAttribute('aria-selected', 'true');
  // A link to another of its pages, followed while Settings is open, opens that page.
  await page.evaluate(() => {
    location.hash = location.hash.replace(/tab=theme$/, 'tab=security');
  });
  await expect(tab('Security')).toHaveAttribute('aria-selected', 'true');
  // One the member lacks opens the first.
  await page.evaluate(() => {
    location.hash = location.hash.replace(/tab=security$/, 'tab=data');
  });
  await expect(tab('Profile')).toHaveAttribute('aria-selected', 'true');
});
