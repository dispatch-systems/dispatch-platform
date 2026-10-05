import { test, expect, login } from '../../../../core/shell/tests/support/fixtures.js';

test('Settings shows a member the tabs they may use, and its address keeps the one chosen', async ({
  page,
}) => {
  await login(page, 'member@dispatch.test');
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  const tabs = page.getByRole('tablist', { name: 'Settings', exact: true });
  await expect(tabs.getByRole('tab')).toHaveText(['Profile', 'Security', 'Theme']);
  await expect(tabs.getByRole('tab', { name: 'Profile', exact: true })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await tabs.getByRole('tab', { name: 'Theme', exact: true }).click();
  await expect(page).toHaveURL(/\/settings\?tab=theme$/);
  await page.reload();
  await expect(
    page
      .getByRole('tablist', { name: 'Settings', exact: true })
      .getByRole('tab', { name: 'Theme', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
});
