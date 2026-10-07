import { test, expect, login } from '../../../../core/shell/tests/support/fixtures.js';

// Weekly Scorecard has no page of its own: what its frontend draws is its events in the audit
// log, and its kinds of data on the Agents page, from the generated read toggles.

test('a week collected from the fixture reads in the audit log in Weekly Scorecard’s words', async ({
  page,
  dispatch,
}) => {
  const owner = await dispatch.client();
  const dsp = owner.session.dsps.find(
    (item: { name: string }) => item.name === 'Northline Logistics',
  );
  await owner.select(dsp.id);
  const cortex = { username: 'fixture@example.test', password: 'fixture-password' };
  expect((await owner.post('/api/dsp/connections/cortex', cortex)).value.status).toBe('ready');
  const profile = {
    name: dsp.name,
    abbreviation: 'NLL',
    stationCode: 'TST1',
    timezone: dsp.timezone,
  };
  expect((await owner.post('/api/dsp/profile', profile)).status).toBe(200);
  await owner.select(dsp.id);
  const week = { requestId: 'browser-week-38', week: '2026-W38' };
  expect((await owner.post('/api/dsp/weekly-scorecard/collect', week)).status).toBe(202);
  await login(page);
  await page.getByRole('link', { name: 'Audit log', exact: true }).click();
  await expect(
    page.getByRole('listitem').filter({ hasText: 'started a weekly scorecard collection' }),
  ).toContainText('for week 2026-W38');
});
