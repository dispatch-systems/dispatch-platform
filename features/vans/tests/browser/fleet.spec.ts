import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';

test('Collect now on the fleet bar collects the fleet, beside its schedules', async ({
  page,
  dispatch,
}) => {
  const owner = await dispatch.client();
  const north = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(north.id);
  const profile = { name: north.name, abbreviation: 'NLL', stationCode: 'TST1' };
  expect(
    (await owner.post('/api/dsp/profile', { ...profile, timezone: north.timezone })).status,
  ).toBe(200);
  await owner.select(north.id);
  const connected = await owner.post('/api/dsp/connections/cortex', {
    username: 'fixture@example.test',
    password: 'fixture-password',
  });
  expect(connected.status, connected.body).toBe(200);
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  await page
    .getByRole('navigation', { name: 'Settings', exact: true })
    .getByRole('link', { name: 'Vans', exact: true })
    .click();
  const collection = page.getByRole('region', { name: 'Collection', exact: true });
  await expect(collection.getByRole('button', { name: 'Add schedule' })).toBeVisible();
  const fleet = page.locator('.vans-fleet');
  await expect(fleet).toContainText('Not collected yet');
  await fleet.getByRole('button', { name: 'Collect now', exact: true }).click();
  await expect(fleet).toContainText('Updated', { timeout: 30_000 });
});
