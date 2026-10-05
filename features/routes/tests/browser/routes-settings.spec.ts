import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';

test('a manager keeps route data until they choose a retention window', async ({ page }) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  await page.getByRole('tab', { name: 'Data', exact: true }).click();
  const panel = page.getByRole('region', { name: 'Route data', exact: true });
  await expect(panel.getByText('No days stored yet.', { exact: true })).toBeVisible();
  const keep = panel.getByLabel('Keep route data for');
  await expect(keep).toHaveValue('forever');
  const save = panel.getByRole('button', { name: 'Save', exact: true });
  await expect(save).toBeDisabled();
  // A custom window outside 30 to 3,650 days cannot be saved.
  await keep.selectOption('custom');
  await panel.getByLabel('Days', { exact: true }).fill('10');
  await expect(save).toBeDisabled();
  await panel.getByLabel('Days', { exact: true }).fill('120');
  await expect(save).toBeEnabled();
  await keep.selectOption('90');
  await save.click();
  await expect(page.getByText('Route data retention saved', { exact: true })).toBeVisible();
  await expect(save).toBeDisabled();
  await page.reload();
  await expect(
    page.getByRole('region', { name: 'Route data', exact: true }).getByLabel('Keep route data for'),
  ).toHaveValue('90');
  // Keeping every day again deletes nothing, so it needs no confirmation.
  await page
    .getByRole('region', { name: 'Route data', exact: true })
    .getByLabel('Keep route data for')
    .selectOption('forever');
  await page
    .getByRole('region', { name: 'Route data', exact: true })
    .getByRole('button', { name: 'Save', exact: true })
    .click();
  await expect(page.getByText('Route data retention saved', { exact: true }).first()).toBeVisible();
});

test('a member without management permissions never sees Data or Driver Match tabs', async ({
  page,
}) => {
  await login(page, 'member@dispatch.test');
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  await expect(page.getByRole('tab', { name: 'Profile', exact: true })).toBeVisible();
  await expect(page.getByRole('tab', { name: 'Data', exact: true })).toHaveCount(0);
  await expect(page.getByRole('tab', { name: /Driver Match/ })).toHaveCount(0);
});
