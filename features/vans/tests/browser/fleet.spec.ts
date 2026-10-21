import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';
import { connectFleet } from '../support/fleet.js';

test('Collect now on the fleet bar collects the fleet', async ({ page, dispatch }) => {
  await connectFleet(dispatch);
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  await page
    .getByRole('navigation', { name: 'Settings', exact: true })
    .getByRole('link', { name: 'Vans', exact: true })
    .click();
  const fleet = page.locator('.vans-fleet');
  await expect(fleet).toContainText('Not collected yet');
  await fleet.getByRole('button', { name: 'Collect now', exact: true }).click();
  await expect(fleet).toContainText('Updated', { timeout: 30_000 });
});
