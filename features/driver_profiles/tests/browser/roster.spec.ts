import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';

test('Collectors shows the end of a roster collection that ran elsewhere, as a rule runs it', async ({
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
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page
    .getByRole('navigation', { name: 'Settings', exact: true })
    .getByRole('link', { name: 'Collectors', exact: true })
    .click();
  await page
    .getByRole('navigation', { name: 'Collections', exact: true })
    .getByRole('button', { name: /^Roster/ })
    .click();
  const roster = page.getByRole('region', { name: 'Roster', exact: true });
  await expect(roster.getByRole('definition').first()).toHaveText('Never');
  // Started by another session, as a rule starts it, the page hears only that it finished.
  const now = { collects: 'associates', requestId: 'elsewhere' };
  const queued = await owner.post('/api/dsp/collectors/associates/collect', now);
  expect(queued.status, queued.body).toBe(202);
  await expect(roster.getByLabel('Succeeded', { exact: true })).toBeVisible({ timeout: 30_000 });
});
