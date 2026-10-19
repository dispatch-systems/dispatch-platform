import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';

test('an open roster bar shows a collection that ran elsewhere, as a schedule runs it', async ({
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
    .getByRole('link', { name: 'Driver Profiles', exact: true })
    .click();
  const roster = page.locator('.driver-roster');
  await expect(roster).toContainText('Not collected yet');
  // Started by another session, as a schedule starts it, the page hears only that it finished.
  const job = await owner.post('/api/dsp/driver-profiles/collect', { requestId: 'elsewhere' });
  expect(job.status, job.body).toBe(202);
  await expect(roster).toContainText('Updated', { timeout: 30_000 });
});
