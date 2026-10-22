import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';
import { connectFleet } from '../support/fleet.js';

test('Collect now on Collectors collects the fleet', async ({ page, dispatch }) => {
  await connectFleet(dispatch);
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page
    .getByRole('navigation', { name: 'Settings', exact: true })
    .getByRole('link', { name: 'Collectors', exact: true })
    .click();
  await page
    .getByRole('navigation', { name: 'Collections', exact: true })
    .getByRole('button', { name: /^Vans/ })
    .click();
  const fleet = page.getByRole('region', { name: 'Vans', exact: true });
  await expect(fleet.getByRole('definition').first()).toHaveText('Never');
  await fleet.getByRole('button', { name: 'Collect now', exact: true }).click();
  await expect(fleet.getByLabel('Succeeded', { exact: true })).toBeVisible({ timeout: 30_000 });
});
