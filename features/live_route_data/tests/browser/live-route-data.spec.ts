import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';

// A demo DSP has Live Route Data off, as it reads a provider: the platform owner switches it on.

test('Live Route Data words the schedule its page of Settings refuses, and keeps the days and interval chosen', async ({
  page,
  dispatch,
}) => {
  const owner = await dispatch.client();
  const north = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  const switched = await owner.post(`/api/platform/dsps/${north.id}/features`, {
    feature: 'live_route_data',
    enabled: true,
  });
  expect(switched.status, switched.body).toBe(200);
  const kept = async () => {
    await owner.select(north.id);
    const settings = await owner.read('/api/dsp/settings');
    return settings.sections.find((s: { id: string }) => s.id === 'live_route_data.collection')
      .values;
  };

  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page
    .getByRole('navigation', { name: 'Primary navigation' })
    .getByRole('button', { name: 'Settings', exact: true })
    .click();
  await page
    .getByRole('navigation', { name: 'Settings', exact: true })
    .getByRole('link', { name: 'Live Route Data', exact: true })
    .click();
  const section = page.getByRole('region', { name: 'Collection', exact: true });
  const save = section.getByRole('button', { name: 'Save', exact: true });
  const stop = section.getByLabel('Stop', { exact: true });
  await expect(stop).toHaveValue('21:00');
  // A stop before its start would run past midnight.
  await stop.fill('07:00');
  await save.click();
  await expect(section.getByRole('alert')).toHaveText('Stop must be after start.');
  expect((await kept()).stop).toBe('21:00');
  await stop.fill('21:00');
  // Every day is chosen until the DSP chooses otherwise; with none, it would never read.
  const days = section.getByRole('group', { name: 'Days', exact: true }).getByRole('button');
  for (const day of await days.all()) {
    await expect(day).toHaveAttribute('aria-pressed', 'true');
    await day.click();
  }
  await save.click();
  await expect(section.getByRole('alert')).toHaveText('Choose at least one day.');
  expect((await kept()).days).toHaveLength(7);

  // Chosen out of order, the days are kept in the week's.
  await section.getByRole('button', { name: 'Wed', exact: true }).click();
  await section.getByRole('button', { name: 'Mon', exact: true }).click();
  await section.getByLabel('Every', { exact: true }).selectOption({ label: '15 min' });
  await save.click();
  await expect(page.getByText('Collection saved', { exact: true })).toBeVisible();
  const values = await kept();
  expect([values.days, values.every]).toEqual([['mon', 'wed'], '15']);
});

test('Live Route Data shows its last read, its next and its last error on its page of Settings', async ({
  page,
  dispatch,
}) => {
  const owner = await dispatch.client();
  const north = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  const switched = await owner.post(`/api/platform/dsps/${north.id}/features`, {
    feature: 'live_route_data',
    enabled: true,
  });
  expect(switched.status, switched.body).toBe(200);
  // Tomorrow alone, so nothing reads today and its next read is tomorrow's start, chosen before
  // it can read at all: it reads only at a station, with Cortex connected.
  await owner.select(north.id);
  const settings = await owner.read('/api/dsp/settings');
  const { revision } = settings.sections.find(
    (s: { id: string }) => s.id === 'live_route_data.collection',
  );
  const tomorrow = new Intl.DateTimeFormat('en-US', { timeZone: north.timezone, weekday: 'short' })
    .format(new Date(Date.now() + 86_400_000))
    .toLowerCase();
  const saved = await owner.post('/api/dsp/settings/live_route_data.collection', {
    values: { days: [tomorrow] },
    revision,
  });
  expect(saved.status, saved.body).toBe(200);
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
  // Its reads today: one that failed with a code the platform words, then one kept.
  const now = Date.now();
  dispatch.database('data/preview/jobs.sqlite', (db) => {
    db.exec('PRAGMA busy_timeout=5000');
    const read = db.prepare(
      `INSERT INTO live_reads(reader,dsp_id,day,at,ms,outcome,code,counts)
       VALUES ('live_route_data.routes',?,date('now'),?,0,?,?,?)`,
    );
    read.run(north.id, now - 10 * 60_000, 'failed', 'connection_busy', null);
    read.run(north.id, now - 5 * 60_000, 'kept', null, '{}');
  });

  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page
    .getByRole('navigation', { name: 'Primary navigation' })
    .getByRole('button', { name: 'Settings', exact: true })
    .click();
  await page
    .getByRole('navigation', { name: 'Settings', exact: true })
    .getByRole('link', { name: 'Live Route Data', exact: true })
    .click();
  const section = page.getByRole('region', { name: 'Collection', exact: true });
  const stat = (term: string) =>
    section
      .locator('dl')
      .filter({ has: page.getByRole('term').filter({ hasText: term }) })
      .getByRole('definition');
  await expect(stat('Last read').getByLabel('Kept', { exact: true })).toBeVisible();
  await expect(stat('Next read')).toContainText('8:00 AM');
  await expect(stat('Last error')).toContainText(
    'The browser is busy. Please try again in a moment.',
  );
});
