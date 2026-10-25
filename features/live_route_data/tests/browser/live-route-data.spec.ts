import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';

// A demo DSP has Live Route Data off, as it reads a provider: the platform owner switches it on.

test('Live Route Data words the window its page of Settings refuses, and keeps nothing', async ({
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
  const stop = section.getByLabel('Stop', { exact: true });
  await expect(stop).toHaveValue('21:00');
  // A stop before its start would run past midnight.
  await stop.fill('07:00');
  await section.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(section.getByRole('alert')).toHaveText('Stop must be after start.');
  await owner.select(north.id);
  const settings = await owner.read('/api/dsp/settings');
  const kept = settings.sections.find((s: { id: string }) => s.id === 'live_route_data.collection');
  expect(kept.values.stop).toBe('21:00');
});
